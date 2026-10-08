use sha2::{Digest, Sha256};
use std::{io::{Read, Write}, sync::atomic::{AtomicBool, Ordering}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChunkCodec { Zstd = 1, Xz = 2, DeltaZstd = 3, DeltaXz = 4, BrotliXz = 5, XzBrotli = 6 }

impl ChunkCodec {
    pub fn from_id(id: u32) -> Result<Self, String> {
        match id {
            1 => Ok(Self::Zstd), 2 => Ok(Self::Xz),
            3 => Ok(Self::DeltaZstd), 4 => Ok(Self::DeltaXz),
            5 => Ok(Self::BrotliXz), 6 => Ok(Self::XzBrotli),
            _ => Err(format!("Unsupported chunk codec ID {id}")),
        }
    }
}

pub struct EncodedChunk {
    pub data: Vec<u8>,
    pub source_sha256: [u8; 32],
    pub codec: ChunkCodec,
}

pub fn encode_chunk(source: &[u8], level: i32) -> Result<EncodedChunk, String> {
    let data = zstd::bulk::compress(source, level).map_err(|e| format!("Compress chunk: {e}"))?;
    Ok(EncodedChunk { data, source_sha256: Sha256::digest(source).into(), codec: ChunkCodec::Zstd })
}

fn delta_encode(source: &[u8]) -> Vec<u8> {
    let mut previous = 0u8;
    source.iter().map(|&byte| {
        let delta = byte.wrapping_sub(previous);
        previous = byte;
        delta
    }).collect()
}

fn delta_decode(bytes: &mut [u8]) {
    let mut previous = 0u8;
    for byte in bytes {
        previous = previous.wrapping_add(*byte);
        *byte = previous;
    }
}

fn xz_encode(source: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    encoder.write_all(source).map_err(|e| format!("Compress XZ chunk: {e}"))?;
    encoder.finish().map_err(|e| format!("Finish XZ chunk: {e}"))
}

fn xz_decode(data: &[u8], source_len: usize) -> Result<Vec<u8>, String> {
    let mut decoder = xz2::read::XzDecoder::new(data);
    let mut output = Vec::with_capacity(source_len);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = decoder.read(&mut buffer).map_err(|e| format!("Decompress XZ chunk: {e}"))?;
        if n == 0 { break; }
        if n > source_len - output.len() { return Err("XZ chunk expands beyond recorded size".into()); }
        output.extend_from_slice(&buffer[..n]);
    }
    if decoder.total_in() != data.len() as u64 { return Err("XZ chunk contains trailing data".into()); }
    Ok(output)
}

fn brotli_encode(source: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = brotli::CompressorReader::new(source, 4096, 11, 22);
    let mut output = Vec::new();
    encoder.read_to_end(&mut output).map_err(|e| format!("Compress Brotli chunk: {e}"))?;
    Ok(output)
}

fn brotli_decode(data: &[u8], max_len: usize) -> Result<Vec<u8>, String> {
    let mut decoder = brotli::Decompressor::new(data, 4096);
    let mut output = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = decoder.read(&mut buffer).map_err(|e| format!("Decompress Brotli chunk: {e}"))?;
        if n == 0 { break; }
        if n > max_len - output.len() { return Err("Brotli chunk expands beyond recorded size".into()); }
        output.extend_from_slice(&buffer[..n]);
    }
    Ok(output)
}

pub fn encode_combined_chunk(source: &[u8], cancel: &AtomicBool) -> Result<EncodedChunk, String> {
    let hash = Sha256::digest(source).into();
    let brotli = brotli_encode(source)?;
    if cancel.load(Ordering::Relaxed) { return Err("Operation cancelled".into()); }
    let brotli_xz = xz_encode(&brotli)?;
    if cancel.load(Ordering::Relaxed) { return Err("Operation cancelled".into()); }
    let xz = xz_encode(source)?;
    if cancel.load(Ordering::Relaxed) { return Err("Operation cancelled".into()); }
    let xz_brotli = brotli_encode(&xz)?;
    let (data, codec) = if brotli_xz.len() <= xz_brotli.len() {
        (brotli_xz, ChunkCodec::BrotliXz)
    } else { (xz_brotli, ChunkCodec::XzBrotli) };
    Ok(EncodedChunk { data, source_sha256: hash, codec })
}

pub fn encode_best_chunk(source: &[u8], zstd_level: i32, cancel: &AtomicBool) -> Result<EncodedChunk, String> {
    let hash = Sha256::digest(source).into();
    let mut best = EncodedChunk {
        data: zstd::bulk::compress(source, zstd_level).map_err(|e| format!("Compress chunk: {e}"))?,
        source_sha256: hash, codec: ChunkCodec::Zstd,
    };
    let transformed = delta_encode(source);
    for codec in [ChunkCodec::Xz, ChunkCodec::DeltaZstd, ChunkCodec::DeltaXz] {
        if cancel.load(Ordering::Relaxed) { return Err("Operation cancelled".into()); }
        let input = if matches!(codec, ChunkCodec::DeltaZstd | ChunkCodec::DeltaXz) { &transformed } else { source };
        let data = if matches!(codec, ChunkCodec::Xz | ChunkCodec::DeltaXz) {
            xz_encode(input)?
        } else {
            zstd::bulk::compress(input, zstd_level).map_err(|e| format!("Compress chunk: {e}"))?
        };
        if data.len() < best.data.len() { best.data = data; best.codec = codec; }
    }
    Ok(best)
}

pub fn decode_chunk(data: &[u8], source_len: usize, source_sha256: &[u8; 32]) -> Result<Vec<u8>, String> {
    decode_chunk_codec(data, source_len, source_sha256, ChunkCodec::Zstd)
}

pub fn decode_chunk_codec(data: &[u8], source_len: usize, source_sha256: &[u8; 32], codec: ChunkCodec) -> Result<Vec<u8>, String> {
    let intermediate_limit = source_len.checked_add(1024 * 1024).ok_or("Chunk size overflow")?;
    let mut source = match codec {
        ChunkCodec::Zstd | ChunkCodec::DeltaZstd => zstd::bulk::decompress(data, source_len).map_err(|e| format!("decompression failed: {e}"))?,
        ChunkCodec::Xz | ChunkCodec::DeltaXz => xz_decode(data, source_len)?,
        ChunkCodec::BrotliXz => {
            let inner = xz_decode(data, intermediate_limit)?;
            brotli_decode(&inner, source_len)?
        },
        ChunkCodec::XzBrotli => {
            let inner = brotli_decode(data, intermediate_limit)?;
            xz_decode(&inner, source_len)?
        },
    };
    if matches!(codec, ChunkCodec::DeltaZstd | ChunkCodec::DeltaXz) { delta_decode(&mut source); }
    if source.len() != source_len || Sha256::digest(&source).as_slice() != source_sha256 {
        return Err("SHA-256 mismatch".into());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::{brotli_encode, decode_chunk, decode_chunk_codec, encode_best_chunk, encode_chunk, encode_combined_chunk, xz_encode, ChunkCodec};
    use sha2::{Digest, Sha256};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn chunk_codec_accepts_non_disc_data() {
        let source = b"Generic bytes without a sector boundary";
        let chunk = encode_chunk(source, 1).unwrap();
        assert_eq!(decode_chunk(&chunk.data, source.len(), &chunk.source_sha256).unwrap(), source);
        let mut incorrect_hash = chunk.source_sha256;
        incorrect_hash[0] ^= 1;
        assert!(decode_chunk(&chunk.data, source.len(), &incorrect_hash).is_err());
    }

    #[test]
    fn best_size_roundtrips_and_checks_all_codec_ids() {
        let source = b"repeated disc bytes with a changing header 0123456789".repeat(600);
        let hash: [u8; 32] = Sha256::digest(&source).into();
        let best = encode_best_chunk(&source, 19, &AtomicBool::new(false)).unwrap();
        assert_eq!(decode_chunk_codec(&best.data, source.len(), &hash, best.codec).unwrap(), source);
        for codec in [ChunkCodec::Zstd, ChunkCodec::Xz, ChunkCodec::DeltaZstd, ChunkCodec::DeltaXz] {
            let transformed = if matches!(codec, ChunkCodec::DeltaZstd | ChunkCodec::DeltaXz) {
                super::delta_encode(&source)
            } else { source.clone() };
            let compressed = if matches!(codec, ChunkCodec::Xz | ChunkCodec::DeltaXz) {
                xz_encode(&transformed).unwrap()
            } else { zstd::bulk::compress(&transformed, 19).unwrap() };
            assert_eq!(decode_chunk_codec(&compressed, source.len(), &hash, codec).unwrap(), source);
            assert_eq!(ChunkCodec::from_id(codec as u32).unwrap(), codec);
        }
        assert!(ChunkCodec::from_id(99).is_err());
        let mut trailing = xz_encode(&source).unwrap(); trailing.push(1);
        assert!(decode_chunk_codec(&trailing, source.len(), &hash, ChunkCodec::Xz).is_err());
    }

    #[test]
    fn combined_brotli_xz_roundtrips_in_both_orders() {
        let source = b"PS1 sector data with repeated patterns 0123456789".repeat(500);
        let hash: [u8; 32] = Sha256::digest(&source).into();
        let brotli_xz = xz_encode(&brotli_encode(&source).unwrap()).unwrap();
        let xz_brotli = brotli_encode(&xz_encode(&source).unwrap()).unwrap();
        assert_eq!(decode_chunk_codec(&brotli_xz,source.len(),&hash,ChunkCodec::BrotliXz).unwrap(),source);
        assert_eq!(decode_chunk_codec(&xz_brotli,source.len(),&hash,ChunkCodec::XzBrotli).unwrap(),source);
        let best = encode_combined_chunk(&source,&AtomicBool::new(false)).unwrap();
        assert_eq!(best.data.len(),brotli_xz.len().min(xz_brotli.len()));
        assert!(matches!(best.codec,ChunkCodec::BrotliXz | ChunkCodec::XzBrotli));
        assert_eq!(decode_chunk_codec(&best.data,source.len(),&hash,best.codec).unwrap(),source);
        assert_eq!(ChunkCodec::from_id(5).unwrap(),ChunkCodec::BrotliXz);
        assert_eq!(ChunkCodec::from_id(6).unwrap(),ChunkCodec::XzBrotli);
    }
}
