import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { ask, open, save } from '@tauri-apps/plugin-dialog';
import { revealItemInDir, openUrl } from '@tauri-apps/plugin-opener';
import './styles.css';

type Disc = { platform?: string; media?: string; serial?: string; region?: string; volume_id?: string; boot_executable?: string; title?: string; disc_number?: number };
type Source = { cue_path: string; bin_path: string; referenced_bin_name: string; bin_size: number; cue_size: number; track_count: number; data_track_count: number; disc: Disc };
type Manifest = { format_version: number; format_minor?: number; created_utc: string; bin: { name: string; source_name?: string; size: number; sha256: string }; cue: { name: string; size: number; sha256: string }; disc: Disc; compression: string; zstd_level: number; chunk_size: number; chunk_count: number };
type PxInfo = { manifest: Manifest; px1_size: number; container_sha256: string };
type Report = PxInfo & { chunks_verified: number; container_verified: boolean; bin_verified: boolean; cue_verified: boolean };
type Progress = { stage: string; done: number; total: number; bytes_written: number; elapsed_seconds: number };
type Mode = 'compress' | 'extract' | 'verify' | 'settings';
type Ps2Source = { path: string; platform: string; media: string; format: string; serial?: string; volume_id?: string; title?: string; files: string[]; original_size: number; compressed_size?: number; confidence: string };
type ChdReport = { manifest: { disc: Ps2Source; files: { name: string; size: number; sha256: string }[]; chd_size: number; exact_restoration_verified: boolean }; verified: boolean; warning?: string };
type Ps3Info = { path: string; platform: string; media: string; format: string; original_name: string; original_size: number; archive_size?: number; sha256?: string; title_id?: string; title?: string; confidence: string };
type Ps3Report = { info: Ps3Info; verified: boolean };

document.querySelector<HTMLDivElement>('#app')!.innerHTML = `
  <nav class="modebar" aria-label="Mode">
    <button data-mode="compress" class="selected">Compress</button>
    <button data-mode="extract">Extract</button>
    <button data-mode="verify">Verify</button>
    <button data-mode="settings" class="settingsButton">Settings</button>
  </nav>
  <main>
  <section id="settingsPane" class="settingsDialog page" aria-label="Game Information and Artwork settings" hidden><h2>Game Information &amp; Artwork</h2>
    <fieldset><legend>Game detection</legend>
      <label><input type="checkbox" id="settingDetect"> Enable automatic game detection</label>
      <label><input type="checkbox" id="settingPanel"> Show game information panel</label>
      <label><input type="checkbox" id="settingRegion"> Identify region and disc number</label>
    </fieldset>
    <fieldset><legend>Artwork</legend>
      <label><input type="checkbox" id="settingArtwork"> Enable game cover artwork</label>
      <label><input type="checkbox" id="settingDownload"> Automatically download missing covers</label>
      <label><input type="checkbox" id="settingPreferLocal"> Prefer locally stored artwork</label>
      <label><input type="checkbox" id="settingCache"> Enable artwork caching</label>
      <label>Preferred style <select id="settingStyle"><option value="default">Flat front cover</option></select></label>
      <div class="field pathfield"><label for="settingsArtDir">Folder:</label><input id="settingsArtDir" readonly><button id="settingsBrowseArt">…</button></div>
      <p id="settingsArtSize" class="muted"></p>
      <div class="settingsActions"><button id="downloadArt">Download missing artwork</button><button id="clearArt">Clear artwork cache</button></div>
    </fieldset>
  </section>
    <div id="workspace" class="workspace"><div class="workLeft">
    <div class="supportedFormats" aria-label="Supported PlayStation disc formats">
      <div><strong>PS1</strong><span>BIN/CUE → PX1</span></div>
      <div><strong>PS2</strong><span>DVD ISO · CD BIN/CUE → CHD</span></div>
      <div><strong>PS3</strong><span>Blu-ray ISO → PS3Z</span></div>
      <small>One BIN per CUE</small>
    </div>
    <details class="artworkSettings"><summary>Artwork <span id="artworkSize">(separate local collection)</span></summary>
      <div class="field pathfield"><label for="artworkDir">Folder:</label><input id="artworkDir" readonly><button id="browseArtworkDir" title="Choose artwork folder">…</button></div>
      <small>PS1/PS2 covers stay local. PS3 covers come from <button id="artworkProvider" class="textButton">GameTDB</button> and are cached for offline use.</small>
    </details>
    <section id="compressPane" class="page">
      <fieldset><legend>Source disc</legend>
        <div class="field pathfield"><label for="cuePath">Disc:</label><input id="cuePath" readonly placeholder="Select a CUE or ISO"><button id="browseCue" title="Browse for disc" aria-label="Browse for disc">…</button></div>
        <div class="field pathfield"><label for="binPath">BIN:</label><input id="binPath" readonly placeholder="Located from CUE"><button id="browseBin" title="Select BIN manually" aria-label="Select BIN manually">…</button></div>
        <div id="sourceSummary" class="properties muted">Select or drop a CUE or ISO file.</div>
        <div id="ps1Artwork" class="artworkBox" hidden></div>
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
      <fieldset><legend>Disc information</legend><div id="extractSummary" class="properties muted">Select or drop a PX1 file.</div><div id="extractArtwork" class="artworkBox" hidden></div></fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="extractDir">Folder:</label><input id="extractDir" readonly placeholder="Select output folder"><button id="browseDir" title="Choose output folder" aria-label="Choose output folder">…</button></div>
      </fieldset>
      <div class="pageActions"><button id="doExtract" class="defaultAction" disabled>Extract</button></div>
    </section>
    <section id="verifyPane" class="page" hidden>
      <fieldset><legend>Source archive</legend>
        <div class="field pathfield"><label for="verifyPath">File:</label><input id="verifyPath" readonly placeholder="Select PX1 or CHD"><button id="browseVerify" title="Browse for archive" aria-label="Browse for archive">…</button></div>
      </fieldset>
      <fieldset><legend>Disc information</legend><div id="verifySummary" class="properties muted">Verification checks the complete PX1 without extracting it.</div><div id="verifyArtwork" class="artworkBox" hidden></div></fieldset>
      <div class="pageActions"><button id="doVerify" class="defaultAction" disabled>Verify</button></div>
    </section>
    <section id="ps2Pane" class="page" hidden>
      <fieldset><legend>Source disc</legend>
        <div class="field pathfield"><label for="ps2Path">Disc:</label><input id="ps2Path" readonly placeholder="Select ISO, CUE, or CHD"><button id="browsePs2">…</button></div>
        <div class="field pathfield"><label for="ps2Bin">BIN:</label><input id="ps2Bin" readonly placeholder="Located from CUE"><button id="browsePs2Bin">…</button></div>
        <div id="ps2Summary" class="properties muted">Select a PS2 disc image. Place chdman.exe beside the app or on PATH.</div>
        <div id="ps2Artwork" class="artworkBox" hidden></div>
        <div class="field shortfield"><label for="ps2Platform">Platform:</label><select id="ps2Platform"><option value="auto">Auto detect</option><option value="PlayStation">PlayStation (manual)</option><option value="PlayStation 2">PlayStation 2 (manual)</option><option value="PlayStation 3">PlayStation 3 (validate ISO)</option></select></div>
      </fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="ps2Output">CHD:</label><input id="ps2Output" placeholder="Output .chd path"><button id="browsePs2Output">…</button></div>
      </fieldset>
      <div class="pageActions"><button id="doPs2Batch">Batch ISO/CUE</button><button id="doPs2Compress" class="defaultAction">Compress</button></div>
    </section>
    <section id="ps3Pane" class="page" hidden>
      <fieldset><legend>Source disc</legend>
        <div class="field pathfield"><label for="ps3Path">ISO:</label><input id="ps3Path" readonly placeholder="Select a PS3 disc ISO"><button id="browsePs3">…</button></div>
        <div id="ps3Summary" class="properties muted">Select a PS3 ISO.</div>
        <div class="field shortfield"><label for="ps3ArtId">Art ID:</label><input id="ps3ArtId" placeholder="Detected title ID or optional manual ID" maxlength="9"></div>
        <div id="ps3Artwork" class="artworkBox" hidden></div>
      </fieldset>
      <fieldset><legend>Output</legend>
        <div class="field pathfield"><label for="ps3Output">File:</label><input id="ps3Output" placeholder="Output .ps3z path"><button id="browsePs3Output">…</button></div>
      </fieldset>
      <div class="pageActions"><button id="doPs3Compress" class="defaultAction" disabled>Compress</button></div>
    </section>
    </div><aside id="gamePanel" class="gamePanel" aria-label="Game information">
      <div id="coverPlaceholder" class="coverPlaceholder" aria-hidden="true">PS</div>
      <div id="gameArtwork" class="artworkBox"></div>
      <h2 id="gameTitle">PlayStation disc</h2>
      <p id="gameHint" class="muted">Select a disc to view game information.</p>
      <div id="gameDetails" class="gameDetails"></div>
      <details id="manualGame" hidden><summary>Identify manually</summary>
        <label>Title <input id="manualTitle" placeholder="Game title"></label>
        <label>Console <select id="manualPlatform"><option value="">Unknown</option><option value="PlayStation">PlayStation</option><option value="PlayStation 2">PlayStation 2</option><option value="PlayStation 3">PlayStation 3</option></select></label>
        <button id="saveManualGame">Apply</button>
      </details>
    </aside></div>
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
  </footer>
  <dialog id="coverDetail"><button id="closeCoverDetail">Close</button><img id="fullCover" alt="Full cover artwork"></dialog>`;

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
let extractPs3: Ps3Info | null = null;
let verifyPs3: Ps3Info | null = null;
let ps3Source: Ps3Info | null = null;
let activeSource: 'ps1' | 'ps2' | 'ps3' = 'ps1';
let busy = false;
let lastOutput = '';
let cueLoadId = 0;
let extractLoadId = 0;
let verifyLoadId = 0;
let ps2LoadId = 0;
let ps3LoadId = 0;
let discSelectionId = 0;
let ps2Source: Ps2Source | null = null;
let ps2BinPath: string | null = null;
let artworkRoot = localStorage.getItem('artworkRoot') || '';
type Settings = { detect: boolean; panel: boolean; region: boolean; artwork: boolean; download: boolean; preferLocal: boolean; cache: boolean };
const defaults: Settings = { detect: true, panel: true, region: true, artwork: true, download: false, preferLocal: true, cache: true };
let settings: Settings = { ...defaults };
try { settings = { ...defaults, ...JSON.parse(localStorage.getItem('gameSettings') || '{}') }; } catch { /* Ignore invalid older settings. */ }
type GameInfo = { title?: string; platform?: string; serial?: string; region?: string; disc?: string; format?: string; bytes?: number; artworkId?: string };
const gameStates: Record<Mode, GameInfo | null> = { compress: null, extract: null, verify: null, settings: null };
let lastWorkMode: Exclude<Mode, 'settings'> = 'compress';
let gameRenderId = 0;
function regionFor(id?: string, explicit?: string): string | undefined {
  if (!settings.region) return undefined;
  if (explicit) return explicit === 'USA' ? 'North America' : explicit;
  if (!id) return undefined;
  if (/^(SLUS|SCUS|BLUS|BCUS)/.test(id)) return 'North America';
  if (/^(SLES|SCES|BLES|BCES)/.test(id)) return 'Europe';
  if (/^(SLPS|SLPM|SCPS|BLJM|BLJS|BCJS)/.test(id)) return 'Japan';
  return undefined;
}
function titleFor(id?: string): string | undefined {
  if (id === 'SLUS-00892') return 'Final Fantasy VIII';
  if (id === 'BLJM85001') return 'Monster Hunter Portable 3rd HD';
  return undefined;
}
function discFor(id?: string, explicit?: number): string | undefined {
  if (!settings.region) return undefined;
  if (explicit) return String(explicit);
  if (id === 'SLUS-00892') return '1 of 4';
  return undefined;
}
function renderGame() {
  const token = ++gameRenderId;
  el<HTMLElement>('gamePanel').hidden = !settings.panel;
  el<HTMLElement>('workspace').classList.toggle('withoutPanel', !settings.panel);
  if (!settings.panel) return;
  const game = gameStates[mode];
  const title = game?.title || (game ? 'Unknown PlayStation Disc' : 'PlayStation disc');
  text('gameTitle', title);
  text('gameHint', game ? (!game.title ? 'Title unavailable from disc metadata.' : '') : 'Select a disc to view game information.');
  el<HTMLElement>('gameDetails').innerHTML = game ? properties([
    ['Console:', known(game.platform)], ['Serial:', known(game.serial)],
    ['Region:', known(game.region)], ['Disc:', known(game.disc)],
    ['Format:', known(game.format)], ['Disc size:', game.bytes == null ? 'Unknown' : size(game.bytes)]
  ]) : '';
  el<HTMLElement>('manualGame').hidden = !game;
  el<HTMLElement>('coverPlaceholder').hidden = false;
  if (!settings.artwork || !game?.artworkId) { void showArtwork('gameArtwork', null); return; }
  const platform = game.platform === 'PlayStation' ? 'ps1' : game.platform === 'PlayStation 2' ? 'ps2' : game.platform === 'PlayStation 3' ? 'ps3' : null;
  if (!platform) { void showArtwork('gameArtwork', null); return; }
  void showArtwork('gameArtwork', { platform, serial: game.artworkId }, settings.download && settings.cache && platform === 'ps3').then(() => { if (token !== gameRenderId) return; });
}
function setGame(target: Mode, game: GameInfo | null) { gameStates[target] = game; if (mode === target) renderGame(); }
type ArtContext = { platform: 'ps1' | 'ps2' | 'ps3'; serial: string };
const artContexts: Record<string, ArtContext | null> = { ps1Artwork: null, ps2Artwork: null, ps3Artwork: null, extractArtwork: null, verifyArtwork: null, gameArtwork: null };

async function artworkStats() {
  if (!artworkRoot) return;
  try { const s = await invoke<{ files: number; bytes: number }>('artwork_stats', { root: artworkRoot });
    text('artworkSize', `(${s.files} images, ${size(s.bytes)})`);
    text('settingsArtSize', `${s.files} images · ${size(s.bytes)}`);
  } catch { text('artworkSize', '(folder unavailable)'); }
}
async function showArtwork(hostId: string, context: ArtContext | null, fetchPs3 = false) {
  if (!settings.artwork) context = null;
  artContexts[hostId] = context;
  const host = el<HTMLElement>(hostId); host.replaceChildren(); delete host.dataset.key; host.hidden = !context || (hostId !== 'gameArtwork' && settings.panel);
  if (hostId !== 'gameArtwork' && settings.panel) return;
  if (!context || !artworkRoot) return;
  const key = `${context.platform}:${context.serial}`; host.dataset.key = key;
  const picture = document.createElement('img'); picture.className = 'coverPreview'; picture.alt = `Cover for ${context.serial}`;
  picture.title = 'View full cover'; picture.onclick = async () => { try { const image = await invoke<string | null>('artwork_lookup', { root: artworkRoot, platform: context.platform, variant: variant(), serial: context.serial, thumbnail: false, manualOnly: !settings.preferLocal && context.platform !== 'ps3' });
    if (image) { el<HTMLImageElement>('fullCover').src = image; el<HTMLDialogElement>('coverDetail').showModal(); }
  } catch (error) { message.textContent = String(error); } };
  const message = document.createElement('span'); message.textContent = 'Checking local artwork…';
  const controls = document.createElement('div'); controls.className = 'coverControls';
  const replace = document.createElement('button'); replace.textContent = 'Replace artwork';
  const remove = document.createElement('button'); remove.textContent = 'Remove cached artwork';
  controls.append(replace, remove); host.append(picture, message, controls);
  const variant = () => context.platform === 'ps3' ? 'covers' : 'default';
  const refresh = async (download: boolean) => {
    try {
      let image = await invoke<string | null>('artwork_lookup', { root: artworkRoot, platform: context.platform, variant: variant(), serial: context.serial, thumbnail: true, manualOnly: !settings.preferLocal && context.platform !== 'ps3' });
      if (!image && download && context.platform === 'ps3') {
        message.textContent = 'Checking GameTDB…';
        image = await invoke<string | null>('artwork_download_ps3', { root: artworkRoot, titleId: context.serial });
        void artworkStats();
      }
      if (host.dataset.key !== key) return;
      picture.hidden = !image; if (image) picture.src = image; else picture.removeAttribute('src');
      if (hostId === 'gameArtwork') el<HTMLElement>('coverPlaceholder').hidden = !!image;
      message.textContent = image ? context.serial : 'No cover available';
    } catch (error) { if (host.dataset.key === key) message.textContent = `Artwork unavailable: ${error}`; }
  };
  replace.onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Artwork image', extensions: ['jpg', 'jpeg', 'png', 'webp'] }] });
    if (typeof path !== 'string') return;
    try { await invoke('artwork_replace', { root: artworkRoot, platform: context.platform, variant: variant(), serial: context.serial, source: path }); await refresh(false); await artworkStats(); }
    catch (error) { message.textContent = String(error); }
  };
  remove.onclick = async () => { try { await invoke('artwork_remove', { root: artworkRoot, platform: context.platform, variant: variant(), serial: context.serial }); await refresh(false); await artworkStats(); }
    catch (error) { message.textContent = String(error); } };
  void refresh(fetchPs3);
}
input('artworkDir').value = artworkRoot;
void (async () => { if (!artworkRoot) { artworkRoot = await invoke<string>('artwork_default_dir'); input('artworkDir').value = artworkRoot; } await artworkStats();
  for (const [id, context] of Object.entries(artContexts)) if (context) void showArtwork(id, context, false);
})();
el<HTMLButtonElement>('browseArtworkDir').onclick = async () => { const path = await open({ directory: true, multiple: false }); if (typeof path !== 'string') return;
  artworkRoot = path; localStorage.setItem('artworkRoot', path); input('artworkDir').value = path; await artworkStats();
  input('settingsArtDir').value = path;
  for (const [id, context] of Object.entries(artContexts)) if (context) void showArtwork(id, context, false);
};
el<HTMLButtonElement>('artworkProvider').onclick = () => { void openUrl('https://www.gametdb.com/PS3'); };
el<HTMLButtonElement>('closeCoverDetail').onclick = () => { el<HTMLDialogElement>('coverDetail').close(); el<HTMLImageElement>('fullCover').removeAttribute('src'); };
const settingIds: Record<keyof Settings, string> = { detect: 'settingDetect', panel: 'settingPanel', region: 'settingRegion', artwork: 'settingArtwork', download: 'settingDownload', preferLocal: 'settingPreferLocal', cache: 'settingCache' };
function saveSettings() {
  for (const key of Object.keys(settingIds) as (keyof Settings)[]) settings[key] = input(settingIds[key]).checked;
  localStorage.setItem('gameSettings', JSON.stringify(settings));
  renderGame();
  for (const [id, context] of Object.entries(artContexts)) if (id !== 'gameArtwork' && context) void showArtwork(id, context, false);
}
for (const [key, id] of Object.entries(settingIds) as [keyof Settings, string][]) {
  input(id).checked = settings[key]; input(id).onchange = saveSettings;
}
el<HTMLButtonElement>('settingsBrowseArt').onclick = () => el<HTMLButtonElement>('browseArtworkDir').click();
el<HTMLButtonElement>('downloadArt').onclick = () => {
  const game = gameStates[lastWorkMode];
  if (!settings.artwork || !settings.cache) { text('settingsArtSize', 'Enable artwork and caching first.'); return; }
  if (game?.platform !== 'PlayStation 3' || !game.artworkId) { text('settingsArtSize', 'Automatic downloads currently support identified PS3 games.'); return; }
  void showArtwork('gameArtwork', { platform: 'ps3', serial: game.artworkId }, true);
  text('settingsArtSize', 'Checking GameTDB for this game…');
};
el<HTMLButtonElement>('clearArt').onclick = async () => {
  if (!await ask('Remove downloaded PS3 covers and your manual artwork replacements? The separate PS1/PS2 cover collection will stay intact.', { title: 'Clear artwork cache', kind: 'warning' })) return;
  try { const removed = await invoke<number>('artwork_clear_cache', { root: artworkRoot });
    text('settingsArtSize', `Removed ${removed} cached files.`); await artworkStats(); renderGame();
  } catch (error) { text('settingsArtSize', `Could not clear cache: ${error}`); }
};
el<HTMLButtonElement>('saveManualGame').onclick = () => {
  const game = gameStates[mode]; if (!game) return;
  const title = input('manualTitle').value.trim();
  const platform = input('manualPlatform').value;
  setGame(mode, { ...game, title: title || game.title, platform: platform || game.platform });
  el<HTMLDetailsElement>('manualGame').open = false;
};
renderGame();

function log(message: string) {
  const pane = el<HTMLPreElement>('log');
  pane.textContent += `${new Date().toLocaleTimeString()}  ${message}\n`;
  pane.scrollTop = pane.scrollHeight;
}
function setStatus(value: string) { text('status', value); }
function updateButtons() {
  el<HTMLButtonElement>('doCompress').disabled = busy || !source || !input('pxOutput').value;
  el<HTMLButtonElement>('browseBin').disabled = busy || !input('cuePath').value;
  el<HTMLButtonElement>('doExtract').disabled = busy || (!extractInfo && !extractChd && !extractPs3) || !input('extractDir').value;
  el<HTMLButtonElement>('doVerify').disabled = busy || (!verifyInfo && !verifyChd && !verifyPs3);
  el<HTMLButtonElement>('doPs2Compress').disabled = busy || !ps2Source || ps2Source.format === 'CHD' || ps2Source.platform === 'PlayStation' || !input('ps2Output').value;
  el<HTMLButtonElement>('doPs3Compress').disabled = busy || !ps3Source || !input('ps3Output').value;
  el<HTMLButtonElement>('doPs2Batch').disabled = busy;
  document.querySelectorAll<HTMLButtonElement>('.modebar button[data-mode]').forEach(button => button.disabled = busy);
}
function clearResult() {
  el<HTMLElement>('result').hidden = true;
  el<HTMLElement>('result').replaceChildren();
  el<HTMLElement>('resultActions').hidden = true;
}
function setMode(next: Mode) {
  mode = next;
  if (next !== 'settings') lastWorkMode = next;
  for (const name of ['compress', 'extract', 'verify', 'settings'] as Mode[]) {
    el<HTMLElement>(`${name}Pane`).hidden = name !== next || (name === 'compress' && activeSource !== 'ps1');
    document.querySelector<HTMLButtonElement>(`[data-mode="${name}"]`)!.classList.toggle('selected', name === next);
  }
  el<HTMLElement>('workspace').hidden = next === 'settings';
  if (next === 'settings') { input('settingsArtDir').value = artworkRoot; void artworkStats(); }
  el<HTMLElement>('ps2Pane').hidden = next !== 'compress' || activeSource !== 'ps2';
  el<HTMLElement>('ps3Pane').hidden = next !== 'compress' || activeSource !== 'ps3';
  clearResult();
  setStatus('Ready');
  updateButtons();
  renderGame();
}
function showResult(message: string, isError = false) {
  const result = el<HTMLElement>('result');
  result.hidden = false;
  result.classList.toggle('error', isError);
  result.textContent = message;
}

async function loadCue(path: string, selectedBin: string | null = null) {
  activeSource = 'ps1'; ps2Source = null; setMode('compress');
  void showArtwork('ps1Artwork', null);
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
  setGame('compress', null);
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
    void showArtwork('ps1Artwork', source.disc.serial ? { platform: 'ps1', serial: source.disc.serial } : null);
    setGame('compress', settings.detect ? { title: source.disc.title || titleFor(source.disc.serial), platform: source.disc.platform, serial: source.disc.serial, region: regionFor(source.disc.serial, source.disc.region), disc: discFor(source.disc.serial, source.disc.disc_number), format: 'BIN/CUE · CD-ROM', bytes: source.bin_size + source.cue_size, artworkId: source.disc.serial } : { format: 'BIN/CUE · CD-ROM', bytes: source.bin_size + source.cue_size });
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
  void showArtwork(target === 'extract' ? 'extractArtwork' : 'verifyArtwork', null);
  const loadId = target === 'extract' ? ++extractLoadId : ++verifyLoadId;
  input(target === 'extract' ? 'extractPath' : 'verifyPath').value = path;
  if (target === 'extract') { extractInfo = null; extractChd = null; extractPs3 = null; } else { verifyInfo = null; verifyChd = null; verifyPs3 = null; }
  const summary = el<HTMLElement>(target === 'extract' ? 'extractSummary' : 'verifySummary');
  summary.textContent = 'Inspecting PX1…';
  setGame(target, null);
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
    void showArtwork(target === 'extract' ? 'extractArtwork' : 'verifyArtwork', manifest.disc.serial ? { platform: 'ps1', serial: manifest.disc.serial } : null);
    setGame(target, settings.detect ? { title: manifest.disc.title || titleFor(manifest.disc.serial), platform: manifest.disc.platform, serial: manifest.disc.serial, region: regionFor(manifest.disc.serial, manifest.disc.region), disc: discFor(manifest.disc.serial, manifest.disc.disc_number), format: 'PX1 (BIN/CUE)', bytes: manifest.bin.size + manifest.cue.size, artworkId: manifest.disc.serial } : { format: 'PX1', bytes: manifest.bin.size + manifest.cue.size });
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
  text('activityTitle', name.endsWith('_ps2') ? 'Running chdman and checking restored hashes…' : name.endsWith('_ps3') ? 'Streaming PS3 ISO and checking SHA-256…' : 'Starting…');
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

document.querySelectorAll<HTMLButtonElement>('.modebar button[data-mode]').forEach(button => button.onclick = () => setMode(button.dataset.mode as Mode));
async function loadDisc(path: string) {
  const selectionId = ++discSelectionId;
  ++cueLoadId; ++ps2LoadId; ++ps3LoadId;
  if (/\.cue$/i.test(path)) {
    try {
      const linkedIso = await invoke<string | null>('resolve_ps3_cue', { path });
      if (selectionId !== discSelectionId) return;
      if (linkedIso) { await loadPs3(linkedIso, path); return; }
    } catch (error) { if (selectionId === discSelectionId) log(`CUE/ISO link: ${error}`); }
    try {
      const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath: null });
      if (selectionId !== discSelectionId) return;
      if (disc.platform === 'PlayStation') { await loadCue(path); return; }
      await loadPs2(path); return;
    } catch { if (selectionId === discSelectionId) await loadCue(path); return; }
  }
  if (/\.iso$/i.test(path)) {
    try { await invoke<Ps3Info>('inspect_ps3', { path }); if (selectionId === discSelectionId) await loadPs3(path); }
    catch { if (selectionId === discSelectionId) await loadPs2(path); }
    return;
  }
  if (/\.chd$/i.test(path)) { setMode('extract'); await loadArchive(path, 'extract'); }
  if (/\.ps3z$/i.test(path)) { setMode('extract'); await loadArchive(path, 'extract'); }
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
  void showArtwork(target === 'extract' ? 'extractArtwork' : 'verifyArtwork', null);
  setGame(target, null);
  if (/\.ps3z$/i.test(path)) {
    const summary = el<HTMLElement>(target === 'extract' ? 'extractSummary' : 'verifySummary');
    input(target === 'extract' ? 'extractPath' : 'verifyPath').value = path;
    if (target === 'extract') { extractInfo = null; extractChd = null; extractPs3 = null; input('extractDir').value = parent(path); }
    else { verifyInfo = null; verifyChd = null; verifyPs3 = null; }
    summary.textContent = 'Inspecting PS3Z…'; updateButtons();
    try { const info = await invoke<Ps3Info>('inspect_ps3z', { path });
      if (target === 'extract') extractPs3 = info; else verifyPs3 = info;
      summary.innerHTML = properties([['Platform:', info.platform], ['Disc type:', info.media], ['Restored file:', info.original_name], ['Original size:', size(info.original_size)], ['Archive size:', size(info.archive_size || 0)], ['Original SHA-256:', known(info.sha256)], ['Verification:', 'Run Verify to decode and hash every byte']]);
      setGame(target, settings.detect ? { title: titleFor(info.title_id), platform: info.platform, serial: info.title_id, region: regionFor(info.title_id), format: 'PS3Z (ISO)', bytes: info.original_size, artworkId: info.title_id } : { format: 'PS3Z', bytes: info.original_size });
    } catch (error) { summary.textContent = String(error); } updateButtons(); return;
  }
  if (!/\.chd$/i.test(path)) { await loadPx1(path, target); return; }
  const summary = el<HTMLElement>(target === 'extract' ? 'extractSummary' : 'verifySummary');
  input(target === 'extract' ? 'extractPath' : 'verifyPath').value = path;
  if (target === 'extract') { extractInfo = null; extractChd = null; extractPs3 = null; input('extractDir').value = parent(path); }
  else { verifyInfo = null; verifyChd = null; verifyPs3 = null; }
  summary.textContent = 'Inspecting CHD…'; updateButtons();
  try {
    const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath: null });
    if (target === 'extract') extractChd = disc; else verifyChd = disc;
    const originalKnown = !disc.confidence.includes('no original-hash');
    summary.innerHTML = properties([
      ['Platform:', disc.platform], ['Disc type:', disc.media], ['Original format:', originalKnown ? (disc.files.length > 1 ? 'BIN/CUE' : 'ISO') : 'Unknown'],
      ['Original size:', originalKnown ? size(disc.original_size) : 'Unknown'], ['CHD size:', size(disc.compressed_size ?? disc.original_size)],
      ['Verification:', disc.confidence.includes('no original-hash') ? 'Original hashes unavailable' : 'Run Verify to check CHD integrity']
    ]);
    void showArtwork(target === 'extract' ? 'extractArtwork' : 'verifyArtwork', disc.serial && disc.platform === 'PlayStation 2' ? { platform: 'ps2', serial: disc.serial } : null);
    setGame(target, settings.detect ? { title: disc.title || titleFor(disc.serial), platform: disc.platform, serial: disc.serial, region: regionFor(disc.serial), format: `CHD (${disc.media})`, bytes: originalKnown ? disc.original_size : undefined, artworkId: disc.serial } : { format: 'CHD', bytes: originalKnown ? disc.original_size : undefined });
  } catch (error) { summary.textContent = String(error); }
  updateButtons();
}
el<HTMLButtonElement>('browseExtract').onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Disc archive', extensions: ['px1', 'chd', 'ps3z'] }] }); if (typeof path === 'string') await loadArchive(path, 'extract'); };
el<HTMLButtonElement>('browseVerify').onclick = async () => { const path = await open({ multiple: false, filters: [{ name: 'Disc archive', extensions: ['px1', 'chd', 'ps3z'] }] }); if (typeof path === 'string') await loadArchive(path, 'verify'); };
el<HTMLButtonElement>('browseDir').onclick = async () => { const path = await open({ directory: true, multiple: false }); if (typeof path === 'string') { input('extractDir').value = path; updateButtons(); } };
async function loadPs2(path: string, binPath: string | null = null) {
  const loadId = ++ps2LoadId;
  activeSource = 'ps2'; source = null; setMode('compress');
  setGame('compress', null);
  void showArtwork('ps2Artwork', null);
  ps2Source = null; ps2BinPath = binPath; input('ps2Path').value = path; input('ps2Bin').value = binPath || '';
  text('ps2Summary', 'Inspecting disc…');
  try {
    const disc = await invoke<Ps2Source>('inspect_ps2', { path, binPath });
    if (loadId !== ps2LoadId) return;
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
    void showArtwork('ps2Artwork', disc.serial && disc.platform === 'PlayStation 2' ? { platform: 'ps2', serial: disc.serial } : null);
    setGame('compress', settings.detect ? { title: disc.title || titleFor(disc.serial), platform: disc.platform, serial: disc.serial, region: regionFor(disc.serial), disc: discFor(disc.serial), format: `${disc.format} · ${disc.media}`, bytes: disc.original_size, artworkId: disc.serial } : { format: disc.format, bytes: disc.original_size });
    updateButtons();
  } catch (e) { if (loadId === ps2LoadId) text('ps2Summary', String(e)); }
  updateButtons();
}
el<HTMLButtonElement>('browsePs2').onclick = async () => { const p = await open({ multiple: false, filters: [{ name: 'Disc image', extensions: ['iso', 'cue', 'chd'] }] }); if (typeof p === 'string') await loadDisc(p); };
el<HTMLSelectElement>('ps2Platform').onchange = async () => {
  if (input('ps2Platform').value === 'PlayStation' && ps2Source?.format === 'BIN/CUE') { await loadCue(ps2Source.path, ps2BinPath); return; }
  if (input('ps2Platform').value === 'PlayStation 3' && ps2Source) {
    try { await invoke<Ps3Info>('inspect_ps3', { path: ps2Source.path }); await loadPs3(ps2Source.path); }
    catch (error) { input('ps2Platform').value = 'auto'; text('ps2Summary', `PS3 identification failed: ${error}`); }
  }
};
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
async function loadPs3(path: string, linkedCue: string | null = null) {
  const loadId = ++ps3LoadId;
  activeSource = 'ps3'; ps3Source = null; setMode('compress'); input('ps3Path').value = path;
  setGame('compress', null);
  input('ps3ArtId').value = '';
  void showArtwork('ps3Artwork', null);
  text('ps3Summary', 'Inspecting PS3 ISO…'); updateButtons();
  try { const info = await invoke<Ps3Info>('inspect_ps3', { path }); if (loadId !== ps3LoadId) return; ps3Source = info;
    input('ps3Output').value = path.replace(/\.iso$/i, '.ps3z');
    el<HTMLElement>('ps3Summary').innerHTML = properties([['Platform:', info.platform], ['Title ID:', known(info.title_id)], ['Disc type:', info.media], ['Original format:', info.format], ['Original size:', size(info.original_size)], ['Compression format:', 'PS3Z (zstd)'], ['Detection:', info.confidence]]) + (linkedCue ? '<p class="note">Opened the ISO referenced by the CUE. PS3Z preserves the ISO; keep the original CUE separately.</p>' : '');
    input('ps3ArtId').value = info.title_id || '';
    void showArtwork('ps3Artwork', info.title_id ? { platform: 'ps3', serial: info.title_id } : null, settings.download && settings.cache);
    setGame('compress', settings.detect ? { title: titleFor(info.title_id) || info.title, platform: info.platform, serial: info.title_id, region: regionFor(info.title_id), format: `${info.format} · ${info.media}`, bytes: info.original_size, artworkId: info.title_id } : { format: info.format, bytes: info.original_size });
  } catch (error) { if (loadId === ps3LoadId) text('ps3Summary', String(error)); } updateButtons();
}
el<HTMLButtonElement>('browsePs3').onclick = async () => { const p = await open({ multiple: false, filters: [{ name: 'PS3 ISO', extensions: ['iso'] }] }); if (typeof p === 'string') await loadDisc(p); };
input('ps3ArtId').onchange = () => { const id = input('ps3ArtId').value.trim().toUpperCase();
  void showArtwork('ps3Artwork', /^[A-Z]{4}[0-9]{5}$/.test(id) ? { platform: 'ps3', serial: id } : null, settings.download && settings.cache);
  if (gameStates.compress?.platform === 'PlayStation 3') setGame('compress', { ...gameStates.compress, artworkId: id });
};
el<HTMLButtonElement>('browsePs3Output').onclick = async () => { const p = await save({ defaultPath: input('ps3Output').value || undefined, filters: [{ name: 'PS3Z archive', extensions: ['ps3z'] }] }); if (p) { input('ps3Output').value = p; updateButtons(); } };
el<HTMLButtonElement>('doPs3Compress').onclick = () => { if (!ps3Source) return;
  void run<Ps3Report>('compress_ps3', { path: ps3Source.path, outputPath: input('ps3Output').value }, report => {
    const a = report.info.archive_size || 0, b = report.info.original_size;
    setStatus('PS3 ISO archived; exact restoration verified');
    showResult(`Original: ${size(b)}   PS3Z: ${size(a)}   Ratio: ${(a / b * 100).toFixed(1)}%   Original ISO SHA-256: PASS`);
    lastOutput = input('ps3Output').value; el<HTMLElement>('resultActions').hidden = false; el<HTMLButtonElement>('verifyResult').hidden = false; text('verifyResult', 'Verify PS3Z');
  });
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
  if (extractPs3) {
    void run<Ps3Report>('extract_ps3', { path: input('extractPath').value, outputDir: input('extractDir').value }, report => {
      setStatus('Restored PS3 ISO matches stored SHA-256'); showResult(`Restored ${report.info.original_name} byte for byte. SHA-256: PASS`);
      lastOutput = input('extractDir').value; el<HTMLElement>('resultActions').hidden = false; el<HTMLButtonElement>('verifyResult').hidden = true;
    }); return;
  }
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
  if (verifyPs3) {
    void run<Ps3Report>('verify_ps3', { path: input('verifyPath').value }, report => {
      setStatus('PS3Z verified'); showResult(`Decoded ISO size and SHA-256 match: ${report.info.original_name}`);
    }); return;
  }
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
  else if (/\.(px1|chd|ps3z)$/i.test(path)) { const target = mode === 'verify' ? 'verify' : 'extract'; setMode(target); void loadArchive(path, target); }
});
log('Ready. Select a CUE, ISO, PX1, CHD, or PS3Z file.');
