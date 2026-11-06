import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open, save } from '@tauri-apps/plugin-dialog';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import './styles.css';

type Disc = { platform?: string; media?: string; serial?: string; region?: string; volume_id?: string; boot_executable?: string; title?: string; disc_number?: number };
type Source = { cue_path: string; bin_path: string; referenced_bin_name: string; bin_size: number; cue_size: number; track_count: number; data_track_count: number; disc: Disc };
type Manifest = { format_version: number; format_minor?: number; created_utc: string; bin: { name: string; source_name?: string; size: number; sha256: string }; cue: { name: string; size: number; sha256: string }; disc: Disc; compression: string; zstd_level: number; chunk_size: number; chunk_count: number };
type PxInfo = { manifest: Manifest; px1_size: number; container_sha256: string };
type Report = PxInfo & { chunks_verified: number; container_verified: boolean; bin_verified: boolean; cue_verified: boolean };
type Progress = { stage: string; done: number; total: number; bytes_written: number; elapsed_seconds: number };
type Mode = 'compress' | 'extract' | 'verify';
type Ps2Source = { path: string; platform: string; media: string; format: string; serial?: string; volume_id?: string; title?: string; files: string[]; original_size: number; compressed_size?: number; confidence: string };
type ChdReport = { manifest: { disc: Ps2Source; files: { name: string; size: number; sha256: string }[]; chd_size: number; exact_restoration_verified: boolean }; verified: boolean; warning?: string };

document.querySelector<HTMLDivElement>('#app')!.innerHTML = `
  <nav class="modebar" aria-label="Mode">
    <button data-mode="compress" class="selected">Compress</button>
    <button data-mode="extract">Extract</button>
    <button data-mode="verify">Verify</button>
  </nav>
  <main>
    <section id="compressPane" class="page">
      <fieldset><legend>Source disc</legend>
        <div class="field pathfield"><label for="cuePath">Disc:</label><input id="cuePath" readonly placeholder="Select a CUE or ISO"><button id="browseCue" title="Browse for disc" aria-label="Browse for disc">…</button></div>
        <div class="field pathfield"><label for="binPath">BIN:</label><input id="binPath" readonly placeholder="Located from CUE"><button id="browseBin" title="Select BIN manually" aria-label="Select BIN manually">…</button></div>
        <div id="sourceSummary" class="properties muted">Select or drop a CUE or ISO file.</div>
      </fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="pxOutput">File:</label><input id="pxOutput" placeholder="Output .px1 path"><button id="browseOutput" title="Choose output PX1" aria-label="Choose output PX1">…</button></div>
        <div class="field shortfield"><label for="level">Compression:</label><select id="level"><option value="1">Fast</option><option value="6" selected>Balanced</option><option value="19">Maximum</option><option value="best">Best Size (slower)</option><option value="combined">Combine Best Two (Brotli + XZ)</option></select></div>
        <details id="advanced"><summary>Options…</summary>
          <div class="field shortfield"><label for="title">Title:</label><input id="title" placeholder="Optional description"></div>
          <div class="field numberfield"><label for="discNumber">Disc:</label><input id="discNumber" type="number" min="1" step="1" placeholder="Unknown"></div>
          <p class="note">Descriptions do not affect the preserved BIN/CUE bytes or hashes.</p>
        </details>
      </fieldset>
      <div class="pageActions"><button id="doCompress" class="defaultAction" disabled>Compress</button></div>
    </section>
    <section id="extractPane" class="page" hidden>
      <fieldset><legend>Source archive</legend>
        <div class="field pathfield"><label for="extractPath">File:</label><input id="extractPath" readonly placeholder="Select PX1 or CHD"><button id="browseExtract" title="Browse for archive" aria-label="Browse for archive">…</button></div>
      </fieldset>
      <fieldset><legend>Disc information</legend><div id="extractSummary" class="properties muted">Select or drop a PX1 file.</div></fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="extractDir">Folder:</label><input id="extractDir" readonly placeholder="Select output folder"><button id="browseDir" title="Choose output folder" aria-label="Choose output folder">…</button></div>
      </fieldset>
      <div class="pageActions"><button id="doExtract" class="defaultAction" disabled>Extract</button></div>
    </section>
    <section id="verifyPane" class="page" hidden>
      <fieldset><legend>Source archive</legend>
        <div class="field pathfield"><label for="verifyPath">File:</label><input id="verifyPath" readonly placeholder="Select PX1 or CHD"><button id="browseVerify" title="Browse for archive" aria-label="Browse for archive">…</button></div>
      </fieldset>
      <fieldset><legend>Disc information</legend><div id="verifySummary" class="properties muted">Verification checks the complete PX1 without extracting it.</div></fieldset>
      <div class="pageActions"><button id="doVerify" class="defaultAction" disabled>Verify</button></div>
    </section>
    <section id="ps2Pane" class="page" hidden>
      <fieldset><legend>Source disc</legend>
        <div class="field pathfield"><label for="ps2Path">Disc:</label><input id="ps2Path" readonly placeholder="Select ISO, CUE, or CHD"><button id="browsePs2">…</button></div>
        <div class="field pathfield"><label for="ps2Bin">BIN:</label><input id="ps2Bin" readonly placeholder="Located from CUE"><button id="browsePs2Bin">…</button></div>
        <div id="ps2Summary" class="properties muted">Select a PS2 disc image. Place chdman.exe beside the app or on PATH.</div>
        <div class="field shortfield"><label for="ps2Platform">Platform:</label><select id="ps2Platform"><option value="auto">Auto detect</option><option value="PlayStation">PlayStation (manual)</option><option value="PlayStation 2">PlayStation 2 (manual)</option></select></div>
      </fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="ps2Output">CHD:</label><input id="ps2Output" placeholder="Output .chd path"><button id="browsePs2Output">…</button></div>
      </fieldset>
      <div class="pageActions"><button id="doPs2Batch">Batch ISO/CUE</button><button id="doPs2Compress" class="defaultAction">Compress</button></div>
    </section>
    <section id="activity" class="activity" hidden aria-live="polite">
      <strong id="activityTitle">Working…</strong><progress id="progress" max="100" value="0"></progress>
      <div id="jobNumbers" class="activityDetails"></div>
      <div class="pageActions"><button id="cancel">Cancel</button></div>
    </section>
    <section id="result" class="result" hidden aria-live="polite"></section>
    <div id="resultActions" class="pageActions" hidden><button id="openFolder">Show in folder</button><button id="verifyResult">Verify PX1</button></div>
  </main>
  <footer>
    <section id="logPane" class="logPane" hidden>
      <div class="logHeader"><strong>Log</strong><div><button id="copyLog">Copy</button><button id="saveLog">Save</button><button id="clearLog">Clear</button></div></div>
      <pre id="log" aria-live="polite"></pre>
    </section>
    <div class="statusbar"><span id="status">Ready</span><button id="toggleLog" aria-expanded="false">Show Log</button></div>
  </footer>`;

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const input = (id: string) => el<HTMLInputElement>(id);
const text = (id: string, value: string) => { el<HTMLElement>(id).textContent = value; };
const size = (n: number) => n >= 1024 ** 3 ? `${(n / 1024 ** 3).toFixed(2)} GiB` : n >= 1024 ** 2 ? `${(n / 1024 ** 2).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;
const known = (v?: string | number) => v == null || v === '' ? 'Unknown' : String(v);
const compressionName = (manifest: Manifest) => manifest.compression === 'stacked' ? 'Brotli + XZ (both layers)' : manifest.compression === 'mixed' ? 'Best Size (zstd/XZ, optional delta)' : `Zstandard (level ${manifest.zstd_level})`;
const escapeHtml = (s: string) => s.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));
const base = (p: string) => p.split(/[\\/]/).pop() || p;
const parent = (p: string) => p.slice(0, Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/')));
const properties = (rows: [string, string][]) => `<dl>${rows.map(([key, value]) => `<dt>${escapeHtml(key)}</dt><dd>${escapeHtml(value)}</dd>`).join('')}</dl>`;

let mode: Mode = 'compress';
let source: Source | null = null;
let manualBinPath: string | null = null;
let extractInfo: PxInfo | null = null;
let verifyInfo: PxInfo | null = null;
let extractChd: Ps2Source | null = null;
let verifyChd: Ps2Source | null = null;
let activeSource: 'ps1' | 'ps2' = 'ps1';
let busy = false;
let lastOutput = '';
let cueLoadId = 0;
let extractLoadId = 0;
let verifyLoadId = 0;
let ps2Source: Ps2Source | null = null;
let ps2BinPath: string | null = null;

function log(message: string) {
  const pane = el<HTMLPreElement>('log');
  pane.textContent += `${new Date().toLocaleTimeString()}  ${message}\n`;
  pane.scrollTop = pane.scrollHeight;
}
function setStatus(value: string) { text('status', value); }
function updateButtons() {
  el<HTMLButtonElement>('doCompress').disabled = busy || !source || !input('pxOutput').value;
  el<HTMLButtonElement>('browseBin').disabled = busy || !input('cuePath').value;
  el<HTMLButtonElement>('doExtract').disabled = busy || (!extractInfo && !extractChd) || !input('extractDir').value;
  el<HTMLButtonElement>('doVerify').disabled = busy || (!verifyInfo && !verifyChd);
  el<HTMLButtonElement>('doPs2Compress').disabled = busy || !ps2Source || ps2Source.format === 'CHD' || ps2Source.platform === 'PlayStation' || !input('ps2Output').value;
  el<HTMLButtonElement>('doPs2Batch').disabled = busy;
  document.querySelectorAll<HTMLButtonElement>('.modebar button').forEach(button => button.disabled = busy);
}
function clearResult() {
  el<HTMLElement>('result').hidden = true;
  el<HTMLElement>('result').replaceChildren();
  el<HTMLElement>('resultActions').hidden = true;
}
function setMode(next: Mode) {
  mode = next;
  for (const name of ['compress', 'extract', 'verify'] as Mode[]) {
    el<HTMLElement>(`${name}Pane`).hidden = name !== next || (name === 'compress' && activeSource === 'ps2');
    document.querySelector<HTMLButtonElement>(`[data-mode="${name}"]`)!.classList.toggle('selected', name === next);
  }
  el<HTMLElement>('ps2Pane').hidden = next !== 'compress' || activeSource !== 'ps2';
  clearResult();
  setStatus('Ready');
  updateButtons();
}
function showResult(message: string, isError = false) {
  const result = el<HTMLElement>('result');
  result.hidden = false;
  result.classList.toggle('error', isError);
  result.textContent = message;
}

async function loadCue(path: string, selectedBin: string | null = null) {
  activeSource = 'ps1'; ps2Source = null; setMode('compress');
  const loadId = ++cueLoadId;
  if (input('cuePath').value !== path) {
    input('title').value = '';
    input('discNumber').value = '';
  }
  source = null;
  manualBinPath = selectedBin;
  input('cuePath').value = path;
  input('binPath').value = selectedBin || '';
  text('sourceSummary', 'Inspecting CUE…');
  clearResult();
  updateButtons();
  try {
    const inspected = await invoke<Source>('inspect_cue', { path, binPath: selectedBin });
    if (loadId !== cueLoadId) return;
    source = inspected;
    input('binPath').value = source.bin_path;
    input('pxOutput').value = path.replace(/\.cue$/i, '.px1');
    const rows: [string, string][] = [
      ['Platform:', known(source.disc.platform)], ['Serial:', known(source.disc.serial)],
      ['Region:', known(source.disc.region)], ['Image size:', size(source.bin_size)],
      ['Tracks:', `${source.data_track_count} data / ${source.track_count} total`],
      ['Disc files:', '1 BIN, 1 CUE'],
      ['Estimated PX1 space:', `up to ${size(source.bin_size + source.cue_size + 16 * 1024 * 1024)}`]
    ];
    if (base(source.bin_path) !== source.referenced_bin_name) {
      rows.push(['Restored BIN name:', source.referenced_bin_name]);
    }
    el<HTMLElement>('sourceSummary').innerHTML = properties(rows);
    setStatus(`Loaded ${base(path)}`);
    log(`Loaded ${base(path)}; located ${base(source.bin_path)} (${source.bin_size} bytes)`);
  } catch (error) {
    if (loadId !== cueLoadId) return;
    text('sourceSummary', String(error));
    setStatus('CUE could not be loaded');
    log(`CUE error: ${error}`);
  }
  updateButtons();
}

async function loadPx1(path: string, target: 'extract' | 'verify') {
  const loadId = target === 'extract' ? ++extractLoadId : ++verifyLoadId;
  input(target === 'extract' ? 'extractPath' : 'verifyPath').value = path;
  if (target === 'extract') { extractInfo = null; extractChd = null; } else { verifyInfo = null; verifyChd = null; }
  const summary = el<HTMLElement>(target === 'extract' ? 'extractSummary' : 'verifySummary');
  summary.textContent = 'Inspecting PX1…';
  clearResult();
  updateButtons();
  try {
    const info = await invoke<PxInfo>('inspect_px1', { path });
    if (loadId !== (target === 'extract' ? extractLoadId : verifyLoadId)) return;
    if (target === 'extract') { extractInfo = info; input('extractDir').value = parent(path); }
    else verifyInfo = info;
    const manifest = info.manifest;
    const rows: [string, string][] = target === 'verify' ? [
      ['Platform:', known(manifest.disc.platform)], ['Serial:', known(manifest.disc.serial)],
      ['Original size:', size(manifest.bin.size + manifest.cue.size)], ['PX1 size:', size(info.px1_size)]
    ] : [
      ['Platform:', known(manifest.disc.platform)], ['Media:', known(manifest.disc.media)],
      ['Serial:', known(manifest.disc.serial)],
      ['Region:', known(manifest.disc.region)], ['Title:', known(manifest.disc.title)],
      ['Disc:', known(manifest.disc.disc_number)],
      ['Original size:', size(manifest.bin.size + manifest.cue.size)], ['PX1 size:', size(info.px1_size)],
      ['Compression:', compressionName(manifest)],
      ['Restored files:', `${manifest.bin.name}, ${manifest.cue.name}`]
    ];
    summary.innerHTML = properties(rows) + `<details class="technical"><summary>Technical details</summary>${properties([
      ['Created:', manifest.created_utc], ['Format:', `PX1 v${manifest.format_version}.${manifest.format_minor ?? 0}`],
      ['Region:', known(manifest.disc.region)], ['Title:', known(manifest.disc.title)],
      ['Disc:', known(manifest.disc.disc_number)], ['Compression:', compressionName(manifest)],
      ['Restored files:', `${manifest.bin.name}, ${manifest.cue.name}`],
      ...(manifest.bin.source_name ? [['Source BIN label:', manifest.bin.source_name] as [string, string]] : []),
      ['Volume ID:', known(manifest.disc.volume_id)], ['Boot executable:', known(manifest.disc.boot_executable)],
      ['BIN SHA-256:', manifest.bin.sha256], ['CUE SHA-256:', manifest.cue.sha256],
      ['Container SHA-256:', info.container_sha256]
    ])}<p>Stored hashes; run Verify to check them.</p></details>`;
    setStatus(`Loaded ${base(path)}`);
    log(`Loaded ${base(path)} (${info.px1_size} bytes)`);
  } catch (error) {
    if (loadId !== (target === 'extract' ? extractLoadId : verifyLoadId)) return;
    summary.textContent = String(error);
    setStatus('PX1 could not be loaded');
    log(`PX1 error: ${error}`);
  }
  updateButtons();
}

async function run<T>(name: string, args: Record<string, unknown>, success: (report: T) => void) {
  busy = true;
  clearResult();
  el<HTMLElement>('activity').hidden = false;
  el<HTMLButtonElement>('cancel').hidden = name.endsWith('_ps2');
  el<HTMLProgressElement>('progress').value = 0;
  text('activityTitle', name.endsWith('_ps2') ? 'Running chdman and checking restored hashes…' : 'Starting…');
  text('jobNumbers', '');
  setStatus('Starting…');
  updateButtons();
  log(`${name.replace('_', ' ')} started`);
  try {
    const report = await invoke<T>(name, args);
    success(report);
    log(`${name.replace('_', ' ')} completed successfully`);
  } catch (error) {
    setStatus('Operation failed');
    showResult(String(error), true);
    log(`Failed: ${error}`);
  } finally {
    busy = false;
    el<HTMLElement>('activity').hidden = true;
    updateButtons();
  }
}

document.querySelectorAll<HTMLButtonElement>('.modebar button').forEach(button => button.onclick = () => setMode(button.dataset.mode as Mode));
async function loadDisc(path: string) {
  if (/\.cue$/i.test(path)) {
    try {
      const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath: null });
      if (disc.platform === 'PlayStation') { await loadCue(path); return; }
      await loadPs2(path); return;
    } catch { await loadCue(path); return; }
  }
  if (/\.iso$/i.test(path)) { await loadPs2(path); return; }
  if (/\.chd$/i.test(path)) { setMode('extract'); await loadArchive(path, 'extract'); }
}
el<HTMLButtonElement>('browseCue').onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Disc image', extensions: ['cue', 'iso'] }] }); if (typeof path === 'string') await loadDisc(path); };
el<HTMLButtonElement>('browseBin').onclick = async () => {
  const cuePath = input('cuePath').value;
  if (!cuePath) return;
  const path = await open({ multiple: false, filters: [{ name: 'BIN image', extensions: ['bin'] }] });
  if (typeof path === 'string') {
    try {
      const disc = await invoke<Ps2Source>('inspect_ps2', { path: cuePath, binPath: path });
      if (disc.platform === 'PlayStation 2') { await loadPs2(cuePath, path); return; }
    } catch { /* Show the existing PX1 import error below. */ }
    await loadCue(cuePath, path);
  }
};
el<HTMLButtonElement>('browseOutput').onclick = async () => { const path = await save({ defaultPath: input('pxOutput').value || undefined, filters: [{ name: 'PX1 disc', extensions: ['px1'] }] }); if (path) { input('pxOutput').value = path; updateButtons(); } };
async function loadArchive(path: string, target: 'extract' | 'verify') {
  if (!/\.chd$/i.test(path)) { await loadPx1(path, target); return; }
  const summary = el<HTMLElement>(target === 'extract' ? 'extractSummary' : 'verifySummary');
  input(target === 'extract' ? 'extractPath' : 'verifyPath').value = path;
  if (target === 'extract') { extractInfo = null; extractChd = null; input('extractDir').value = parent(path); }
  else { verifyInfo = null; verifyChd = null; }
  summary.textContent = 'Inspecting CHD…'; updateButtons();
  try {
    const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath: null });
    if (target === 'extract') extractChd = disc; else verifyChd = disc;
    summary.innerHTML = properties([
      ['Platform:', disc.platform], ['Disc type:', disc.media], ['Original format:', 'CHD'],
      ['Original size:', size(disc.original_size)], ['CHD size:', size(disc.compressed_size ?? disc.original_size)],
      ['Verification:', disc.confidence.includes('no original-hash') ? 'Original hashes unavailable' : 'Run Verify to check CHD integrity']
    ]);
  } catch (error) { summary.textContent = String(error); }
  updateButtons();
}
el<HTMLButtonElement>('browseExtract').onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Disc archive', extensions: ['px1', 'chd'] }] }); if (typeof path === 'string') await loadArchive(path, 'extract'); };
el<HTMLButtonElement>('browseVerify').onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Disc archive', extensions: ['px1', 'chd'] }] }); if (typeof path === 'string') await loadArchive(path, 'verify'); };
el<HTMLButtonElement>('browseDir').onclick = async () => { const path = await open({ directory: true, multiple: false }); if (typeof path === 'string') { input('extractDir').value = path; updateButtons(); } };
async function loadPs2(path: string, binPath: string | null = null) {
  activeSource = 'ps2'; source = null; setMode('compress');
  ps2Source = null; ps2BinPath = binPath; input('ps2Path').value = path; input('ps2Bin').value = binPath || '';
  text('ps2Summary', 'Inspecting disc…');
  try {
    const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath });
    ps2Source = disc;
    if (disc.format === 'BIN/CUE') input('ps2Bin').value = disc.files[0];
    input('ps2Output').value = path.replace(/\.[^.\\/]+$/, '.chd');
    el<HTMLElement>('ps2Summary').innerHTML = properties([
      ['Platform:', disc.platform], ['Disc type:', disc.media], ['Original format:', disc.format],
      ['Title:', known(disc.title)], ['Serial:', known(disc.serial)], ['Volume:', known(disc.volume_id)], ['Original size:', size(disc.original_size)],
      ['Compression format:', disc.format === 'CHD' ? 'CHD' : 'CHD (planned output)'],
      ['Compressed size:', disc.compressed_size == null ? 'Not compressed yet' : size(disc.compressed_size)],
      ['Ratio:', disc.compressed_size == null ? 'Unknown' : `${(disc.compressed_size / disc.original_size * 100).toFixed(1)}%`],
      ['Storage change:', disc.compressed_size == null ? 'Unknown' : disc.compressed_size <= disc.original_size ? `Saved ${size(disc.original_size - disc.compressed_size)}` : `Larger by ${size(disc.compressed_size - disc.original_size)}`],
      ['Verification:', disc.format === 'CHD' ? 'Run Verify CHD' : 'Not compressed yet'],
      ['Detection:', disc.confidence]
    ]) + (disc.platform === 'PlayStation' && disc.format === 'ISO' ? '<p class="note">PS1 ISO is not supported; select its BIN/CUE dump.</p>' : '');
    updateButtons();
  } catch (e) { text('ps2Summary', String(e)); }
  updateButtons();
}
el<HTMLButtonElement>('browsePs2').onclick = async () => { const p = await open({ multiple: false, filters: [{ name: 'Disc image', extensions: ['iso', 'cue', 'chd'] }] }); if (typeof p === 'string') await loadDisc(p); };
el<HTMLSelectElement>('ps2Platform').onchange = () => { if (input('ps2Platform').value === 'PlayStation' && ps2Source?.format === 'BIN/CUE') void loadCue(ps2Source.path, ps2BinPath); };
el<HTMLButtonElement>('browsePs2Bin').onclick = async () => { const p = await open({ multiple: false, filters: [{ name: 'BIN image', extensions: ['bin'] }] }); if (typeof p === 'string' && input('ps2Path').value) await loadPs2(input('ps2Path').value, p); };
el<HTMLButtonElement>('browsePs2Output').onclick = async () => { const p = await save({ defaultPath: input('ps2Output').value || undefined, filters: [{ name: 'CHD image', extensions: ['chd'] }] }); if (p) { input('ps2Output').value = p; updateButtons(); } };
el<HTMLButtonElement>('doPs2Compress').onclick = () => {
  if (!ps2Source || !input('ps2Output').value) return;
  const source = { ...ps2Source, platform: input('ps2Platform').value === 'auto' ? ps2Source.platform : 'PlayStation 2' };
  void run<ChdReport>('compress_ps2', { source, outputPath: input('ps2Output').value }, report => {
    const original = report.manifest.files.reduce((sum, f) => sum + f.size, 0);
    showResult(`CHD: ${size(report.manifest.chd_size)} / Original: ${size(original)}   Ratio: ${(report.manifest.chd_size / original * 100).toFixed(1)}%   ${report.warning || 'Exact restoration verified'}`);
    lastOutput = input('ps2Output').value; el<HTMLElement>('resultActions').hidden = false; el<HTMLButtonElement>('verifyResult').hidden = false; text('verifyResult', 'Verify CHD');
  });
};
el<HTMLButtonElement>('doPs2Batch').onclick = async () => {
  const paths = await open({ multiple: true, filters: [{ name: 'PS2 ISO or CUE', extensions: ['iso', 'cue'] }] });
  if (!Array.isArray(paths) || paths.length === 0) return;
  const outputDir = await open({ directory: true, multiple: false }); if (typeof outputDir !== 'string') return;
  busy = true; updateButtons();
  for (const p of paths) {
    try {
      const source = await invoke<Ps2Source>('inspect_ps2', { path: p, binPath: null });
      if (source.platform !== 'PlayStation 2') { log(`Skipped unidentified disc: ${base(p)}`); continue; }
      const outputPath = outputDir.replace(/[\\/]$/, '') + '\\' + base(p).replace(/\.[^.]+$/, '.chd');
      const report = await invoke<ChdReport>('compress_ps2', { source, outputPath });
      log(`${base(p)}: ${size(report.manifest.chd_size)}; ${report.warning || 'exact'}`);
    } catch (e) { log(`${base(p)}: ${e}`); }
  }
  busy = false; updateButtons(); showResult('Batch finished. See Log for each disc and any warnings.');
};
input('pxOutput').oninput = updateButtons;

el<HTMLButtonElement>('doCompress').onclick = () => {
  if (!source) return;
  const output = input('pxOutput').value;
  const number = Number(input('discNumber').value);
  const bestSize = input('level').value === 'best';
  const combined = input('level').value === 'combined';
  lastOutput = output;
  void run<Report>('compress_disc', { cuePath: source.cue_path, binPath: manualBinPath, outputPath: output, level: bestSize || combined ? 19 : Number(input('level').value), bestSize, combined, title: input('title').value || null, discNumber: Number.isInteger(number) && number > 0 ? number : null }, report => {
    const original = report.manifest.bin.size + report.manifest.cue.size;
    const saved = original - report.px1_size;
    const change = saved >= 0 ? `Saved: ${size(saved)}` : `Larger: ${size(-saved)}`;
    setStatus('Compression complete; verification passed');
    showResult(`Original: ${size(original)}   PX1: ${size(report.px1_size)}   ${change}   Ratio: ${(report.px1_size / original * 100).toFixed(1)}%   BIN/CUE SHA-256: PASS`);
    el<HTMLElement>('resultActions').hidden = false;
    el<HTMLButtonElement>('verifyResult').hidden = false;
    text('verifyResult', 'Verify PX1');
  });
};
el<HTMLButtonElement>('doExtract').onclick = () => {
  if (extractChd) {
    void run<ChdReport>('extract_ps2', { path: input('extractPath').value, outputDir: input('extractDir').value }, report => {
      setStatus(report.verified ? 'Restoration matched original hashes' : 'Restoration completed with warning');
      showResult(report.verified ? 'Restored files match original SHA-256 hashes byte for byte.' : report.warning || 'Original bytes could not be proven.');
      lastOutput = input('extractDir').value; el<HTMLElement>('resultActions').hidden = false; el<HTMLButtonElement>('verifyResult').hidden = true;
    });
    return;
  }
  if (!extractInfo) return;
  void run<Report>('extract_disc', { path: input('extractPath').value, outputDir: input('extractDir').value }, report => {
    lastOutput = input('extractDir').value.replace(/[\\/]$/, '') + '\\' + report.manifest.bin.name;
    setStatus('Extraction complete; BIN and CUE verified');
    showResult(`BIN: VERIFIED   CUE: VERIFIED   Chunks: ${report.chunks_verified}`);
    el<HTMLElement>('resultActions').hidden = false;
    el<HTMLButtonElement>('verifyResult').hidden = true;
  });
};
el<HTMLButtonElement>('doVerify').onclick = () => {
  if (verifyChd) {
    void run<ChdReport>('verify_ps2', { path: input('verifyPath').value }, report => {
      setStatus('CHD integrity verified'); showResult(report.warning || 'CHD integrity verified; original bytes matched at creation.');
    });
    return;
  }
  if (!verifyInfo) return;
  void run<Report>('verify_disc', { path: input('verifyPath').value }, report => {
    setStatus('Verification passed');
    const result = el<HTMLElement>('result');
    result.hidden = false;
    result.classList.remove('error');
    result.innerHTML = properties([
      ['Header:', 'PASS'], ['Manifest:', 'PASS'], ['Chunks:', `${report.chunks_verified} / ${report.manifest.chunk_count}`],
      ['BIN SHA-256:', 'PASS'], ['CUE SHA-256:', 'PASS'], ['Container:', 'PASS'], ['Overall:', 'PASS']
    ]);
  });
};
el<HTMLButtonElement>('cancel').onclick = () => { void invoke('cancel_job'); setStatus('Cancelling…'); log('Cancellation requested'); };
el<HTMLButtonElement>('openFolder').onclick = () => { void revealItemInDir(lastOutput); };
el<HTMLButtonElement>('verifyResult').onclick = () => { setMode('verify'); void loadArchive(lastOutput, 'verify'); };
el<HTMLButtonElement>('toggleLog').onclick = () => {
  const pane = el<HTMLElement>('logPane');
  pane.hidden = !pane.hidden;
  text('toggleLog', pane.hidden ? 'Show Log' : 'Hide Log');
  el<HTMLButtonElement>('toggleLog').setAttribute('aria-expanded', String(!pane.hidden));
};
el<HTMLButtonElement>('copyLog').onclick = () => { void navigator.clipboard.writeText(el<HTMLPreElement>('log').textContent || ''); };
el<HTMLButtonElement>('saveLog').onclick = async () => { const path = await save({ defaultPath: 'px1-log.txt', filters: [{ name: 'Text', extensions: ['txt'] }] }); if (path) { try { await invoke('save_log', { path, content: el<HTMLPreElement>('log').textContent || '' }); log(`Saved log to ${path}`); } catch (error) { log(`Could not save log: ${error}`); } } };
el<HTMLButtonElement>('clearLog').onclick = () => { text('log', ''); };

void listen<Progress>('px1-progress', event => {
  const progress = event.payload;
  const byteStage = ['Hashing source', 'Sealing container', 'Checking container'].includes(progress.stage);
  text('activityTitle', progress.stage);
  setStatus(progress.total ? byteStage ? `${progress.stage}: ${size(progress.done)} / ${size(progress.total)}` : `${progress.stage} chunk ${progress.done} / ${progress.total}` : progress.stage);
  el<HTMLProgressElement>('progress').value = progress.total ? Math.round(progress.done / progress.total * 100) : 0;
  if (progress.stage === 'Compressing' && source) {
    const read = Math.min(source.bin_size, progress.done * 4 * 1024 * 1024);
    const speed = progress.elapsed_seconds ? read / progress.elapsed_seconds : 0;
    const remaining = progress.done && progress.elapsed_seconds ? Math.ceil((progress.total - progress.done) * progress.elapsed_seconds / progress.done) : 0;
    text('jobNumbers', `${size(read)} / ${size(source.bin_size)}     ${size(speed)}/s     ~${remaining}s remaining     Written ${size(progress.bytes_written)}`);
  } else if (byteStage) {
    const speed = progress.elapsed_seconds ? progress.done / progress.elapsed_seconds : 0;
    text('jobNumbers', `${size(progress.done)} / ${size(progress.total)}${speed ? `     ${size(speed)}/s` : ''}`);
  }
  else text('jobNumbers', progress.total ? `${progress.done} / ${progress.total} chunks` : '');
});
void getCurrentWebview().onDragDropEvent(event => {
  if (event.payload.type !== 'drop' || busy) return;
  const path = event.payload.paths[0];
  if (!path) return;
  if (/\.(cue|iso)$/i.test(path)) { setMode('compress'); void loadDisc(path); }
  else if (/\.(px1|chd)$/i.test(path)) { const target = mode === 'verify' ? 'verify' : 'extract'; setMode(target); void loadArchive(path, target); }
});
log('Ready. Select a CUE, ISO, PX1, or CHD file.');
