// Tidy UI - plain JS, no build step. Runs inside Tauri or in a browser with mock data.

'use strict';

const tauri = window.__TAURI__;

const invoke = (cmd, args) => tauri ? tauri.core.invoke(cmd, args) : Promise.reject(new Error('mock'));

const GB = 1024 ** 3, MB = 1024 ** 2;

const $ = s => document.querySelector(s);

const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

const ic = (id, cls = '') => `<svg class="${cls}"><use href="#i-${id}"/></svg>`;

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));

const ease = t => 1 - Math.pow(1 - clamp(t), 3);



function fmt(b, d) {

  if (b >= GB) { const v = b / GB; return (v >= 100 ? v.toFixed(0) : v.toFixed(d ?? 1)) + ' GB'; }

  if (b >= MB) return (b / MB).toFixed(0) + ' MB';

  return Math.round(b / 1024) + ' KB';

}

const n = v => Math.round(v).toLocaleString('en-US');

function rng(seed) { return () => { seed |= 0; seed = seed + 0x6D2B79F5 | 0; let t = Math.imul(seed ^ seed >>> 15, 1 | seed); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }

function series(seed, base, amp, len = 40) { const r = rng(seed); let v = base; return Array.from({ length: len }, () => (v = clamp(v + (r() - .5) * amp, base * .35, base * 1.9 + 1))); }



// ---------- mock data (shape matches the Rust commands) ----------

const SEG = { system: ['Windows', 'var(--c-system)'], apps: ['Apps', 'var(--c-apps)'], games: ['Games', 'var(--c-games)'], docs: ['Documents', 'var(--c-docs)'],

  media: ['Photos & video', 'var(--c-media)'], other: ['Other', 'var(--c-other)'], junk: ['Cleanable', 'var(--c-junk)'] };

const drive = (mount, label, kind, total, segs) => ({ mount, label, kind, total: total * GB, segments: Object.entries(segs).map(([id, g]) => ({ id, bytes: g * GB })),

  get free() { return this.total - this.segments.reduce((a, s) => a + s.bytes, 0); } });

// ---------- safety screen (blueprint M2 UI) ----------

const SAFETY_MOCK = {
  loaded: false, loading: false, showQ: false,
  manifestVersion: '1',
  categories: [
    ['temp_user', 'Your temp files', false, true, 7], ['temp_windows', 'Windows temp', true, false, 30],
    ['browser_cache_chromium', 'Chromium browser cache', false, true, 14], ['browser_cache_firefox', 'Firefox cache', false, true, 14],
    ['gpu_shader_cache', 'Graphics shader cache', false, true, 7], ['thumbnail_cache', 'Thumbnail cache', false, true, 0],
    ['crash_dumps', 'Crash reports', false, true, 30], ['delivery_optimization', 'Delivery Optimization cache', true, false, 30],
    ['ps_history', 'PowerShell history', false, false, 0],
  ].map(([id, displayName, requiresAdmin, enabledByDefault, minAgeDays]) => ({ id, displayName, requiresAdmin, enabledByDefault, minAgeDays })),
  enabledCategories: ['temp_user', 'browser_cache_chromium', 'browser_cache_firefox', 'gpu_shader_cache', 'thumbnail_cache', 'crash_dumps'],
  autonomousEnabled: false, dryRunsCompleted: 2, dryRunPhaseComplete: false,
  quarantineFiles: 312, quarantineBytes: 486 * MB, running: false, schedulerInstalled: false,
  scheduleMode: 'weekly', scheduleDay: 'Sunday', scheduleHour: 3,
  pauseOnFullscreen: true, batteryPauseBelowPct: 30,
  lastRun: { at: new Date(Date.now() - 86400000).toISOString(), source: 'ui', outcome: 'complete', detail: '', dryRun: true, files: 9214, bytes: 1.8 * GB },
  preflight: { batteryPercent: null, onAcPower: true, fullscreenApp: false, idleSeconds: 0, freeSpaceGb: 96.4 },
  runs: [
    { at: new Date(Date.now() - 86400000).toISOString(), source: 'ui', outcome: 'complete', detail: '', dryRun: true, files: 9214, bytes: 1.8 * GB },
    { at: new Date(Date.now() - 86600000).toISOString(), source: 'ui', outcome: 'complete', detail: '', dryRun: true, files: 9180, bytes: 1.8 * GB },
    { at: new Date(Date.now() - 172800000).toISOString(), source: 'scheduled', outcome: 'skipped', detail: 'a fullscreen app is active', dryRun: false, files: 0, bytes: 0 },
  ],
  quarantine: [],
};

const CAT_BLURB = {
  temp_user: 'Your %TEMP% folder', temp_windows: 'System-wide temp folder',
  browser_cache_chromium: 'Chrome, Edge, Brave and other Chromium browsers',
  browser_cache_firefox: 'Firefox cache storage',
  gpu_shader_cache: 'Rebuilt automatically by your games',
  thumbnail_cache: 'Explorer previews, rebuilt as you browse',
  crash_dumps: 'Crash dumps and error reports',
  delivery_optimization: 'Windows Update download cache',
  ps_history: 'Command history file only',
};

const RUN_SOURCE = { ui: 'Manual run', tray: 'Tray run', scheduled: 'Scheduled run', cli: 'Command line' };

function fmtRunAt(iso) {
  const d = new Date(iso);
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) + ' · ' +
    d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
}


const S = {

  host: 'DESKTOP-7QK2M4R', os: 'Windows 11 Pro 24H2', cpu: 23,

  drives: [drive('C:', 'Windows', 'NVMe SSD', 476.3, { system: 38.6, apps: 112.4, games: 168.9, docs: 21.7, media: 34.2, other: 26.5, junk: 12.8 }),

    drive('D:', 'Data', 'SSD', 1863, { games: 604.3, media: 361.8, docs: 88.1, apps: 41.6, other: 54.8 })],

  mem: { total: 32 * GB, used: 19.8 * GB, cached: 7.4 * GB, compressed: 1.3 * GB, available: 12.2 * GB, committed: 24.1 * GB, commitLimit: 37.9 * GB, score: 63, pressure: 'normal' },

  memHist: series(7, 61, 5, 60),

  cats: [

    ['temp', 'Temporary files', 'Leftovers from installers and apps, older than a day', 4.12, 18420, 'none', [['%TEMP%', 3.61], ['C:\\Windows\\Temp', 0.51]]],

    ['browser', 'Browser caches', 'Chrome, Edge and Firefox. Sites load a bit slower once', 3.05, 41337, 'low', [['Chrome · 2 profiles', 1.84], ['Microsoft Edge', 0.86], ['Firefox', 0.35]]],

    ['shader', 'Shader caches', 'NVIDIA DXCache, D3DSCache. Games rebuild them on launch', 2.36, 3104, 'low', [['NVIDIA DXCache', 1.71], ['D3DSCache', 0.48], ['NVIDIA GLCache', 0.17]]],

    ['windows-update', 'Windows Update cache', 'Installed update packages. Needs admin rights', 1.74, 212, 'review', []],

    ['crash-dumps', 'Crash dumps & error reports', 'Memory dumps from past app and system crashes', 0.82, 37, 'none', [['C:\\Windows\\Minidump', 0.61], ['WER ReportArchive', 0.21]]],

    ['thumbnails', 'Thumbnail cache', 'Explorer previews, rebuilt as you browse', 0.31, 14, 'none', []],

    ['recycle-bin', 'Recycle Bin', 'Files you deleted. Emptying cannot be undone', 0.42, 96, 'review', []],

  ].map(([id, label, explain, gb, files, risk, subs]) => ({ id, label, explain, bytes: gb * GB, files, risk, subs, selected: id !== 'recycle-bin' })),

  procs: [

    ['chrome.exe', 'Google Chrome', 'Every tab and extension is its own process', 'browser', 14, 8.4, 2.14, '#E0A43B'],

    ['obs64.exe', 'OBS Studio', 'Recording and streaming. CPU depends on encoder settings', 'app', 1, 6.1, 0.61, '#4A4A50'],

    ['MsMpEng.exe', 'Microsoft Defender Antivirus', 'Real-time malware scanning; busy after big downloads', 'service', 1, 4.7, 0.29, '#6A8FD8'],

    ['dwm.exe', 'Desktop Window Manager', 'Draws every window and animation on screen', 'system', 1, 3.2, 0.18, '#8E8E93'],

    ['ms-teams.exe', 'Microsoft Teams', 'Chat and meetings. Heavy on memory in the background', 'app', 6, 2.6, 1.32, '#7B83C9'],

    ['svchost.exe', 'Service Host', 'Runs Windows services in groups', 'service', 78, 2.3, 1.58, '#8E8E93'],

    ['SearchIndexer.exe', 'Windows Search Indexer', 'Builds the search index; calms down after catching up', 'service', 1, 1.9, 0.09, '#8E8E93'],

    ['Spotify.exe', 'Spotify', 'Music streaming. Several processes are normal', 'app', 5, 1.2, 0.47, '#3FAE68'],

    ['Code.exe', 'Visual Studio Code', 'Code editor; extensions run in helper processes', 'app', 9, 0.9, 1.06, '#4E8FD0'],

    ['steamwebhelper.exe', 'Steam Web Helper', 'The browser inside Steam that renders the store', 'app', 7, 0.8, 0.52, '#4D6A8C'],

    ['explorer.exe', 'Windows Explorer', 'Taskbar, Start menu and File Explorer', 'system', 1, 0.6, 0.21, '#E3AC4A'],

    ['Memory Compression', 'Memory Compression', 'Windows squeezing idle memory instead of paging to disk', 'system', 1, 0.3, 1.3, '#8E8E93'],

    ['OneDrive.exe', 'OneDrive', 'Syncs files to the cloud; busy after adding many files', 'app', 1, 0.4, 0.12, '#4E8FD0'],

    ['System', 'System', 'The Windows kernel. High CPU usually means a busy driver', 'system', 1, 0.5, 0.01, '#8E8E93'],

  ].map(([name, title, what, kind, instances, cpu, mem, color], i) => ({ name, title, what, kind, instances, cpu, memory: mem * GB, color, history: series(i * 13 + 3, cpu, cpu * .5 + .4) })),

  startup: [

    ['Microsoft Teams', 'C:\\Users\\alex\\AppData\\Local\\Microsoft\\WindowsApps\\ms-teams.exe msteams:system-initiated', 'HKCU Run', 'high', true, '#7B83C9'],

    ['Steam', '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', 'HKCU Run', 'high', true, '#4D6A8C'],

    ['OneDrive', '"C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe" /background', 'HKCU Run', 'high', true, '#4E8FD0'],

    ['Epic Games Launcher', '"C:\\Program Files (x86)\\Epic Games\\Launcher\\EpicGamesLauncher.exe" -silent', 'HKCU Run', 'high', false, '#4A4A50'],

    ['Spotify', 'C:\\Users\\alex\\AppData\\Roaming\\Spotify\\Spotify.exe /minimized', 'HKCU Run', 'high', false, '#3FAE68'],

    ['Docker Desktop', '"C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe" -Autostart', 'HKCU Run', 'medium', true, '#4E8FD0'],

    ['Stream Deck', '"C:\\Program Files\\Elgato\\StreamDeck\\StreamDeck.exe" --runinbk', 'HKLM Run', 'medium', true, '#4A4A50'],

    ['PowerToys', 'C:\\Program Files\\PowerToys\\PowerToys.exe', 'HKCU Run', 'medium', true, '#E3AC4A'],

    ['Realtek Audio Service', 'C:\\Program Files\\Realtek\\Audio\\RtkAudUService64.exe -background', 'HKLM Run', 'low', true, '#E4836F'],

    ['Windows Security', '%windir%\\system32\\SecurityHealthSystray.exe', 'HKLM Run', 'low', true, '#6A8FD8'],

  ].map(([name, command, source, impact, enabled, color]) => ({ id: source + '|' + name, name, command, source, impact, enabled, color })),

  big: [['D:\\Captures\\2026-10-31 ranked night.mkv', 14.2, 'Video'], ['C:\\Users\\alex\\Downloads\\Win11_24H2_English_x64.iso', 5.8, 'Disk image'],

    ['D:\\VMs\\dev-box\\dev-box.vhdx', 4.9, 'Disk image'], ['C:\\Users\\alex\\Downloads\\Blender-4.2-assets.zip', 2.7, 'Archive'],

    ['D:\\Captures\\clip_0412.mp4', 1.9, 'Video'], ['C:\\Users\\alex\\Downloads\\DaVinci_Resolve_19.exe', 1.1, 'Installer']],

  buckets: [['100-500 MB', 142], ['0.5-1 GB', 38], ['1-4 GB', 17], ['4 GB +', 6]],

  dupes: { groups: 214, wasted: 6.3 * GB },

  sort: 'cpu', sel: 'chrome.exe', open: { browser: true }, scanP: 1, scanMs: 2840, cleanP: null, dry: true, query: '',

  safety: SAFETY_MOCK,

};



// ---------- small renderers ----------

const k = c => `style="--k:${c}"`;

const reclaim = () => S.cats.filter(c => c.selected).reduce((a, c) => a + c.bytes, 0);

function spark(vals, w = 88, h = 24, max) {

  const m = max || Math.max(...vals, 1), step = w / (vals.length - 1);

  const pts = vals.map((v, i) => [i * step, h - 1.5 - (v / m) * (h - 4)].map(x => x.toFixed(1)).join(','));

  return `<svg class="spark" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none"><path class="area" d="M0,${h} L${pts.join(' L')} L${w},${h}Z"/><path d="M${pts.join(' L')}"/></svg>`;

}

function bar(d, junkP = 1, sweep) {

  const segs = d.segments.map(s => `<i class="${s.id === 'junk' ? 'junk' : ''}" style="--k:${SEG[s.id][1]};width:${(s.id === 'junk' ? s.bytes * junkP : s.bytes) / d.total * 100}%"></i>`).join('');

  return `<div class="bar">${segs}${sweep != null ? `<span class="sweep" style="left:calc(${sweep * 100}% - 35px)"></span>` : ''}</div>`;

}

function driveBlock(d, junkP, sweep) {

  const legend = d.segments.map(s => `<span ${k(SEG[s.id][1])}>${SEG[s.id][0]} <b>${fmt(s.id === 'junk' ? s.bytes * (junkP ?? 1) : s.bytes)}</b></span>`).join('');

  return `<div class="drive"><div class="top"><b>${d.label} (${d.mount})</b><span class="muted">${d.kind}</span>

    <span class="r"><b class="num">${fmt(d.free)}</b> free of ${fmt(d.total, 0)}</span></div>${bar(d, junkP, sweep)}

    <div class="legend">${legend}</div></div>`;

}

function gauge(score, size = 1) {

  const col = score < 65 ? 'var(--green)' : score < 85 ? '#FF9F0A' : '#FF3B30';

  const arc = 'M14 84 A70 70 0 0 1 154 84';

  return `<svg class="g" viewBox="0 0 168 98" style="width:${168 * size}px;height:${98 * size}px">

    <path d="${arc}" stroke="var(--track)" pathLength="100"/><path d="${arc}" stroke="${col}" pathLength="100" stroke-dasharray="${score} 100"/>

    <text x="84" y="74" text-anchor="middle" style="font:600 30px var(--d);letter-spacing:-.03em">${Math.round(score)}</text>

    <text x="84" y="92" text-anchor="middle" style="font:500 11px var(--t);fill:var(--ink2)">pressure</text></svg>`;

}

const pressureLbl = s => s < 65 ? ['ok', 'Normal'] : s < 85 ? ['warn', 'Elevated'] : ['bad', 'High'];

const impactCol = { high: '#FF3B30', medium: '#FF9F0A', low: 'var(--green)' };

function memBar(m) {
  const comp = [['In use', m.used - m.compressed, 'var(--c-apps)'], ['Compressed', m.compressed, 'var(--c-games)'], ['Cached', m.cached, 'var(--c-other)']];
  return `<div class="bar" style="margin-top:16px">${comp.map(c => `<i style="--k:${c[2]};width:${c[1] / m.total * 100}%"></i>`).join('')}</div>
    <div class="legend">${comp.map(c => `<span ${k(c[2])}>${c[0]} <b>${fmt(c[1])}</b></span>`).join('')}<span ${k('var(--track)')}>Free <b>${fmt(m.total - m.used - m.cached)}</b></span></div>`;
}
const head = (title, sub, act = '') => `<div class="head"><div><h1>${title}</h1><p>${sub}</p></div><div class="act">${act}</div></div>`;



// ---------- screens ----------

const V = {};

V.overview = () => {

  const total = reclaim(), p = S.scanP, scanning = p < 1;

  const cur = S.cats[Math.min(S.cats.length - 1, Math.floor(p * S.cats.length))];

  const found = S.cats.reduce((a, c, i) => a + c.bytes * clamp(p * S.cats.length - i), 0);

  const chips = S.cats.filter(c => c.selected).map((c, i) => `<span style="--k:${['#34C759', '#5FD47D', '#2DB24F', '#8BE0A1', '#248A3D', '#B6EBC4'][i]}">${c.label} ${fmt(c.bytes * clamp(p * S.cats.length - S.cats.indexOf(c)))}</span>`).join('');

  const top = [...S.procs].sort((a, b) => b.cpu - a.cpu).slice(0, 5);

  const m = S.mem, [pc, pl] = pressureLbl(m.score);

  return head('Overview', scanning ? 'Smart Scan in progress…' : `Last Smart Scan just now · took ${(S.scanMs / 1000).toFixed(1)} s on ${S.threads || 8} threads`,

    `<button class="btn" data-act="scan">${ic('scan')}Scan again</button>`) +

  `<div class="grid">

    <div class="card hero"><div>

      <div class="lbl">${scanning ? `Scanning ${esc(cur.label.toLowerCase())}…` : 'Ready to clean'}</div>

      <div class="amt num" id="found">${fmt(scanning ? found : total).replace(' GB', '<small>GB</small>')}</div>

      ${scanning ? `<div class="scanline"><i style="width:${p * 100}%"></i></div>` : `<div class="chips">${chips}</div>`}

    </div><div class="cta">

      <button class="btn go big" data-act="clean" ${scanning ? 'disabled' : ''}>${scanning ? 'Scanning…' : 'Clean ' + fmt(total)}</button>

      <div class="lbl">${ic('shield')}Safe-list protects 37 locations</div></div></div>

    <div class="card pad"><div class="ch"><h2>Storage</h2><span class="r">${S.drives.length} drives</span></div>${S.drives.map((d, i) => driveBlock(d, i ? 1 : (scanning ? ease(p) : 1), i || !scanning ? null : p)).join('')}</div>

    <div class="grid g2">

      <div class="card pad"><div class="ch"><h2>Memory</h2><span class="pill ${pc}">${pl}</span><span class="r">${fmt(m.total, 0)} installed</span></div>

        <div class="gauge">${gauge(m.score)}<div class="stats">

          <div><span class="lbl">In use</span><b>${fmt(m.used)}</b></div><div><span class="lbl">Available</span><b>${fmt(m.available)}</b></div>

          <div><span class="lbl">Cached</span><b>${fmt(m.cached)}</b></div><div><span class="lbl">Compressed</span><b>${fmt(m.compressed)}</b></div></div></div>${memBar(m)}</div>

      <div class="card pad"><div class="ch"><h2>Using the CPU</h2><span class="r">Total <b class="num" style="color:var(--ink)">${S.cpu}%</b></span></div>

        <div class="rows">${top.map(pr => `<div class="row"><span class="ico" ${k(pr.color)}>${pr.title[0]}</span><div class="nm"><b>${esc(pr.title)}</b><small>${esc(pr.what)}</small></div>${spark(pr.history.slice(-24))}<span class="v">${pr.cpu.toFixed(1)}%</span></div>`).join('')}</div></div>

    </div></div>`;

};



V.memory = () => {

  const m = S.mem, [pc, pl] = pressureLbl(m.score), h = S.memHist, W = 560, H = 196;

  const x = i => 34 + i * (W - 44) / (h.length - 1), y = v => 12 + (1 - v / 100) * (H - 34);

  const grid = [0, 25, 50, 75, 100].map(v => `<path d="M34 ${y(v)}H${W - 10}" stroke="${v ? 'var(--line2)' : 'var(--line)'}"/><text x="26" y="${y(v) + 3.5}" text-anchor="end">${v}</text>`).join('');

  const line = h.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(' L');

  const comp = [['In use', m.used - m.compressed, 'var(--c-apps)'], ['Compressed', m.compressed, 'var(--c-games)'], ['Cached', m.cached, 'var(--c-other)']];

  const memTop = [...S.procs].sort((a, b) => b.memory - a.memory).slice(0, 6);

  return head('Memory', `${fmt(m.total, 0)} RAM · ${fmt(m.committed)} of ${fmt(m.commitLimit)} committed`, `<button class="btn" data-act="purge" title="Needs administrator rights">Free standby memory</button>`) +

  `<div class="grid g32"><div class="card pad"><div class="ch"><h2>Memory pressure</h2><span class="pill ${pc}">${pl}</span><span class="r">Last 60 seconds</span></div>

    <svg class="chart" viewBox="0 0 ${W} ${H}">${grid}<path d="M${line} L${x(h.length - 1)},${y(0)} L34,${y(0)}Z" fill="var(--green-soft)" stroke="none"/><path d="M${line}" stroke="var(--green-ink)" stroke-width="1.5"/>

    <text x="34" y="${H - 4}">60 s</text><text x="${W - 10}" y="${H - 4}" text-anchor="end">now</text></svg>

    <div class="bar" style="margin-top:16px">${comp.map(c => `<i style="--k:${c[2]};width:${c[1] / m.total * 100}%"></i>`).join('')}</div>

    <div class="legend">${comp.map(c => `<span ${k(c[2])}>${c[0]} <b>${fmt(c[1])}</b></span>`).join('')}<span ${k('var(--track)')}>Free <b>${fmt(m.total - m.used - m.cached)}</b></span></div></div>

  <div class="card pad"><div class="ch"><h2>Now</h2></div><div class="gauge" style="justify-content:center">${gauge(m.score, 1.15)}</div>

    <dl class="kv" style="margin-top:14px"><dt>Physical load</dt><dd>62%</dd><dt>Commit charge</dt><dd>${Math.round(m.committed / m.commitLimit * 100)}%</dd><dt>Available</dt><dd>${fmt(m.available)}</dd><dt>Processes</dt><dd>291</dd><dt>Handles</dt><dd>148,302</dd></dl></div></div>

  <div class="card pad" style="margin-top:16px"><div class="ch"><h2>Using the most memory</h2></div><div class="rows">${memTop.map(p => `<div class="row"><span class="ico" ${k(p.color)}>${p.title[0]}</span><div class="nm"><b>${esc(p.title)}</b><small>${p.instances > 1 ? p.instances + ' processes' : esc(p.name)}</small></div><span class="v" style="width:90px">${fmt(p.memory)}</span></div>`).join('')}</div></div>`;

};



V.storage = () => {

  const maxB = Math.max(...S.buckets.map(b => b[1]));

  return head('Storage', `${S.drives.length} drives · ${fmt(S.drives.reduce((a, d) => a + d.free, 0))} free in total`, `<button class="btn" data-act="scan">${ic('search')}Find large files</button>`) +

  `<div class="card pad">${S.drives.map(d => driveBlock(d)).join('')}</div>

  <div class="grid g32" style="margin-top:16px"><div class="card pad"><div class="ch"><h2>Largest files</h2><span class="r">Over 1 GB · safe-listed folders skipped</span></div>

    <div class="rows">${S.big.map(([p, g, kind]) => `<div class="row"><span class="ico" style="--k:#F0F0F3;color:var(--ink2)">${ic('file')}</span><div class="nm"><b>${esc(p.split('\\').pop())}</b><small class="mono">${esc(p.slice(0, p.lastIndexOf('\\')))}</small></div><span class="pill">${kind}</span><span class="v" style="width:76px">${fmt(g * GB)}</span></div>`).join('')}</div></div>

  <div class="grid"><div class="card pad"><div class="ch"><h2>Files by size</h2><span class="r">${n(S.buckets.reduce((a, b) => a + b[1], 0))} files</span></div>

    <div class="histo">${S.buckets.map(([l, c]) => `<div><i style="height:${c / maxB * 64 + 2}px"></i>${l}<br><b style="color:var(--ink)">${c}</b></div>`).join('')}</div></div>

    <div class="card pad"><div class="ch"><h2>Duplicates</h2><span class="pill ok">${fmt(S.dupes.wasted)} wasted</span></div>

      <p class="muted">${S.dupes.groups} groups of identical files, matched by size, then a 64 KB fingerprint, then full content.</p>

      <button class="btn" style="margin-top:12px">${ic('copy')}Review duplicates</button></div></div></div>`;

};



V.processes = () => {

  const p = S.procs.find(x => x.name === S.sel) || S.procs[0];

  return head('Processes', `${S.procs.reduce((a, x) => a + x.instances, 0)} processes grouped into ${S.procs.length} apps · CPU ${S.cpu}%`) +

  `<div class="tools"><label class="search">${ic('search')}<input id="q" placeholder="Search apps and processes" value="${esc(S.query)}"></label>

    <div class="seg" id="sort">${['cpu', 'memory', 'name'].map(s => `<button data-sort="${s}" class="${S.sort === s ? 'on' : ''}">${s === 'cpu' ? 'CPU' : s[0].toUpperCase() + s.slice(1)}</button>`).join('')}</div>

    <span class="lbl" style="margin-left:auto">Updated every second</span></div>

  <div class="split"><div class="card table"><div class="th"><span>App · what is this?</span><span class="${S.sort === 'cpu' ? 's' : ''}">CPU</span><span class="${S.sort === 'memory' ? 's' : ''}">Memory</span><span>Procs</span></div><div class="tbody" id="tbody"></div></div>

  <div class="card pad detail"><span class="ico" style="--k:${p.color};width:40px;height:40px;border-radius:10px;font-size:17px">${p.title[0]}</span>

    <h3>${esc(p.title)}</h3><span class="mono muted">${esc(p.name)}</span><p>${esc(p.what)}.</p>

    <div class="ch" style="margin:0 0 6px"><span class="lbl">CPU · last 40 s</span><span class="r num">${p.cpu.toFixed(1)}%</span></div>${spark(p.history, 250, 54).replace('class="spark"', 'class="spark" style="width:100%;height:54px"')}

    <dl class="kv" style="margin-top:14px"><dt>Memory</dt><dd>${fmt(p.memory)}</dd><dt>Processes</dt><dd>${p.instances}</dd><dt>Type</dt><dd style="text-transform:capitalize">${p.kind}</dd><dt>Safe to end</dt><dd>${p.kind === 'system' || p.kind === 'service' ? 'No, part of Windows' : 'Yes'}</dd></dl>

    <button class="btn" style="width:100%;justify-content:center" ${p.kind === 'system' || p.kind === 'service' ? 'disabled' : ''} data-act="end">End ${esc(p.title)}</button></div></div>`;

};

const ROW = 48;

function procRows(list, rankOf) {

  const tb = $('#tbody'); if (!tb) return;

  tb.style.height = list.length * ROW + 'px';

  const maxCpu = Math.max(...S.procs.map(p => Math.max(...p.history)), 1);

  list.forEach((p, i) => {

    let r = tb.querySelector(`[data-n="${CSS.escape(p.name)}"]`);

    if (!r) { r = document.createElement('div'); r.className = 'tr'; r.dataset.n = p.name; tb.appendChild(r); }

    r.classList.toggle('sel', p.name === S.sel);

    r.innerHTML = `<div class="app"><span class="ico" style="--k:${p.color}">${p.title[0]}</span><div><b>${esc(p.title)}</b><small>${esc(p.what)}</small></div></div>

      <span class="cpu ${p.cpu >= 15 ? 'hot' : ''}">${spark(p.history.slice(-24), 70, 22, maxCpu)}${p.cpu.toFixed(1)}%</span><span class="num">${fmt(p.memory)}</span><span class="num muted">${p.instances}</span>`;

    r.style.transform = `translateY(${(rankOf ? rankOf(p, i) : i) * ROW}px)`;

  });

}

function sortedProcs() {

  const q = S.query.toLowerCase(), by = { cpu: (a, b) => b.cpu - a.cpu, memory: (a, b) => b.memory - a.memory, name: (a, b) => a.title.localeCompare(b.title) }[S.sort];

  return S.procs.filter(p => !q || (p.title + p.name).toLowerCase().includes(q)).sort(by);

}



V.startup = () => {

  const on = S.startup.filter(s => s.enabled), hi = on.filter(s => s.impact === 'high').length;

  const delay = on.reduce((a, s) => a + { high: 2.9, medium: 1.2, low: .3 }[s.impact], 0);

  return head('Startup', 'Apps that launch when you sign in to Windows, from the HKCU and HKLM Run keys') +

  `<div class="card sum"><div><span class="lbl">Start with Windows</span><b>${on.length} of ${S.startup.length}</b></div><div><span class="lbl">High impact</span><b style="color:var(--red)">${hi}</b></div>

    <div><span class="lbl">Estimated sign-in delay</span><b>+${delay.toFixed(1)} s</b></div><div><span class="lbl">If you turn off high impact</span><b style="color:var(--green-ink)">−${(hi * 2.9).toFixed(1)} s</b></div></div>

  <div class="card" style="margin-top:16px"><div class="st sh lbl"><span></span><span>App</span><span>Command</span><span>Source</span><span>Impact</span></div>

  ${S.startup.map(s => `<div class="st ${s.enabled ? '' : 'off'}"><span class="toggle ${s.enabled ? 'on' : ''}" data-tog="${esc(s.id)}"></span>

    <div class="app row" style="min-height:0"><span class="ico" style="--k:${s.color}">${s.name[0]}</span><div class="nm"><b>${esc(s.name)}</b><small>${s.enabled ? 'Enabled' : 'Disabled'}</small></div></div>

    <span class="path mono">${esc(s.command)}</span><span class="muted">${s.source}</span><span class="imp" style="--k:${impactCol[s.impact]}"><i></i>${s.impact[0].toUpperCase() + s.impact.slice(1)}</span></div>`).join('')}</div>`;

};



V.cleanup = () => {

  const total = reclaim(), p = S.cleanP, sel = S.cats.filter(c => c.selected);

  const left = c => { if (p == null || !c.selected) return c.bytes; const i = sel.indexOf(c); return c.bytes * (1 - ease(p * sel.length - i)); };

  const freed = sel.reduce((a, c) => a + c.bytes - left(c), 0), done = p != null && p >= 1;

  const sysFree = S.drives[0].free + freed;

  const ring = `<svg class="ring" viewBox="0 0 46 46"><circle cx="23" cy="23" r="20" stroke="var(--green-soft)"/><circle cx="23" cy="23" r="20" pathLength="100" stroke-dasharray="${done ? 100 : ease(p ?? 0) * 100} 100" transform="rotate(-90 23 23)"/><path d="m15 23.5 5.5 5.2L31.5 18" pathLength="100" stroke-dasharray="${done ? 100 : 0} 100" stroke-width="3"/></svg>`;

  const sub = p == null ? `Smart Scan found ${fmt(S.cats.reduce((a, c) => a + c.bytes, 0))} in ${(S.scanMs / 1000).toFixed(1)} s · ${n(S.cats.reduce((a, c) => a + c.files, 0))} files`

    : done ? (S.dry ? 'Dry run — nothing was moved' : 'Moved to quarantine — restore any file from the Safety screen within 7 days') : 'Cleaning…';

  const title = p == null ? 'Cleanup' : `<span class="done">${ring}<span><b>${fmt(freed)} ${S.dry ? 'found' : 'cleaned'}</b></span></span>`;

  return `<div class="head"><div>${p == null ? `<h1>${title}</h1>` : title}<p>${sub}</p></div></div>

  <div class="card">${S.cats.map(c => { const l = left(c), z = c.selected && p != null && l < 1; return `<div class="cat ${c.selected ? 'on' : ''} ${S.open[c.id] ? 'open' : ''}" data-cat="${c.id}">

      <span class="check">${ic('check')}</span><div class="nm"><b>${c.label}</b>${c.risk === 'review' ? ' <span class="pill warn" style="margin-left:6px">Review</span>' : ''}<small>${c.explain}</small></div>

      <span class="files">${z ? 'Cleaned' : n(c.files) + ' files'}</span><span class="size ${z ? 'zero' : ''}">${z ? ic('check') : fmt(l)}</span>${c.subs.length ? ic('chev', 'chev') : '<svg class="chev"></svg>'}</div>

      ${S.open[c.id] ? c.subs.map(([a, g]) => `<div class="sub"><span>${esc(a)}</span><span>${fmt(g * GB * (l / c.bytes))}</span></div>`).join('') : ''}`; }).join('')}

    <div class="foot"><span class="toggle ${S.dry ? 'on' : ''}" data-act="dry"></span><span>Dry run <span class="muted">· report only, delete nothing</span></span>

      <span class="note">${ic('shield')}Documents, Desktop, Program Files and 34 more are never touched</span>

      <button class="btn go big" data-act="clean" ${p != null && !done ? 'disabled' : ''}>${done ? 'Done' : p != null ? 'Cleaning…' : 'Clean ' + fmt(total)}</button></div></div>`;

};




async function loadSafety() {
  const sf = S.safety;
  if (!tauri || sf.loading) return;
  sf.loading = true;
  try {
    const st = await invoke('safety_status');
    Object.assign(sf, st, { loaded: true });
    sf.runs = await invoke('safety_recent_runs', { limit: 8 }).catch(() => sf.runs);
    if (sf.showQ) sf.quarantine = await invoke('safety_quarantine_list').catch(() => sf.quarantine);
  } catch (e) { /* keep last known state */ }
  sf.loading = false;
  if ((location.hash.slice(1) || 'overview') === 'safety') render();
}

async function runSafety(apply) {
  const sf = S.safety;
  sf.running = true; render();
  try { await invoke('safety_clean_now', { apply }); } catch (e) { /* surfaced via last run */ }
  sf.running = false;
  await loadSafety();
}

V.safety = () => {
  const sf = S.safety;
  const phase = sf.dryRunPhaseComplete, left = 3 - sf.dryRunsCompleted;
  const statusTitle = sf.running ? 'Cleanup running…'
    : !phase ? `Dry-run phase — ${sf.dryRunsCompleted} of 3 complete`
    : sf.autonomousEnabled ? 'Automatic cleanup is on' : 'Ready — automatic cleanup is off';
  const statusSub = sf.running ? 'You can pause it at any time; files already moved stay in quarantine.'
    : !phase ? `${left === 1 ? 'One more practice run unlocks' : left + ' more practice runs unlock'} automatic cleanup. Practice runs change nothing.`
    : sf.autonomousEnabled ? `Runs ${sf.scheduleMode === 'daily' ? 'daily' : 'every ' + sf.scheduleDay} at ${sf.scheduleHour}:00 · only the categories below`
    : 'Practice phase complete. Turn on automatic cleanup, or run a cleanup whenever you like.';
  const lr = sf.lastRun;
  const lastLine = !lr ? 'No runs yet'
    : lr.outcome === 'skipped' ? `Last run ${fmtRunAt(lr.at)} · skipped — ${esc(lr.detail)}`
    : `Last run ${fmtRunAt(lr.at)} · ${lr.dryRun ? 'would free' : 'freed'} ${fmt(lr.bytes)}${lr.dryRun ? ' · nothing was moved' : ''}`;
  const days = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
  const catRow = c => {
    const on = sf.enabledCategories.includes(c.id);
    const age = c.minAgeDays > 0 ? ` · older than ${c.minAgeDays} days` : '';
    return `<div class="row"><div class="nm"><b>${esc(c.displayName)}</b><small>${esc(CAT_BLURB[c.id] || '')}${age}</small></div>
      ${c.requiresAdmin ? '<span class="pill warn">Admin</span>' : ''}<span class="toggle ${on ? 'on' : ''}" data-sf-cat="${esc(c.id)}"></span></div>`;
  };
  const runRow = r => `<div class="logrow"><span class="t">${fmtRunAt(r.at)}</span>
      <span class="what">${RUN_SOURCE[r.source] || 'Run'}${r.dryRun ? ' (practice)' : ''} · <span class="${r.outcome === 'complete' ? 'ok' : 'skip'}">${esc(r.outcome)}${r.detail ? ' — ' + esc(r.detail) : ''}</span></span>
      <span class="amt">${r.bytes ? fmt(r.bytes) : '—'}</span></div>`;
  const qRow = q => `<div class="logrow"><span class="what" title="${esc(q.originalPath)}">${esc(q.originalPath.split('\\').pop())}<small class="muted" style="display:block">${esc(q.category)} · ${fmtRunAt(q.ts)}</small></span>
      <span class="amt">${fmt(q.size)}</span><button class="btn" data-act="sf-restore" data-path="${esc(q.quarantinePath)}">Restore</button></div>`;

  return head('Safety cleanup', 'Automatic cleanup that only touches caches and temp files — everything is quarantined for 7 days, never deleted outright.') +
  `<div class="grid g2">
    <div class="card pad"><div class="ch"><h2>Status</h2>${sf.running ? '<span class="pill warn">Running</span>' : ''}</div>
      <div class="statusrow"><div class="shieldic">${ic('safety')}</div><div><b>${statusTitle}</b><small>${statusSub}</small></div></div>
      ${!phase ? `<div class="sfbar"><i style="width:${sf.dryRunsCompleted / 3 * 100}%"></i></div>` : ''}
      <div class="barlbl">${lastLine}</div>
      <div class="btnrow"><button class="btn go" data-act="sf-run" ${sf.running ? 'disabled' : ''}>${sf.running ? 'Running…' : phase ? 'Run cleanup now' : 'Run practice cleanup'}</button>
      <button class="btn" data-act="sf-pause" ${sf.running ? '' : 'disabled'}>Pause</button></div></div>

    <div class="card pad"><div class="ch"><h2>Automatic cleanup</h2><span class="r">${phase ? (sf.schedulerInstalled ? 'Scheduled in Windows' : 'Not scheduled') : 'Locked until dry runs finish'}</span></div>
      <div class="rows">
      <div class="row"><div class="nm"><b>Run automatically</b><small>Only the categories below, on the schedule below</small></div>
        <span class="toggle ${sf.autonomousEnabled ? 'on' : ''} ${phase ? '' : 'locked'}" data-act="sf-auto"></span></div>
      <div class="row"><div class="nm"><b>Schedule</b></div>
        <select class="sfsel" id="sf-mode"><option value="weekly" ${sf.scheduleMode === 'weekly' ? 'selected' : ''}>Weekly</option><option value="daily" ${sf.scheduleMode === 'daily' ? 'selected' : ''}>Daily</option></select>
        <select class="sfsel" id="sf-day" ${sf.scheduleMode === 'daily' ? 'disabled' : ''}>${days.map(d => `<option ${d === sf.scheduleDay ? 'selected' : ''}>${d}</option>`).join('')}</select>
        <select class="sfsel" id="sf-hour">${Array.from({ length: 24 }, (_, h) => `<option value="${h}" ${h === sf.scheduleHour ? 'selected' : ''}>${h}:00</option>`).join('')}</select></div>
      <div class="row"><div class="nm"><b>Pause on low battery</b><small>Below ${sf.batteryPauseBelowPct}%, while on battery power</small></div>
        <span class="toggle ${sf.batteryPauseBelowPct > 0 ? 'on' : ''}" data-act="sf-battery"></span></div>
      <div class="row"><div class="nm"><b>Pause during fullscreen apps</b><small>Games and presentations are never interrupted</small></div>
        <span class="toggle ${sf.pauseOnFullscreen ? 'on' : ''}" data-act="sf-fullscreen"></span></div>
      </div></div>

    <div class="card pad"><div class="ch"><h2>Categories</h2><span class="r">${sf.enabledCategories.length} of ${sf.categories.length} on</span></div>
      <div class="rows">${sf.categories.map(catRow).join('')}</div></div>

    <div class="card pad"><div class="ch"><h2>Quarantine</h2><span class="r">${n(sf.quarantineFiles)} files · ${fmt(sf.quarantineBytes)}</span></div>
      <p><b>Nothing is deleted for 7 days</b></p>
      <p class="muted" style="margin-top:4px">Anything a cleanup moved can be put back with one click until it expires.</p>
      <div class="btnrow"><button class="btn" data-act="sf-review">${sf.showQ ? 'Hide list' : 'Review & restore…'}</button>
      <button class="btn" data-act="sf-purge">Empty now</button></div>
      ${sf.showQ ? `<div style="margin-top:10px">${sf.quarantine.length ? sf.quarantine.map(qRow).join('') : '<p class="muted">Quarantine is empty.</p>'}</div>` : ''}
      <div class="ch" style="margin-top:18px"><h2>Recent runs</h2></div>
      ${sf.runs.length ? sf.runs.map(runRow).join('') : '<p class="muted">No runs yet.</p>'}
      <div class="btnrow"><button class="btn" data-act="sf-log">Open full log</button></div></div>
  </div>`;
};


// ---------- shell ----------

const NAV = [['overview', 'Overview'], ['memory', 'Memory'], ['storage', 'Storage'], ['processes', 'Processes'], ['startup', 'Startup'], ['cleanup', 'Cleanup'], ['safety', 'Safety']];

function render() {

  const route = (location.hash.slice(1) || 'overview').split('?')[0];

  const r = V[route] ? route : 'overview';

  $('#nav').innerHTML = NAV.map(([id, l], i) => `${i === 4 ? '<div class="sep"></div>' : ''}<a href="#${id}" class="${id === r ? 'on' : ''}">${ic(id)}${l}${id === 'cleanup' && S.cleanP == null ? `<span class="badge">${fmt(reclaim())}</span>` : ''}${id === 'startup' ? `<span class="badge" style="color:var(--ink2)">${S.startup.filter(s => s.enabled).length}</span>` : ''}${id === 'safety' ? (S.safety.dryRunPhaseComplete ? (S.safety.autonomousEnabled ? '<span class="badge">On</span>' : '') : `<span class="badge">${S.safety.dryRunsCompleted} of 3</span>`) : ''}</a>`).join('');

  $('#view').innerHTML = V[r]();

  if (r === 'processes') procRows(sortedProcs());

  if (r === 'safety') loadSafety();

  $('#pc-cpu').textContent = S.cpu + '%';

}

window.addEventListener('hashchange', () => { S.cleanP = S.cleanP >= 1 ? null : S.cleanP; render(); });



function animate(ms, step) {

  return new Promise(res => { const t0 = performance.now(); const f = now => { const t = clamp((now - t0) / ms); step(t); t < 1 ? requestAnimationFrame(f) : res(); }; requestAnimationFrame(f); });

}

async function runScan() {

  const real = invoke('smart_scan').then(applyScan).catch(() => null);

  await animate(2600, t => { S.scanP = t * .98; render(); });

  await real; S.scanP = 1; render();

}

async function runClean() {

  if (S.cleanP >= 1) { S.cleanP = null; return render(); }

  if (location.hash !== '#cleanup') location.hash = '#cleanup';

  const ids = S.cats.filter(c => c.selected).map(c => c.id);

  invoke('clean', { categories: ids, dryRun: S.dry }).catch(() => null);

  await animate(2200, t => { S.cleanP = t; render(); });

}



document.addEventListener('click', e => {

  const t = e.target.closest('[data-act],[data-cat],[data-sort],[data-tog],[data-win],[data-sf-cat],.tr');

  if (!t) return;

  if (t.dataset.win) return tauri && tauri.window.getCurrentWindow()[t.dataset.win]();

  const a = t.dataset.act;

  if (a === 'scan') return runScan();

  if (a === 'clean') return runClean();

  if (a === 'dry') S.dry = !S.dry;

  if (a === 'purge') invoke('purge_standby').catch(err => alert(String(err).replace('Error: mock', 'Freeing standby memory needs administrator rights (coming in 1.1).')));

  if (a === 'end') invoke('end_process', { name: S.sel }).catch(() => null);

  if (a === 'sf-run') return runSafety(S.safety.dryRunPhaseComplete);

  if (a === 'sf-pause') invoke('safety_cancel').catch(() => null);

  if (a === 'sf-auto') { const sf = S.safety; if (sf.dryRunPhaseComplete) { invoke('safety_set_autonomous', { enabled: !sf.autonomousEnabled }).then(() => loadSafety()).catch(err => alert(String(err))); } }

  if (a === 'sf-battery') { const sf = S.safety; invoke('safety_set_pauses', { pauseOnFullscreen: sf.pauseOnFullscreen, batteryPauseBelowPct: sf.batteryPauseBelowPct > 0 ? 0 : 30 }).then(() => loadSafety()).catch(() => null); }

  if (a === 'sf-fullscreen') { const sf = S.safety; invoke('safety_set_pauses', { pauseOnFullscreen: !sf.pauseOnFullscreen, batteryPauseBelowPct: sf.batteryPauseBelowPct }).then(() => loadSafety()).catch(() => null); }

  if (a === 'sf-review') { S.safety.showQ = !S.safety.showQ; if (S.safety.showQ) invoke('safety_quarantine_list').then(q => { S.safety.quarantine = q; render(); }).catch(() => null); }

  if (a === 'sf-restore') invoke('safety_restore', { path: t.dataset.path }).then(() => loadSafety()).catch(err => alert(String(err)));

  if (a === 'sf-purge') { if (confirm('Empty the quarantine now? Files in it will be permanently deleted.')) invoke('safety_purge_quarantine').then(() => loadSafety()).catch(() => null); }

  if (a === 'sf-log') invoke('safety_open_logs').catch(() => null);

  if (t.dataset.cat) {

    const c = S.cats.find(x => x.id === t.dataset.cat);

    if (e.target.closest('.chev')) S.open[c.id] = !S.open[c.id]; else if (S.cleanP == null) c.selected = !c.selected;

  }

  if (t.dataset.sort) S.sort = t.dataset.sort;

  if (t.dataset.sfCat) { const sf = S.safety; const id = t.dataset.sfCat; const ids = sf.enabledCategories.includes(id) ? sf.enabledCategories.filter(x => x !== id) : [...sf.enabledCategories, id]; invoke('safety_set_categories', { enabledIds: ids }).then(() => loadSafety()).catch(err => alert(String(err))); }

  if (t.dataset.tog) { const s = S.startup.find(x => x.id === t.dataset.tog); invoke('set_startup_enabled', { id: s.id, enabled: !s.enabled }).catch(() => null); s.enabled = !s.enabled; }

  if (t.classList.contains('tr')) S.sel = t.dataset.n;

  render();

});

document.addEventListener('input', e => { if (e.target.id === 'q') { S.query = e.target.value; $('#tbody').innerHTML = ''; procRows(sortedProcs()); } });

document.addEventListener('change', e => {

  if (!['sf-mode', 'sf-day', 'sf-hour'].includes(e.target.id)) return;

  const sf = S.safety;

  invoke('safety_set_schedule', { mode: $('#sf-mode').value, day: $('#sf-day').value, hour: Number($('#sf-hour').value) })

    .then(() => loadSafety()).catch(err => alert(String(err)));

});



// ---------- live data from the Rust backend ----------

const COLORS = ['#6A8FD8', '#E3AC4A', '#3FAE68', '#9C86D4', '#E4836F', '#4D6A8C', '#8E8E93'];

const colorOf = s => COLORS[[...s].reduce((a, c) => a + c.charCodeAt(0), 0) % COLORS.length];

function applyScan(r) {

  S.scanMs = r.durationMs; S.threads = r.threads;

  S.cats = r.categories.map(c => ({ ...c, subs: [] }));

  const junk = S.drives[0] && S.drives[0].segments.find(s => s.id === 'junk'); if (junk) junk.bytes = reclaim();

}

function applyProcs(rows) {

  S.procs = rows.map(p => ({ ...p, what: p.what || 'No description yet', color: colorOf(p.name), history: p.history.length > 1 ? p.history : [p.cpu, p.cpu] }));

}

async function live() {

  try {

    const o = await invoke('get_overview');

    Object.assign(S, { host: o.host, os: o.os, cpu: Math.round(o.cpuPercent) });

    S.mem = { ...o.memory, score: o.memory.pressureScore };

    S.drives = o.drives.map(d => ({ ...d, kind: d.kind, get free() { return d.free; } }));

    S.startup = (await invoke('list_startup')).map(s => ({ ...s, color: colorOf(s.name) }));

    applyProcs(await invoke('list_processes', { sort: S.sort }));

    $('#pc-name').textContent = o.host; $('#pc-os').textContent = o.os;

    render();

    if ((await invoke('get_settings')).scanOnLaunch) runScan();

    setInterval(async () => {

      applyProcs(await invoke('list_processes', { sort: S.sort }));

      S.mem = { ...(await invoke('get_memory')) }; S.mem.score = S.mem.pressureScore;

      S.memHist = [...S.memHist.slice(1), S.mem.score];

      if (location.hash === '#processes') procRows(sortedProcs());

    }, 1500);

    tauri.event.listen('tray-scan', runScan);

  } catch (e) { console.error(e); }

}



window.Tidy = { S, render, procRows, sortedProcs, fmt, spark, GB };

render();

if (tauri) live();

