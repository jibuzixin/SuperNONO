const state = {
  animations: [],
  current: null,
  frames: [],
  colorMaps: [],
  units: [],
  selected: new Set(),
  frameIndex: 0,
  mode: 'parts',
  mask: document.createElement('canvas'),
  mctx: null,
  brush: 10,
  overlay: document.createElement('canvas'),
};

const view = document.getElementById('view');
const vctx = view.getContext('2d');
const listEl = document.getElementById('list');
const titleEl = document.getElementById('title');
const statusEl = document.getElementById('status');
const selectedEl = document.getElementById('selected');
const countEl = document.getElementById('count');
const selectedWrap = document.getElementById('selectedWrap');
const frameEl = document.getElementById('frame');
const frameLabel = document.getElementById('frameLabel');
const brushWrap = document.getElementById('brushWrap');
const saveMaskBtn = document.getElementById('saveMask');

const KIND_LABEL = { walk: '走路', action: '动作', expr: '表情', expression: '表情' };
const flatId = (id) => id.replaceAll('/', '_');
const colorKey = (c) => (c[0] << 16) | (c[1] << 8) | c[2];
const unitKey = (u) => `${u.layer}|${u.paths.join(',')}`;
const unitByColor = new Map();

async function boot() {
  const res = await fetch('/manifest.json');
  const data = await res.json();
  state.animations = data.animations || [];
  buildList();
}

function buildList() {
  listEl.innerHTML = '';
  for (const form of [...new Set(state.animations.map(a => a.form))].sort()) {
    const h = document.createElement('h2');
    h.textContent = `形态 ${form}`;
    listEl.appendChild(h);
    for (const kind of [...new Set(state.animations.filter(a => a.form === form).map(a => a.kind))]) {
      const hk = document.createElement('h2');
      hk.textContent = KIND_LABEL[kind] || kind;
      listEl.appendChild(hk);
      for (const anim of state.animations.filter(a => a.form === form && a.kind === kind)) {
        const el = document.createElement('div');
        el.className = 'item';
        el.textContent = anim.slug;
        el.dataset.id = anim.id;
        el.onclick = () => select(anim);
        listEl.appendChild(el);
      }
    }
  }
}

async function select(anim) {
  state.current = anim;
  state.selected.clear();
  state.frameIndex = 0;
  document.querySelectorAll('.item').forEach(e => e.classList.toggle('active', e.dataset.id === anim.id));
  titleEl.textContent = anim.id;
  statusEl.textContent = '加载中…';

  const [w, h] = anim.frame_size;
  view.width = w; view.height = h;
  const zoom = Math.max(1, Math.min(3, 760 / w));
  view.style.width = `${w * zoom}px`;
  view.style.height = `${h * zoom}px`;

  state.mask.width = w; state.mask.height = h;
  state.mctx = state.mask.getContext('2d');
  state.mctx.clearRect(0, 0, w, h);
  state.overlay.width = w; state.overlay.height = h;

  const flat = flatId(anim.id);
  const [frames, meta] = await Promise.all([
    Promise.all(anim.frames.map(rel => loadImage(`/${rel}`))),
    fetch(`/layers/${flat}.json`).then(r => r.ok ? r.json() : null).catch(() => null),
  ]);
  state.frames = frames;
  state.units = meta ? (meta.units || []) : [];
  state.colorMaps = [];
  unitByColor.clear();
  for (const u of state.units) unitByColor.set(colorKey(u.color), u);
  if (meta) {
    state.colorMaps = await Promise.all(anim.frames.map((_, i) => loadColorMap(`/layers/${flat}/${String(i).padStart(2, '0')}.png`)));
  }
  frameEl.max = Math.max(0, anim.frames.length - 1);
  frameEl.value = 0;
  frameLabel.textContent = `0 / ${Math.max(0, anim.frames.length - 1)}`;

  // existing mask
  try {
    const r = await fetch(`/masks/${flat}.png`, { cache: 'no-store' });
    if (r.ok) {
      const bmp = await createImageBitmap(await r.blob());
      state.mctx.drawImage(bmp, 0, 0, w, h);
    }
  } catch (_) {}
  // existing parts
  try {
    const r = await fetch(`/parts/${flat}.json`, { cache: 'no-store' });
    if (r.ok) {
      const existing = await r.json();
      for (const p of (existing.parts || [])) {
        if (typeof p === 'number') continue;
        const u = state.units.find(x => x.layer === p.layer && x.paths.join(',') === (p.paths || []).join(','));
        if (u) state.selected.add(unitKey(u));
      }
    }
  } catch (_) {}
  statusEl.textContent = state.units.length ? `${state.units.length} 个部件，${state.frames.length} 帧` : '没有部件映射';
  draw();
  renderSelected();
}

function loadImage(src) {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = src;
  });
}

async function loadColorMap(src) {
  const img = await loadImage(src);
  if (!img) return null;
  const off = document.createElement('canvas');
  off.width = img.naturalWidth; off.height = img.naturalHeight;
  const octx = off.getContext('2d');
  octx.drawImage(img, 0, 0);
  return octx.getImageData(0, 0, off.width, off.height);
}

function activeMap() { return state.colorMaps[state.frameIndex] || null; }

function draw() {
  vctx.clearRect(0, 0, view.width, view.height);
  const frame = state.frames[state.frameIndex];
  if (frame) vctx.drawImage(frame, 0, 0);
  if (state.mode === 'brush') {
    // green overlay of the painted mask
    const ov = state.overlay;
    const octx = ov.getContext('2d');
    octx.clearRect(0, 0, ov.width, ov.height);
    octx.drawImage(state.mask, 0, 0);
    octx.globalCompositeOperation = 'source-in';
    octx.fillStyle = 'rgba(60, 220, 120, 1)';
    octx.fillRect(0, 0, ov.width, ov.height);
    octx.globalCompositeOperation = 'source-over';
    vctx.globalAlpha = 0.45;
    vctx.drawImage(ov, 0, 0);
    vctx.globalAlpha = 1;
    return;
  }
  const cmap = activeMap();
  if (cmap && state.selected.size) {
    const { width, height, data } = cmap;
    const ov = state.overlay;
    ov.width = width; ov.height = height;
    const octx = ov.getContext('2d');
    const out = octx.createImageData(width, height);
    const selectedColors = new Set(state.units.filter(u => state.selected.has(unitKey(u))).map(u => colorKey(u.color)));
    for (let p = 0; p < data.length; p += 4) {
      if (data[p + 3] < 128) continue;
      const key = (data[p] << 16) | (data[p + 1] << 8) | data[p + 2];
      if (!selectedColors.has(key)) continue;
      out.data[p] = 255; out.data[p + 1] = 70; out.data[p + 2] = 120; out.data[p + 3] = 255;
    }
    octx.putImageData(out, 0, 0);
    vctx.globalAlpha = 0.55;
    vctx.drawImage(ov, 0, 0);
    vctx.globalAlpha = 1;
  }
}

function paintAt(clientX, clientY) {
  const r = view.getBoundingClientRect();
  const x = (clientX - r.left) * view.width / r.width;
  const y = (clientY - r.top) * view.height / r.height;
  const ctx = state.mctx;
  ctx.globalCompositeOperation = state.erasing ? 'destination-out' : 'source-over';
  ctx.fillStyle = '#ffffff';
  ctx.beginPath();
  ctx.arc(x, y, state.brush, 0, Math.PI * 2);
  ctx.fill();
  draw();
}

function pick(clientX, clientY) {
  const cmap = activeMap();
  if (!cmap) return;
  const r = view.getBoundingClientRect();
  const x = Math.floor((clientX - r.left) * view.width / r.width);
  const y = Math.floor((clientY - r.top) * view.height / r.height);
  if (x < 0 || y < 0 || x >= cmap.width || y >= cmap.height) return;
  const i = (y * cmap.width + x) * 4;
  const d = cmap.data;
  if (d[i + 3] < 128) return;
  const key = (d[i] << 16) | (d[i + 1] << 8) | d[i + 2];
  let unit = unitByColor.get(key);
  if (!unit) {
    let best = null, bestDist = 60 * 60;
    for (const u of state.units) {
      const dr = u.color[0] - d[i], dg = u.color[1] - d[i + 1], db = u.color[2] - d[i + 2];
      const dist = dr * dr + dg * dg + db * db;
      if (dist < bestDist) { bestDist = dist; best = u; }
    }
    unit = best;
  }
  if (!unit) return;
  const k = unitKey(unit);
  if (state.selected.has(k)) state.selected.delete(k);
  else state.selected.add(k);
  draw();
  renderSelected();
}

view.addEventListener('pointerdown', (e) => {
  if (!state.current) return;
  e.preventDefault();
  if (state.mode === 'brush') { state.painting = true; view.setPointerCapture(e.pointerId); paintAt(e.clientX, e.clientY); }
  else pick(e.clientX, e.clientY);
});
view.addEventListener('pointermove', (e) => { if (state.painting && state.mode === 'brush') paintAt(e.clientX, e.clientY); });
view.addEventListener('pointerup', () => { state.painting = false; });
view.addEventListener('pointercancel', () => { state.painting = false; });

function setMode(mode) {
  state.mode = mode;
  document.getElementById('modeParts').classList.toggle('active', mode === 'parts');
  document.getElementById('modeBrush').classList.toggle('active', mode === 'brush');
  brushWrap.hidden = mode !== 'brush';
  saveMaskBtn.hidden = mode !== 'brush';
  selectedWrap.hidden = mode !== 'parts';
  draw();
}

document.getElementById('modeParts').onclick = () => setMode('parts');
document.getElementById('modeBrush').onclick = () => setMode('brush');
document.getElementById('brush').oninput = (e) => {
  state.brush = Number(e.target.value);
  document.getElementById('brushVal').textContent = state.brush;
};
frameEl.oninput = (e) => {
  state.frameIndex = Number(e.target.value);
  frameLabel.textContent = `${state.frameIndex} / ${state.frames.length - 1}`;
  draw();
};
document.getElementById('prev').onclick = () => {
  state.frameIndex = Math.max(0, state.frameIndex - 1);
  frameEl.value = state.frameIndex;
  frameLabel.textContent = `${state.frameIndex} / ${state.frames.length - 1}`;
  draw();
};
document.getElementById('next').onclick = () => {
  state.frameIndex = Math.min(state.frames.length - 1, state.frameIndex + 1);
  frameEl.value = state.frameIndex;
  frameLabel.textContent = `${state.frameIndex} / ${state.frames.length - 1}`;
  draw();
};
document.getElementById('clear').onclick = () => {
  if (state.mode === 'brush') {
    state.mctx.clearRect(0, 0, state.mask.width, state.mask.height);
    draw();
  } else {
    state.selected.clear(); draw(); renderSelected();
  }
};
document.getElementById('save').onclick = async () => {
  if (!state.current) return;
  const flat = flatId(state.current.id);
  const parts = state.units
    .filter(u => state.selected.has(unitKey(u)))
    .map(u => ({ layer: u.layer, paths: u.paths }));
  const res = await fetch(`/parts/${flat}.json`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: state.current.id, parts }),
  });
  statusEl.textContent = res.ok ? `已保存 ${parts.length} 个部件` : '保存失败';
};
saveMaskBtn.onclick = async () => {
  if (!state.current) return;
  const flat = flatId(state.current.id);
  const blob = await new Promise((resolve) => state.mask.toBlob(resolve, 'image/png'));
  const res = await fetch(`/mask/${flat}.png`, { method: 'POST', body: blob });
  statusEl.textContent = res.ok ? '已保存遮罩（优先生效）' : '保存失败';
};

function renderSelected() {
  selectedEl.innerHTML = '';
  const list = state.units.filter(u => state.selected.has(unitKey(u)));
  countEl.textContent = String(list.length);
  for (const u of list) {
    const el = document.createElement('span');
    el.className = 'chip';
    el.textContent = `${u.nm || '部件'} L${u.layer} [${u.paths.join(',')}]`;
    const x = document.createElement('button');
    x.textContent = '×';
    x.onclick = () => { state.selected.delete(unitKey(u)); draw(); renderSelected(); };
    el.appendChild(x);
    selectedEl.appendChild(el);
  }
}

boot();
