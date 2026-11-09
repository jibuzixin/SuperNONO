//! Runtime view of `assets/catalog.json`.
//!
//! The upstream asset pack stores one manifest per directory with slightly
//! different shapes; `tools/build_catalog.py` normalises them into a single
//! catalog.  This module is the only place that knows about that schema.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::geometry::{Direction, Point};

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("cannot read catalog {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse catalog {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("catalog is invalid: {0}")]
    Invalid(String),
    #[error("animation file missing: {0}")]
    MissingAsset(PathBuf),
    #[error("form {form} has no action/expression `{slug}`")]
    NotFound { form: u8, slug: String },
    #[error("form {form} does not exist")]
    NoForm { form: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationKind {
    Walk,
    Action,
    Expression,
}

/// Per-animation placement correction, in canvas units.
///
/// The upstream conversions are not perfectly consistent between the walk cycle
/// and some actions (see `assets/tuning.json`).  `scale` scales the picture
/// around the pet anchor and `offset` shifts it in canvas units; both are
/// applied at render time without touching the source Lottie files.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub scale: f32,
    /// Offset at the final frame (constant offset when `offset_start` is None).
    pub offset: (f32, f32),
    /// Optional offset at the first frame; interpolated to `offset` so both
    /// ends of the animation can line up (form-2 boot drifts internally).
    pub offset_start: Option<(f32, f32)>,
}

impl Default for Placement {
    fn default() -> Self {
        Self { scale: 1.0, offset: (0.0, 0.0), offset_start: None }
    }
}

#[derive(Debug, Clone)]
pub struct AudioSpec {
    /// Absolute path, resolved against the assets root.
    pub path: PathBuf,
    /// `None` means "loop until the next animation stops it".
    pub loops: Option<u32>,
    pub loop_until_stop: bool,
}

/// Everything the renderer and the window manager need for one animation.
#[derive(Debug, Clone)]
pub struct AnimationSpec {
    pub id: String,
    pub kind: AnimationKind,
    pub form: u8,
    pub file: PathBuf,
    pub canvas: (f32, f32),
    pub pet_anchor: (f32, f32),
    pub frames: u32,
    pub fps: f32,
    /// Walk loops forever; actions/expressions play once.
    pub loop_anim: bool,
    /// Actions only: whether the client returns to idle when the last frame
    /// is reached.  `false` for charge/shutdown — they hold the final frame.
    pub auto_end: bool,
    pub audio: Option<AudioSpec>,
    /// Facing to use when an action returns to idle (`None` keeps the current).
    pub return_facing: Option<Direction>,
    /// Action to enter, looping, when a non-auto-end action finishes
    /// (shutdown -> charge).  `None` holds the last frame.
    pub after_action: Option<String>,
    /// Rendering correction from `assets/tuning.json`.
    pub placement: Placement,
    /// Optional playback length override (used to trim duplicate/empty tails).
    pub play_frames: Option<u32>,
    /// Content bounding box in canvas units (left, top, right, bottom).
    pub content_bbox: Option<(f32, f32, f32, f32)>,
    /// Fade the runtime glow out between these two frames (e.g. summon tail).
    pub glow_fade: Option<[f32; 2]>,
    /// Human-readable label (Chinese name or direction).
    pub label: String,
}

impl AnimationSpec {
    /// Playback length, accounting for a tuning trim.
    pub fn effective_frames(&self) -> u32 {
        self.play_frames.unwrap_or(self.frames).max(1)
    }

    pub fn duration_secs(&self) -> f32 {
        if self.fps <= 0.0 {
            0.0
        } else {
            self.effective_frames() as f32 / self.fps
        }
    }

    pub fn canvas_point(&self) -> Point {
        Point::new(self.canvas.0, self.canvas.1)
    }
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    format_version: u32,
    #[serde(default)]
    asset_version: Option<String>,
    default_form: u8,
    forms: Vec<FormDef>,
}

#[derive(Debug, Deserialize)]
pub struct FormDef {
    pub id: u8,
    pub name: String,
    pub fps: f32,
    pub walk: WalkDef,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub expressions: Vec<ExpressionDef>,
}

#[derive(Debug, Deserialize)]
pub struct WalkDef {
    pub pet_anchor: [f32; 2],
    pub directions: std::collections::BTreeMap<String, DirDef>,
    pub idle_direction: String,
    #[serde(default)]
    pub idle_frame: u32,
}

#[derive(Debug, Deserialize)]
pub struct DirDef {
    pub file: String,
    pub canvas: [f32; 2],
    pub frames: u32,
    pub fps: f32,
    pub pet_anchor: [f32; 2],
    #[serde(default)]
    pub content_bbox: Option<[f32; 4]>,
    #[serde(default)]
    pub audio: Option<AudioDef>,
}

#[derive(Debug, Deserialize)]
pub struct ActionDef {
    pub id: u32,
    pub slug: String,
    pub name: String,
    pub file: String,
    pub canvas: [f32; 2],
    pub frames: u32,
    pub fps: f32,
    pub pet_anchor: [f32; 2],
    #[serde(default = "default_true")]
    pub auto_end: bool,
    /// Optional direction the pet should face when this action ends.
    #[serde(default)]
    pub return_facing: Option<String>,
    /// Action to enter (looping) when this action ends without auto-end.
    #[serde(default)]
    pub after_action: Option<String>,
    #[serde(default)]
    pub content_bbox: Option<[f32; 4]>,
    #[serde(default)]
    pub audio: Option<AudioDef>,
}

#[derive(Debug, Deserialize)]
pub struct ExpressionDef {
    pub id: u32,
    pub slug: String,
    pub name: String,
    pub file: String,
    pub canvas: [f32; 2],
    pub frames: u32,
    pub fps: f32,
    pub pet_anchor: [f32; 2],
    #[serde(default)]
    pub content_bbox: Option<[f32; 4]>,
    #[serde(default)]
    pub audio: Option<AudioDef>,
}

#[derive(Debug, Deserialize)]
pub struct AudioDef {
    pub file: String,
    #[serde(default)]
    pub loops: Option<u32>,
    #[serde(default)]
    pub loop_until_stop: bool,
}

fn default_true() -> bool {
    true
}

/// A public, read-only summary used to build menus without leaking internals.
#[derive(Debug, Clone)]
pub struct ActionInfo {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ExpressionInfo {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct TuningFile {
    format_version: u32,
    #[serde(default)]
    animations: BTreeMap<String, PlacementDef>,
}

#[derive(Debug, Deserialize, Default)]
struct PlacementDef {
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    glow_fade: Option<[f32; 2]>,
    #[serde(default)]
    offset: [f32; 2],
    #[serde(default)]
    offset_start: Option<[f32; 2]>,
    /// Trim the playback after this many frames (duplicate/empty tails in the
    /// source conversions).
    #[serde(default)]
    frames: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Tuning {
    placement: Placement,
    frames: Option<u32>,
    glow_fade: Option<[f32; 2]>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug)]
pub struct Catalog {
    root: PathBuf,
    raw: CatalogFile,
    tuning: BTreeMap<String, Tuning>,
}

impl Catalog {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let root = root.as_ref().to_path_buf();
        let path = root.join("catalog.json");
        let bytes = std::fs::read(&path).map_err(|source| CatalogError::Io {
            path: path.clone(),
            source,
        })?;
        let raw: CatalogFile =
            serde_json::from_slice(&bytes).map_err(|source| CatalogError::Parse {
                path: path.clone(),
                source,
            })?;
        let tuning = Self::load_tuning(&root)?;
        let catalog = Self { root, raw, tuning };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_form(&self) -> u8 {
        self.raw.default_form
    }

    /// Assets pack version (`assets/asset-version.json`), if the pack has one.
    pub fn asset_version(&self) -> Option<&str> {
        self.raw.asset_version.as_deref()
    }

    pub fn forms(&self) -> impl Iterator<Item = &FormDef> {
        self.raw.forms.iter()
    }

    pub fn form(&self, id: u8) -> Result<&FormDef, CatalogError> {
        self.raw
            .forms
            .iter()
            .find(|f| f.id == id)
            .ok_or(CatalogError::NoForm { form: id })
    }

    pub fn form_name(&self, id: u8) -> &str {
        self.raw
            .forms
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.name.as_str())
            .unwrap_or("形态")
    }

    pub fn action_infos(&self, form: u8) -> Vec<ActionInfo> {
        self.form(form)
            .map(|f| {
                f.actions
                    .iter()
                    .map(|a| ActionInfo {
                        slug: a.slug.clone(),
                        name: a.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn expression_infos(&self, form: u8) -> Vec<ExpressionInfo> {
        self.form(form)
            .map(|f| {
                f.expressions
                    .iter()
                    .map(|e| ExpressionInfo {
                        slug: e.slug.clone(),
                        name: e.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn has_action(&self, form: u8, slug: &str) -> bool {
        self.form(form)
            .map(|f| f.actions.iter().any(|a| a.slug == slug))
            .unwrap_or(false)
    }

    pub fn walk_spec(&self, form: u8, dir: Direction) -> Result<AnimationSpec, CatalogError> {
        let form_def = self.form(form)?;
        let key = dir.file_key();
        let def = form_def
            .walk
            .directions
            .get(key)
            .ok_or_else(|| CatalogError::NotFound {
                form,
                slug: key.to_string(),
            })?;
        let file = self.resolve(&def.file)?;
        let id = format!("form{form}/walk/{key}");
        Ok(AnimationSpec {
            id: id.clone(),
            kind: AnimationKind::Walk,
            form,
            file,
            canvas: (def.canvas[0], def.canvas[1]),
            pet_anchor: (def.pet_anchor[0], def.pet_anchor[1]),
            frames: def.frames,
            fps: def.fps,
            loop_anim: true,
            auto_end: false,
            audio: def.audio.as_ref().and_then(|a| self.audio(a)),
            return_facing: None,
            after_action: None,
            placement: self.tuning_for(&id).placement,
            play_frames: self.tuning_for(&id).frames,
            glow_fade: self.tuning_for(&id).glow_fade,
            content_bbox: def.content_bbox.map(|b| (b[0], b[1], b[2], b[3])),
            label: format!("走路·{key}"),
        })
    }

    pub fn action_spec(&self, form: u8, slug: &str) -> Result<AnimationSpec, CatalogError> {
        let form_def = self.form(form)?;
        let def = form_def
            .actions
            .iter()
            .find(|a| a.slug == slug)
            .ok_or_else(|| CatalogError::NotFound {
                form,
                slug: slug.to_string(),
            })?;
        let file = self.resolve(&def.file)?;
        let id = format!("form{form}/action/{slug}");
        let return_facing = match def.return_facing.as_deref() {
            Some(dir) => Some(Direction::from_key(dir).ok_or_else(|| {
                CatalogError::Invalid(format!("{id}: unknown return_facing {dir}"))
            })?),
            None => None,
        };
        Ok(AnimationSpec {
            id: id.clone(),
            kind: AnimationKind::Action,
            form,
            file,
            canvas: (def.canvas[0], def.canvas[1]),
            pet_anchor: (def.pet_anchor[0], def.pet_anchor[1]),
            frames: def.frames,
            fps: def.fps,
            loop_anim: false,
            auto_end: def.auto_end,
            audio: def.audio.as_ref().and_then(|a| self.audio(a)),
            return_facing,
            after_action: def.after_action.clone(),
            placement: self.tuning_for(&id).placement,
            play_frames: self.tuning_for(&id).frames,
            glow_fade: self.tuning_for(&id).glow_fade,
            content_bbox: def.content_bbox.map(|b| (b[0], b[1], b[2], b[3])),
            label: def.name.clone(),
        })
    }

    pub fn expression_spec(&self, form: u8, slug: &str) -> Result<AnimationSpec, CatalogError> {
        let form_def = self.form(form)?;
        let def = form_def
            .expressions
            .iter()
            .find(|e| e.slug == slug)
            .ok_or_else(|| CatalogError::NotFound {
                form,
                slug: slug.to_string(),
            })?;
        let file = self.resolve(&def.file)?;
        let id = format!("form{form}/expr/{slug}");
        Ok(AnimationSpec {
            id: id.clone(),
            kind: AnimationKind::Expression,
            form,
            file,
            canvas: (def.canvas[0], def.canvas[1]),
            pet_anchor: (def.pet_anchor[0], def.pet_anchor[1]),
            frames: def.frames,
            fps: def.fps,
            loop_anim: false,
            auto_end: true,
            audio: def.audio.as_ref().and_then(|a| self.audio(a)),
            return_facing: None,
            after_action: None,
            placement: self.tuning_for(&id).placement,
            play_frames: self.tuning_for(&id).frames,
            glow_fade: self.tuning_for(&id).glow_fade,
            content_bbox: def.content_bbox.map(|b| (b[0], b[1], b[2], b[3])),
            label: def.name.clone(),
        })
    }

    fn resolve(&self, relative: &str) -> Result<PathBuf, CatalogError> {
        let path = self.root.join(relative);
        if !path.is_file() {
            return Err(CatalogError::MissingAsset(path));
        }
        Ok(path)
    }

    fn audio(&self, def: &AudioDef) -> Option<AudioSpec> {
        let path = self.root.join(&def.file);
        if !path.is_file() {
            tracing::warn!(path = %path.display(), "audio file missing, ignoring");
            return None;
        }
        Some(AudioSpec {
            path,
            loops: def.loops,
            loop_until_stop: def.loop_until_stop,
        })
    }

    /// Optional per-animation corrections (`assets/tuning.json`).
    fn load_tuning(root: &Path) -> Result<BTreeMap<String, Tuning>, CatalogError> {
        let path = root.join("tuning.json");
        if !path.is_file() {
            return Ok(BTreeMap::new());
        }
        let bytes = std::fs::read(&path).map_err(|source| CatalogError::Io {
            path: path.clone(),
            source,
        })?;
        let file: TuningFile =
            serde_json::from_slice(&bytes).map_err(|source| CatalogError::Parse {
                path: path.clone(),
                source,
            })?;
        if file.format_version != 1 {
            return Err(CatalogError::Invalid(format!(
                "unsupported tuning format_version {}",
                file.format_version
            )));
        }
        let mut tuning = BTreeMap::new();
        for (id, def) in file.animations {
            if !def.scale.is_finite() || def.scale <= 0.0 {
                return Err(CatalogError::Invalid(format!("{id}: invalid scale")));
            }
            tuning.insert(
                id,
                Tuning {
                    placement: Placement {
                        scale: def.scale,
                        offset: (def.offset[0], def.offset[1]),
                        offset_start: def
                            .offset_start
                            .map(|o| (o[0], o[1])),
                    },
                    frames: def.frames,
                    glow_fade: def.glow_fade,
                },
            );
        }
        Ok(tuning)
    }

    fn tuning_for(&self, id: &str) -> Tuning {
        self.tuning.get(id).copied().unwrap_or_default()
    }

    fn validate(&self) -> Result<(), CatalogError> {
        if self.raw.format_version != 1 {
            return Err(CatalogError::Invalid(format!(
                "unsupported format_version {}",
                self.raw.format_version
            )));
        }
        if self.raw.forms.is_empty() {
            return Err(CatalogError::Invalid("no forms".into()));
        }
        if !self.raw.forms.iter().any(|f| f.id == self.raw.default_form) {
            return Err(CatalogError::Invalid(format!(
                "default_form {} is not present",
                self.raw.default_form
            )));
        }
        for form in &self.raw.forms {
            for dir in ["down", "left", "leftdown", "leftup", "up", "rightup", "right", "rightdown"]
            {
                let def = form
                    .walk
                    .directions
                    .get(dir)
                    .ok_or_else(|| CatalogError::Invalid(format!("form {} missing {dir}", form.id)))?;
                self.resolve(&def.file)?;
            }
            if !form.walk.directions.contains_key(&form.walk.idle_direction) {
                return Err(CatalogError::Invalid(format!(
                    "form {} idle_direction {} missing",
                    form.id, form.walk.idle_direction
                )));
            }
            for action in &form.actions {
                self.resolve(&action.file)?;
            }
            for expr in &form.expressions {
                self.resolve(&expr.file)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_catalog() -> Option<Catalog> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        if root.join("catalog.json").is_file() {
            Some(Catalog::load(root).expect("catalog loads"))
        } else {
            None
        }
    }

    #[test]
    fn real_catalog_loads_and_resolves() {
        let Some(catalog) = repo_catalog() else {
            return;
        };
        for form in catalog.forms() {
            for dir in Direction::ALL {
                let spec = catalog.walk_spec(form.id, dir).unwrap();
                assert!(spec.frames > 0);
                assert!(spec.fps > 0.0);
                assert!(spec.loop_anim);
                assert!(spec.file.is_file());
                assert_eq!(spec.pet_anchor, (form.walk.pet_anchor[0], form.walk.pet_anchor[1]));
            }
            for action in &form.actions {
                let spec = catalog.action_spec(form.id, &action.slug).unwrap();
                assert!(!spec.loop_anim);
                assert_eq!(spec.auto_end, action.auto_end);
            }
            for expr in &form.expressions {
                let spec = catalog.expression_spec(form.id, &expr.slug).unwrap();
                assert_eq!(spec.kind, AnimationKind::Expression);
            }
        }
    }

    #[test]
    fn return_facing_and_tuning_are_loaded() {
        let Some(catalog) = repo_catalog() else {
            return;
        };
        assert_eq!(
            catalog.action_spec(1, "battery").unwrap().return_facing,
            Some(Direction::Left)
        );
        assert_eq!(
            catalog.action_spec(2, "battery").unwrap().return_facing,
            Some(Direction::Right)
        );
        assert_eq!(
            catalog.action_spec(1, "summon").unwrap().return_facing,
            Some(Direction::Down)
        );
        // assets/tuning.json compensates the form-2 boot/cube conversion.
        let boot = catalog.action_spec(2, "boot").unwrap().placement;
        assert!(boot.offset.1 > 0.0);
        assert!(boot.offset_start.is_some(), "form-2 boot needs a start-correction ramp");
        assert!(catalog.action_spec(2, "cube").unwrap().placement.scale < 0.9);
        assert_eq!(
            catalog.walk_spec(2, Direction::Down).unwrap().placement,
            Placement::default()
        );
        // Form-1 shutdown has a duplicated second cycle in the source; it is
        // trimmed to the first one (2.08 s, matching its sound).
        assert_eq!(
            catalog.action_spec(1, "shutdown").unwrap().effective_frames(),
            95
        );
        // Form-3 summon has an empty tail; play only the visible part.
        assert_eq!(
            catalog.action_spec(3, "summon").unwrap().effective_frames(),
            70
        );
    }

    #[test]
    fn form3_lacks_charge_action() {
        let Some(catalog) = repo_catalog() else {
            return;
        };
        assert!(catalog.has_action(1, "charge"));
        assert!(!catalog.has_action(3, "charge"));
    }
}
