//! Sub-layer "part" analysis for tint annotation.
//!
//! A layer can contain one fill that covers several paths (the shell ring plus
//! an eye highlight, for example).  Selecting the whole layer would tint both.
//! This module splits a fill group into *parts*: a path at even nesting depth
//! plus the paths directly inside it (holes) form one part; a path inside a
//! hole is an island and a separate part.  Paths are addressed by their ordinal
//! among the layer's shape paths, so the annotator and renderer agree.

use serde_json::{json, Value};

/// One annotatable part.
pub struct Unit {
    pub layer: usize,
    /// Ordinals of the shape paths inside the layer (depth-first order).
    pub paths: Vec<usize>,
    pub color: [u8; 3],
    pub name: String,
}

fn sh_points(sh: &Value) -> Option<Vec<(f64, f64)>> {
    let k = sh.get("ks")?.get("k")?;
    let v = k.get("v")?.as_array()?;
    let mut out = Vec::with_capacity(v.len());
    for p in v {
        let a = p.as_array()?;
        out.push((a.first()?.as_f64()?, a.get(1)?.as_f64()?));
    }
    if out.len() < 3 { None } else { Some(out) }
}

fn poly_area(p: &[(f64, f64)]) -> f64 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let (x1, y1) = p[i];
        let (x2, y2) = p[(i + 1) % p.len()];
        s += x1 * y2 - x2 * y1;
    }
    (s / 2.0).abs()
}

fn inside(pt: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let (x, y) = pt;
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            c = !c;
        }
        j = i;
    }
    c
}

/// Splits a fill group's shape paths into parts (ordinals within the group).
fn group_units(sh_items: &[Value]) -> Vec<Vec<usize>> {
    let n = sh_items.len();
    if n == 0 {
        return Vec::new();
    }
    let polys: Vec<Option<Vec<(f64, f64)>>> = sh_items.iter().map(sh_points).collect();
    let areas: Vec<f64> = polys
        .iter()
        .map(|p| p.as_ref().map(|p| poly_area(p)).unwrap_or(0.0))
        .collect();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let Some(pi) = &polys[i] else { continue };
        if pi.is_empty() {
            continue;
        }
        let pt = pi[0];
        let mut best: Option<usize> = None;
        let mut best_area = f64::MAX;
        for j in 0..n {
            if i == j || areas[j] <= areas[i] {
                continue;
            }
            let Some(pj) = &polys[j] else { continue };
            if inside(pt, pj) && areas[j] < best_area {
                best = Some(j);
                best_area = areas[j];
            }
        }
        // A tiny detail (a highlight dot or hairline) is not a hole even if it
        // lies inside another path; making it a hole would force it into the
        // shell's part and tint it together with the body.
        if let Some(b) = best {
            if areas[i] < 0.02 * areas[b] {
                best = None;
            }
        }
        parent[i] = best;
    }
    let mut depth = vec![0usize; n];
    for i in 0..n {
        let mut d = 0;
        let mut cur = parent[i];
        let mut guard = 0;
        while let Some(p) = cur {
            d += 1;
            cur = parent[p];
            guard += 1;
            if guard > n {
                break;
            }
        }
        depth[i] = d;
    }
    let mut units = Vec::new();
    for i in 0..n {
        if depth[i] % 2 != 0 {
            continue;
        }
        let mut unit = vec![i];
        for j in 0..n {
            if parent[j] == Some(i) {
                unit.push(j);
            }
        }
        unit.sort_unstable();
        units.push(unit);
    }
    units
}

fn split_items(it: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let sh: Vec<Value> = it
        .iter()
        .filter(|v| v.get("ty").and_then(|t| t.as_str()) == Some("sh"))
        .cloned()
        .collect();
    let other: Vec<Value> = it
        .iter()
        .filter(|v| v.get("ty").and_then(|t| t.as_str()) != Some("sh"))
        .cloned()
        .collect();
    (sh, other)
}

fn make_group(group: &Value, sh: &[Value], other: &[Value], paths: &[usize], color: Option<[u8; 3]>, tint: Option<[u8; 3]>) -> Value {
    let mut items: Vec<Value> = paths.iter().map(|i| sh[*i].clone()).collect();
    for item in other {
        let mut item = item.clone();
        if let Some(c) = color {
            if matches!(item.get("ty").and_then(|t| t.as_str()), Some("fl") | Some("st")) {
                item["c"] = json!({ "a": 0, "k": [c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0, 1.0] });
            }
        }
        if let Some(t) = tint {
            tint_fills(&mut item, t);
        }
        items.push(item);
    }
    json!({ "ty": "gr", "nm": group.get("nm").cloned().unwrap_or(Value::Null), "it": items })
}

fn tint_fills(item: &mut Value, tint: [u8; 3]) {
    if matches!(item.get("ty").and_then(|t| t.as_str()), Some("fl") | Some("st")) {
        if let Some(c) = item.get_mut("c").and_then(|v| v.as_object_mut()) {
            if c.get("a").and_then(|v| v.as_i64()).unwrap_or(0) == 0 {
                if let Some(k) = c.get_mut("k").and_then(|v| v.as_array_mut()) {
                    if k.len() >= 3 {
                        let r = k[0].as_f64().unwrap_or(0.0) as f32;
                        let g = k[1].as_f64().unwrap_or(0.0) as f32;
                        let b = k[2].as_f64().unwrap_or(0.0) as f32;
                        let mx = r.max(g).max(b);
                        let mn = r.min(g).min(b);
                        if mx >= 0.5 && mx - mn <= 0.09 && b <= r + 0.015 {
                            k[0] = json!(tint[0] as f32 / 255.0 * mx);
                            k[1] = json!(tint[1] as f32 / 255.0 * mx);
                            k[2] = json!(tint[2] as f32 / 255.0 * mx);
                        }
                    }
                }
            }
        }
    }
}

/// Recolours the selected path parts of `root`.  `parts` is `(layer, paths)`
/// where paths are the per-layer ordinals from the annotator.
pub fn apply_parts(root: &mut Value, parts: &[(usize, Vec<usize>)], tint: [u8; 3]) {
    let Some(layers) = root.get_mut("layers").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for (layer_idx, layer) in layers.iter_mut().enumerate() {
        let Some(shapes) = layer.get_mut("shapes").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        let mut counter = 0usize;
        let mut new_shapes = Vec::new();
        for mut item in shapes.drain(..) {
            if item.get("ty").and_then(|t| t.as_str()) != Some("gr") {
                new_shapes.push(item);
                continue;
            }
            let Some(it) = item.get("it").and_then(|v| v.as_array()) else {
                new_shapes.push(item);
                continue;
            };
            let (sh, other) = split_items(it);
            let n = sh.len();
            let selected_global: Vec<usize> = parts
                .iter()
                .filter(|(l, _)| *l == layer_idx)
                .flat_map(|(_, p)| p.iter().copied())
                .collect();
            if n == 0 || selected_global.is_empty() {
                counter += n;
                new_shapes.push(item);
                continue;
            }
            let sel: Vec<usize> = (0..n)
                .filter(|o| selected_global.contains(&(counter + o)))
                .collect();
            counter += n;
            if sel.is_empty() {
                new_shapes.push(item);
                continue;
            }
            let units = group_units(&sh);
            // Preserve compound parts: a selected path brings its holes.
            let mut selected_paths: Vec<usize> = sel.clone();
            for unit in &units {
                if unit.iter().any(|o| sel.contains(o)) {
                    for o in unit {
                        if !selected_paths.contains(o) {
                            selected_paths.push(*o);
                        }
                    }
                }
            }
            let rest: Vec<usize> = (0..n).filter(|o| !selected_paths.contains(o)).collect();
            new_shapes.push(make_group(&item, &sh, &other, &selected_paths, None, Some(tint)));
            if !rest.is_empty() {
                new_shapes.push(make_group(&item, &sh, &other, &rest, None, None));
            }
        }
        *shapes = new_shapes;
    }
}

/// Splits every fill group into parts and colours each part uniquely.
pub fn color_map(root: &mut Value) -> Vec<Unit> {
    let mut units = Vec::new();
    let Some(layers) = root.get_mut("layers").and_then(|v| v.as_array_mut()) else {
        return units;
    };
    let mut color_id = 0usize;
    for (layer_idx, layer) in layers.iter_mut().enumerate() {
        let Some(shapes) = layer.get_mut("shapes").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        let mut counter = 0usize;
        let mut new_shapes = Vec::new();
        for mut item in shapes.drain(..) {
            if item.get("ty").and_then(|t| t.as_str()) != Some("gr") {
                new_shapes.push(item);
                continue;
            }
            let Some(it) = item.get("it").and_then(|v| v.as_array()) else {
                new_shapes.push(item);
                continue;
            };
            let (sh, other) = split_items(it);
            let n = sh.len();
            if n == 0 {
                new_shapes.push(item);
                continue;
            }
            let start = counter;
            counter += n;
            let units_local = group_units(&sh);
            let mut group_colors = Vec::new();
            for unit in &units_local {
                let c = [(color_id & 0xFF) as u8, ((color_id >> 8) & 0xFF) as u8, 255u8];
                color_id += 1;
                group_colors.push(c);
                units.push(Unit {
                    layer: layer_idx,
                    paths: unit.iter().map(|o| start + *o).collect(),
                    color: c,
                    name: item.get("nm").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
            for (unit, c) in units_local.iter().zip(group_colors) {
                let g = make_group(&item, &sh, &other, unit, Some(c), None);
                new_shapes.push(g);
            }
        }
        *shapes = new_shapes;
    }
    units
}
