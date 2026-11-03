const state = {
  animations: [],
  current: null,
  frames: [],
  colorMaps: [],
  layers: [],
  selected: new Set(),
  frameIndex: 0,
  overlay: document.createElement('canvas'),
};

const view = document.getElementById('view');
const vctx = view.getContext('2d');
const listEl = document.getElementById('list');
const titleEl = document.getElementById('title');
const statusEl = document.getElementById('status');
const selectedEl = document.getElementById('selected');
const countEl = document.getElementById('count');
const frameEl = document.getElementById('frame');
const frameLabel = document.getElementById('frameLabel');

const KIND_LABEL = { walk: '走路', action: '动作', expr: '表情', expression: '表情' };
const flatId = (id) => id.replaceAll('/', '_');
const colorKey = (c) => (c[0] << 16) | (c[1] << 8) | c[2];

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

  const flat = flatId(anim.id);
  const [frames, meta] = await Promise.all([
    Promise.all(anim.frames.map(rel => loadImage(`/${rel}`))),
    fetch(`/layers/${flat}.json`).then(r => r.ok ? r.json() : null).catch(() => null),
  ]);
  state.frames = frames;
  state.layers = meta ? meta.layers : [];
  state.colorMaps = [];
  if (meta) {
    state.colorMaps = await Promise.all(anim.frames.map((_, i) => loadColorMap(`/layers/${flat}/${String(i).padStart(2, '0')}.png`)));
  }
  frameEl.max = Math.max(0, anim.frames.length - 1);
  frameEl.value = 0;
  frameLabel.textContent = `0 / ${Math.max(0, anim.frames.length - 1)}`;

  try {
    const r = await fetch(`/parts/${flat}.json`, { cache: 'no-store' });
    if (r.ok) {
      const existing = await r.json();
      for (const v of (existing.layers || [])) state.selected.add(Number(v));
    }
  } catch (_) {}
  statusEl.textContent = state.layers.length ? `${state.layers.length} 层，${state.frames.length} 帧` : '没有图层映射';
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
  const cmap = activeMap();
  if (cmap && state.selected.size) {
    const { width, height, data } = cmap;
    const ov = state.overlay;
    ov.width = width; ov.height = height;
    const octx = ov.getContext('2d');
    const out = octx.createImageData(width, height);
    const selectedColors = new Set(state.layers.filter(l => state.selected.has(l.i)).map(l => colorKey(l.color)));
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
  let layer = state.layers.find(l => colorKey(l.color) === key);
  if (!layer) {
    let best = null, bestDist = 60 * 60;
    for (const l of state.layers) {
      const dr = l.color[0] - d[i], dg = l.color[1] - d[i + 1], db = l.color[2] - d[i + 2];
      const dist = dr * dr + dg * dg + db * db;
      if (dist < bestDist) { bestDist = dist; best = l; }
    }
    layer = best;
  }
  if (!layer) return;
  if (state.selected.has(layer.i)) state.selected.delete(layer.i);
  else state.selected.add(layer.i);
  draw();
  renderSelected();
}

function renderSelected() {
  selectedEl.innerHTML = '';
  const list = state.layers.filter(l => state.selected.has(l.i));
  countEl.textContent = String(list.length);
  for (const l of list) {
    const el = document.createElement('span');
    el.className = 'chip';
    el.textContent = `${l.nm} #${l.i}`;
    const x = document.createElement('button');
    x.textContent = '×';
    x.onclick = () => { state.selected.delete(l.i); draw(); renderSelected(); };
    el.appendChild(x);
    selectedEl.appendChild(el);
  }
}

view.addEventListener('pointerdown', (e) => { e.preventDefault(); pick(e.clientX, e.clientY); });

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
  state.selected.clear(); draw(); renderSelected();
};
document.getElementById('save').onclick = async () => {
  if (!state.current) return;
  const flat = flatId(state.current.id);
  const res = await fetch(`/parts/${flat}.json`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: state.current.id, layers: [...state.selected].sort((a, b) => a - b) }),
  });
  statusEl.textContent = res.ok ? `已保存 ${state.selected.size} 层` : '保存失败';
};

boot();
