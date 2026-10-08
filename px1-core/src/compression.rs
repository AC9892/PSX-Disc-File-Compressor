use sha2::{Digest, Sha256};

pub struct EncodedChunk {
    pub data: Vec<u8>,
    pub source_sha256: [u8; 32],
}

pub fn encode_chunk(source: &[u8], level: i32) -> Result<EncodedChunk, String> {
    let data = zstd::bulk::compress(source, level).map_err(|e| format!("Compress chunk: {e}"))?;
    Ok(EncodedChunk { data, source_sha256: Sha256::digest(source).into() })
}

pub fn decode_chunk(data: &[u8], source_len: usize, source_sha256: &[u8; 32]) -> Result<Vec<u8>, String> {
    let source = zstd::bulk::decompress(data, source_len).map_err(|e| format!("decompression failed: {e}"))?;
    if source.len() != source_len || Sha256::digest(&source).as_slice() != source_sha256 {
        return Err("SHA-256 mismatch".into());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::{decode_chunk, encode_chunk};

    #[test]
    fn chunk_codec_accepts_non_disc_data() {
        let source = b"Generic bytes without a sector boundary";
        let chunk = encode_chunk(source, 1).unwrap();
        assert_eq!(decode_chunk(&chunk.data, source.len(), &chunk.source_sha256).unwrap(), source);
        let mut incorrect_hash = chunk.source_sha256;
        incorrect_hash[0] ^= 1;
        assert!(decode_chunk(&chunk.data, source.len(), &incorrect_hash).is_err());
    }
}
