//! Glow: reconstructs the source SWF's GLOW filter at runtime.
//!
//! The Lottie assets cannot carry a real glow (no blur filter), but the labels
//! carry the original filter parameters per layer (`nono.glow`, with
//! `baked: true` meaning "already drawn as stroke stacks, do not light it
//! twice").  Older assets only have the top-level `nono.glowShapes` table, which
//! we still accept as a fallback keyed by source shape id.
//!
//! What we build: one small Lottie per (colour, blur, strength) group that
//! contains only the glowing layers, flat-filled with the glow colour.  The
//! renderer blurs it and composites it *under* the character.  Because the
//! layers keep their own opacity animation, the source's pulsing glow comes for
//! free.
//!
//! Parsing is memory-lean on purpose: a 13 MB Lottie becomes a ~260 MB tree if
//! materialised whole (the key-order-preserving tree ThorVG needs), so the
//! document is read as borrowed raw JSON and only the layers we keep are
//! materialised.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::{JsonTree, RenderError};

/// Bumped whenever the generated glow documents change shape.
const GLOW_VERSION: u32 = 3;

/// Source entries with this colour are the raw filter colour before the
/// placement's colour transform (form 3).  They are drawn as the asset's cyan
/// glow instead: the reference renders show no red at all.
const RED_SOURCE: [f32; 3] = [1.0, 0.0, 0.0];
const RED_FIX: [f32; 3] = [50.0 / 255.0, 254.0 / 255.0, 254.0 / 255.0];

/// Tunables for the runtime glow.  Kept deliberately small: the whole point of
/// doing this at runtime is that these can be tweaked without touching assets.
#[derive(Debug, Clone, Copy)]
pub struct GlowSettings {
    pub enabled: bool,
    /// Blur radius = the asset's source blur × this factor.
    pub sigma_scale: f32,
    /// Global brightness multiplier on top of the source strength.
    pub alpha_scale: f32,
    /// Pixel budget for all glow passes of one animation (they share it).
    pub max_pixels: f32,
    /// ThorVG blur quality (0-100); lower is cheaper and boxier.
    pub quality: u8,
}

impl Default for GlowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sigma_scale: 0.6,
            alpha_scale: 1.0,
            max_pixels: 200_000.0,
            quality: 80,
        }
    }
}

impl GlowSettings {
    /// Pixel size of the glow buffers for one animation, given its canvas.
    ///
    /// Every pass gets an equal share of the budget: an animation with six glow
    /// groups is blurred at a lower resolution than one with a single group.
    pub fn glow_scale(&self, passes: usize, width: u32, height: u32) -> f32 {
        let area = f64::from(width) * f64::from(height);
        if area <= 0.0 || passes == 0 {
            return 1.0;
        }
        let budget = f64::from(self.max_pixels.max(1.0)) / passes as f64;
        ((budget / area).sqrt() as f32).clamp(0.25, 1.0)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GlowPassSpec {
    pub path: PathBuf,
    pub blur: f32,
    pub strength: f32,
}

// ── small JSON helpers ─────────────────────────────────────────────

fn array(node: &JsonTree) -> Option<&[JsonTree]> {
    match node {
        JsonTree::Array(items) => Some(items),
        _ => None,
    }
}

fn number(node: &JsonTree) -> Option<f64> {
    match node {
        JsonTree::Number(value) => value.as_f64(),
        _ => None,
    }
}

fn truthy(node: &JsonTree) -> bool {
    match node {
        JsonTree::Bool(value) => *value,
        JsonTree::Number(value) => value.as_f64().unwrap_or(0.0) != 0.0,
        _ => false,
    }
}

// ── layer metadata ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GlowParams {
    color: [f32; 3],
    blur: f32,
    strength: f32,
    baked: bool,
}

#[derive(Debug)]
struct LayerMeta {
    ind: Option<i64>,
    parent: Option<i64>,
    is_matte: bool,
    has_matte: bool,
    glow: Option<GlowParams>,
}

fn parse_glow(node: &JsonTree) -> Option<GlowParams> {
    let strength = node.get("strength").and_then(number).unwrap_or(0.0) as f32;
    let baked = node.get("baked").is_some_and(truthy);
    if strength <= 0.0 {
        return None;
    }
    let mut color = [1.0f32; 3];
    if let Some(items) = node.get("color").and_then(array) {
        for (slot, item) in color.iter_mut().zip(items.iter()) {
            *slot = number(item).unwrap_or(255.0) as f32 / 255.0;
        }
    }
    if color
        .iter()
        .zip(RED_SOURCE.iter())
        .all(|(a, b)| (a - b).abs() < 0.01)
    {
        color = RED_FIX;
    }
    Some(GlowParams {
        color,
        blur: node.get("blur").and_then(number).unwrap_or(0.0) as f32,
        strength,
        baked,
    })
}

fn read_meta(layer: &JsonTree) -> LayerMeta {
    let nono = layer.get("nono");
    LayerMeta {
        ind: layer.get("ind").and_then(|v| v.as_i64()),
        parent: layer.get("parent").and_then(|v| v.as_i64()),
        is_matte: layer.get("td").is_some_and(truthy),
        has_matte: layer.get("tt").is_some_and(truthy),
        glow: nono.and_then(|n| n.get("glow")).and_then(parse_glow),
    }
}

// ── planning ───────────────────────────────────────────────────────

#[derive(Debug)]
struct Group {
    color: [f32; 3],
    blur: f32,
    strength: f32,
    positions: Vec<usize>,
}

fn plan(metas: &[LayerMeta]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<(u32, u32, u32, u32, u32), usize> = HashMap::new();
    for (position, meta) in metas.iter().enumerate() {
        // `nono.glow` is the single switch: present = draw, absent = never.
        let Some(params) = meta.glow.clone() else { continue };
        if params.baked {
            continue;
        }
        let key = (
            (params.color[0] * 255.0) as u32,
            (params.color[1] * 255.0) as u32,
            (params.color[2] * 255.0) as u32,
            (params.blur * 10.0) as u32,
            (params.strength * 100.0) as u32,
        );
        let slot = *index.entry(key).or_insert_with(|| {
            groups.push(Group {
                color: params.color,
                blur: params.blur,
                strength: params.strength,
                positions: Vec::new(),
            });
            groups.len() - 1
        });
        groups[slot].positions.push(position);
    }
    groups
}

/// Adds parent layers and track-matte partners: dropping them would move or
/// unmask the kept layers.
fn expand_keep(keep: &mut HashSet<usize>, metas: &[LayerMeta]) {
    let mut first_by_ind: HashMap<i64, usize> = HashMap::new();
    for (i, meta) in metas.iter().enumerate() {
        if let Some(ind) = meta.ind {
            first_by_ind.entry(ind).or_insert(i);
        }
    }
    fn add_ancestors(
        keep: &mut HashSet<usize>,
        metas: &[LayerMeta],
        first_by_ind: &HashMap<i64, usize>,
        start: usize,
    ) {
        let mut cursor = metas[start].parent;
        while let Some(parent) = cursor {
            let Some(&pos) = first_by_ind.get(&parent) else { break };
            keep.insert(pos);
            cursor = metas[pos].parent;
        }
    }
    for position in keep.clone() {
        add_ancestors(keep, metas, &first_by_ind, position);
    }
    for i in 0..metas.len() {
        if keep.contains(&i) && metas[i].has_matte {
            let mut j = i;
            while j > 0 {
                j -= 1;
                if metas[j].is_matte {
                    keep.insert(j);
                    add_ancestors(keep, metas, &first_by_ind, j);
                    break;
                }
            }
        }
    }
}

// ── document building ──────────────────────────────────────────────

fn recolor(node: &mut JsonTree, color: [f32; 3]) {
    match node {
        JsonTree::Array(items) => {
            for item in items {
                recolor(item, color);
            }
        }
        JsonTree::Object(entries) => {
            let kind = entries
                .iter()
                .find(|(key, _)| key == "ty")
                .and_then(|(_, value)| value.as_str())
                .map(str::to_owned);
            if matches!(kind.as_deref(), Some("fl") | Some("st")) {
                for (key, value) in entries.iter_mut() {
                    match key.as_str() {
                        "c" => {
                            *value = JsonTree::Object(vec![
                                ("a".into(), JsonTree::number(0.0)),
                                (
                                    "k".into(),
                                    JsonTree::Array(vec![
                                        JsonTree::number(color[0]),
                                        JsonTree::number(color[1]),
                                        JsonTree::number(color[2]),
                                        JsonTree::number(1.0),
                                    ]),
                                ),
                            ])
                        }
                        "o" => *value = JsonTree::number(100.0),
                        _ => {}
                    }
                }
            }
            for (_, value) in entries.iter_mut() {
                recolor(value, color);
            }
        }
        _ => {}
    }
}

fn collect_ref_ids(node: &JsonTree, out: &mut HashSet<String>) {
    match node {
        JsonTree::Array(items) => items.iter().for_each(|item| collect_ref_ids(item, out)),
        JsonTree::Object(entries) => {
            for (key, value) in entries {
                if key == "refId" {
                    if let Some(id) = value.as_str() {
                        out.insert(id.to_string());
                    }
                } else {
                    collect_ref_ids(value, out);
                }
            }
        }
        _ => {}
    }
}

/// Keeps only the precomps the kept layers actually reference, recursively.
fn prune_assets(assets: &[&RawValue], wanted: &HashSet<String>) -> Vec<String> {
    let mut entries: Vec<(String, String, HashSet<String>)> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for asset in assets {
        let text = asset.get();
        let Ok(tree) = serde_json::from_str::<JsonTree>(text) else { continue };
        let Some(id) = tree.get("id").and_then(JsonTree::as_str) else { continue };
        let mut refs = HashSet::new();
        collect_ref_ids(&tree, &mut refs);
        by_id.insert(id.to_string(), entries.len());
        entries.push((id.to_string(), text.to_string(), refs));
    }

    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = wanted.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        if let Some(&slot) = by_id.get(&id) {
            for child in &entries[slot].2 {
                if !reachable.contains(child) {
                    queue.push(child.clone());
                }
            }
        }
    }

    entries
        .into_iter()
        .filter(|(id, _, _)| reachable.contains(id))
        .map(|(_, text, _)| text)
        .collect()
}

#[derive(Deserialize)]
struct RawDoc<'a> {
    #[serde(borrow, default)]
    v: Option<&'a RawValue>,
    #[serde(borrow, default)]
    fr: Option<&'a RawValue>,
    #[serde(borrow, default)]
    ip: Option<&'a RawValue>,
    #[serde(borrow, default)]
    op: Option<&'a RawValue>,
    #[serde(borrow, default)]
    w: Option<&'a RawValue>,
    #[serde(borrow, default)]
    h: Option<&'a RawValue>,
    #[serde(borrow, default)]
    ddd: Option<&'a RawValue>,
    #[serde(borrow, default)]
    assets: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    layers: Vec<&'a RawValue>,
}

fn build_document(doc: &RawDoc<'_>, metas: &[LayerMeta], group: &Group, index: usize) -> String {
    let mut keep: HashSet<usize> = group.positions.iter().copied().collect();
    expand_keep(&mut keep, metas);

    let mut layers: Vec<JsonTree> = Vec::with_capacity(keep.len());
    let mut refs = HashSet::new();
    for position in keep.iter().copied() {
        let Ok(tree) = serde_json::from_str::<JsonTree>(doc.layers[position].get()) else {
            continue;
        };
        collect_ref_ids(&tree, &mut refs);
        let mut layer = tree;
        recolor(&mut layer, group.color);
        layers.push(layer);
    }

    let mut out = String::with_capacity(8192);
    out.push('{');
    for (key, value) in [
        ("v", doc.v),
        ("fr", doc.fr),
        ("ip", doc.ip),
        ("op", doc.op),
        ("w", doc.w),
        ("h", doc.h),
        ("ddd", doc.ddd),
    ] {
        if let Some(value) = value {
            out.push('"');
            out.push_str(key);
            out.push_str("\":");
            out.push_str(value.get());
            out.push(',');
        }
    }
    out.push_str("\"nm\":\"glow");
    out.push_str(&index.to_string());
    out.push_str("\",\"assets\":[");
    let assets = prune_assets(&doc.assets, &refs);
    for (i, asset) in assets.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(asset);
    }
    out.push_str("],\"layers\":[");
    for (i, layer) in layers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(layer).unwrap_or_default());
    }
    out.push_str("]}");
    out
}

// ── cache ──────────────────────────────────────────────────────────

fn cache_key(file: &Path, bytes: &[u8], mtime: u128) -> String {
    let mut hasher = DefaultHasher::new();
    file.hash(&mut hasher);
    bytes.len().hash(&mut hasher);
    mtime.hash(&mut hasher);
    GLOW_VERSION.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn read_cached(dir: &Path) -> Option<Vec<GlowPassSpec>> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut out = Vec::new();
    for entry in value.get("passes")?.as_array()? {
        let path = dir.join(entry.get("file")?.as_str()?);
        if !path.is_file() {
            return None;
        }
        out.push(GlowPassSpec {
            path,
            blur: entry.get("blur")?.as_f64()? as f32,
            strength: entry.get("strength")?.as_f64()? as f32,
        });
    }
    Some(out)
}

fn write_manifest(dir: &Path, specs: &[GlowPassSpec]) -> std::io::Result<()> {
    let passes: Vec<serde_json::Value> = specs
        .iter()
        .map(|spec| {
            serde_json::json!({
                "file": spec.path.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
                "blur": spec.blur,
                "strength": spec.strength,
            })
        })
        .collect();
    let text = serde_json::json!({ "version": GLOW_VERSION, "passes": passes });
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&text)?)
}

/// Builds (or reuses) the glow passes for `file`.
///
/// Returns an empty list for assets without glow labels — the caller then pays
/// nothing at all.
pub(crate) fn passes(
    file: &Path,
    cache_root: Option<&Path>,
) -> Result<Vec<GlowPassSpec>, RenderError> {
    let bytes = std::fs::read(file)?;
    // Cheap gate: no labels and no legacy table, no parsing at all.
    if !bytes.windows(6).any(|w| w == b"glow\":") {
        return Ok(Vec::new());
    }
    let mtime = std::fs::metadata(file)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = cache_root
        .map(|root| root.join("glow").join(cache_key(file, &bytes, mtime)))
        .unwrap_or_else(|| std::env::temp_dir().join("nono-glow"));

    if cache_root.is_some() {
        if let Some(specs) = read_cached(&dir) {
            return Ok(specs);
        }
    }

    let doc: RawDoc<'_> = serde_json::from_slice(&bytes)?;

    // Two passes over the layers: metadata first (memory stays small), then the
    // kept ones are materialised.
    let mut metas = Vec::with_capacity(doc.layers.len());
    for layer in &doc.layers {
        match serde_json::from_str::<JsonTree>(layer.get()) {
            Ok(tree) => metas.push(read_meta(&tree)),
            Err(_) => metas.push(LayerMeta {
                ind: None,
                parent: None,
                is_matte: false,
                has_matte: false,
                glow: None,
            }),
        }
    }
    let groups = plan(&metas);
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(&dir)?;

    let mut specs = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        let document = build_document(&doc, &metas, group, index);
        let path = dir.join(format!("g{index}.json"));
        std::fs::write(&path, document.as_bytes())?;
        specs.push(GlowPassSpec {
            path,
            blur: group.blur,
            strength: group.strength,
        });
    }
    if let Err(err) = write_manifest(&dir, &specs) {
        tracing::warn!(%err, dir = %dir.display(), "cannot cache glow passes");
    }
    Ok(specs)
}

/// Generates the glow passes for `files` ahead of time, so the first time the
/// user plays an animation there is no generation pause.  Intended to run on a
/// background thread after startup.
pub fn warm_cache(cache_root: &Path, files: &[PathBuf]) {
    for file in files {
        if let Err(err) = passes(file, Some(cache_root)) {
            tracing::debug!(%err, file = %file.display(), "glow warm-up failed");
        }
    }
}

// ── compositing ────────────────────────────────────────────────────

#[inline]
fn sample(src: &[u8], width: usize, height: usize, x: f32, y: f32, out: &mut [f32; 4]) {
    let x = x.clamp(0.0, (width - 1) as f32);
    let y = y.clamp(0.0, (height - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let i00 = (y0 * width + x0) * 4;
    let i10 = (y0 * width + x1) * 4;
    let i01 = (y1 * width + x0) * 4;
    let i11 = (y1 * width + x1) * 4;
    for c in 0..4 {
        let top = src[i00 + c] as f32 * (1.0 - tx) + src[i10 + c] as f32 * tx;
        let bottom = src[i01 + c] as f32 * (1.0 - tx) + src[i11 + c] as f32 * tx;
        out[c] = top * (1.0 - ty) + bottom * ty;
    }
}

/// Composites a small, blurred glow buffer *under* the main frame.
///
/// Only the glow's non-empty bounding box is touched: the ThorVG effect surface
/// is canvas-sized, but the ink is usually a few percent of it.
pub(crate) fn composite_under(
    main: &mut [u8],
    width: usize,
    height: usize,
    glow: &[u8],
    glow_width: usize,
    glow_height: usize,
    scale: f32,
) {
    if glow_width == 0 || glow_height == 0 || width == 0 || height == 0 {
        return;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (glow_width, glow_height, 0usize, 0usize);
    for y in 0..glow_height {
        let row = y * glow_width * 4;
        let mut first = None;
        let mut last = 0usize;
        for x in 0..glow_width {
            if glow[row + x * 4 + 3] != 0 {
                if first.is_none() {
                    first = Some(x);
                }
                last = x;
            }
        }
        if let Some(start) = first {
            min_x = min_x.min(start);
            max_x = max_x.max(last);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    if max_x < min_x || max_y < min_y {
        return;
    }
    // `scale` maps main pixels to glow pixels (glow buffers are smaller when the
    // animation's canvas is big); the bbox has to be mapped the other way.
    let scale = scale.max(0.01);
    let ratio = 1.0 / scale;
    let x0 = ((min_x as f32 * ratio).floor().max(0.0)) as usize;
    let x1 = ((((max_x + 1) as f32 * ratio).ceil()) as usize).min(width);
    let y0 = ((min_y as f32 * ratio).floor().max(0.0)) as usize;
    let y1 = ((((max_y + 1) as f32 * ratio).ceil()) as usize).min(height);
    let mut sample_rgba = [0.0f32; 4];
    for y in y0..y1 {
        let fy = y as f32 * scale;
        for x in x0..x1 {
            let index = (y * width + x) * 4;
            sample(glow, glow_width, glow_height, x as f32 * scale, fy, &mut sample_rgba);
            let alpha = sample_rgba[3] / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let keep = 1.0 - alpha;
            for c in 0..3 {
                let under = main[index + c] as f32 / 255.0;
                let value = sample_rgba[c] / 255.0 + under * keep;
                main[index + c] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            let under = main[index + 3] as f32 / 255.0;
            main[index + 3] = ((under + alpha * (1.0 - under)) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()
            .expect("assets dir")
    }

    #[test]
    fn labelled_layers_produce_glow_passes() {
        let file = assets_root().join("nono-form4-walk/nono4_down.json");
        let specs = passes(&file, None).expect("glow passes");
        assert!(!specs.is_empty(), "form4 walk should have glow");
    }
}
