//! Runtime view of `assets/catalog.json`.
//!
//! The upstream asset pack stores one manifest per directory with slightly
//! different shapes; `tools/build_catalog.py` normalises them into a single
//! catalog.  This module is the only place that knows about that schema.

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
    /// First frame to show (0 for everything except the idle pose).
    pub start_frame: u32,
    /// Human-readable label (Chinese name or direction).
    pub label: String,
}

impl AnimationSpec {
    pub fn duration_secs(&self) -> f32 {
        if self.fps <= 0.0 {
            0.0
        } else {
            self.frames as f32 / self.fps
        }
    }

    pub fn canvas_point(&self) -> Point {
        Point::new(self.canvas.0, self.canvas.1)
    }
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    format_version: u32,
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

#[derive(Debug)]
pub struct Catalog {
    root: PathBuf,
    raw: CatalogFile,
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
        let catalog = Self { root, raw };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_form(&self) -> u8 {
        self.raw.default_form
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

    /// The idle pose: a held frame of the walk animation for `dir`.
    pub fn idle_spec(&self, form: u8, dir: Direction) -> Result<AnimationSpec, CatalogError> {
        let mut spec = self.walk_spec(form, dir)?;
        spec.id = format!("form{form}/idle/{}", dir.file_key());
        spec.kind = AnimationKind::Walk;
        spec.label = format!("待机·{}", dir.file_key());
        spec.start_frame = self.form(form).map(|f| f.walk.idle_frame).unwrap_or(0);
        Ok(spec)
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
        Ok(AnimationSpec {
            id: format!("form{form}/walk/{key}"),
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
            start_frame: 0,
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
        Ok(AnimationSpec {
            id: format!("form{form}/action/{slug}"),
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
            start_frame: 0,
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
        Ok(AnimationSpec {
            id: format!("form{form}/expr/{slug}"),
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
            start_frame: 0,
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
    fn form3_lacks_charge_action() {
        let Some(catalog) = repo_catalog() else {
            return;
        };
        assert!(catalog.has_action(1, "charge"));
        assert!(!catalog.has_action(3, "charge"));
    }
}
