//! Persisted user configuration (`config.toml`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Active form (1, 2 or 3).
    pub form: u8,
    /// User-facing zoom.  Asset canvases are rendered 1:1 * scale * dpi.
    pub scale: f32,
    pub sound: bool,
    pub volume: f32,
    /// Allow NONO to wander around on its own.
    pub wander: bool,
    /// Keep the walk cycle playing while standing still (the original client
    /// does this).  Turn it off to save CPU when idle.
    pub idle_animation: bool,
    pub roam_min_secs: f32,
    pub roam_max_secs: f32,
    /// Screen-space (physical px) position of the pet's feet.  `None` means
    /// "pick a spot on the primary monitor at startup".
    pub anchor: Option<Point>,
    /// Recolour the white body parts, without touching the assets.  `None`
    /// keeps the original white.
    pub tint: Option<[u8; 3]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            form: 1,
            scale: 1.5,
            sound: true,
            volume: 0.7,
            wander: true,
            idle_animation: true,
            roam_min_secs: 6.0,
            roam_max_secs: 16.0,
            anchor: None,
            tint: None,
        }
    }
}

impl Config {
    /// Loads the config, falling back to defaults on any error.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(mut config) => {
                    config.sanitize();
                    config
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), %err, "config parse failed, using defaults");
                    Config::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "config read failed, using defaults");
                Config::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        std::fs::write(path, text)
    }

    pub fn sanitize(&mut self) {
        self.scale = self.scale.clamp(0.5, 3.0);
        self.volume = self.volume.clamp(0.0, 1.0);
        if !(1..=3).contains(&self.form) {
            self.form = 1;
        }
        self.roam_min_secs = self.roam_min_secs.clamp(2.0, 120.0);
        self.roam_max_secs = self.roam_max_secs.clamp(self.roam_min_secs, 600.0);
    }
}

/// `~/Library/Application Support/NonoPet/config.toml` on macOS.
///
/// `NONO_CONFIG` overrides the whole path (portable installs, tests, demos).
pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("NONO_CONFIG") {
        return PathBuf::from(path);
    }
    let base = if cfg!(target_os = "macos") {
        home_dir().map(|home| home.join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("NonoPet")
        .join("config.toml")
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Where the pet looks for `catalog.json`.
///
/// Order: `NONO_ASSETS`, `<exe dir>/assets`, current dir `assets`, then the
/// repository layout (`CARGO_MANIFEST_DIR/../../assets` during development).
pub fn default_asset_root() -> PathBuf {
    if let Some(path) = std::env::var_os("NONO_ASSETS") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("assets");
            if candidate.join("catalog.json").is_file() {
                return candidate;
            }
            // macOS bundle: Foo.app/Contents/MacOS/nono -> Contents/Resources/assets
            if let Some(contents) = dir.parent() {
                let candidate = contents.join("Resources/assets");
                if candidate.join("catalog.json").is_file() {
                    return candidate;
                }
            }
        }
    }
    let cwd_assets = PathBuf::from("assets");
    if cwd_assets.join("catalog.json").is_file() {
        return cwd_assets;
    }
    let repo_assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    if repo_assets.join("catalog.json").is_file() {
        return repo_assets;
    }
    cwd_assets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_toml() {
        let mut config = Config::default();
        config.form = 2;
        config.anchor = Some(Point::new(1200.0, 800.0));
        config.tint = Some([255, 150, 180]);
        let text = toml::to_string_pretty(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.form, 2);
        assert_eq!(back.scale, config.scale);
        assert_eq!(back.anchor, config.anchor);
        assert_eq!(back.tint, Some([255, 150, 180]));
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut config = Config { scale: 99.0, volume: -1.0, form: 9, ..Config::default() };
        config.sanitize();
        assert_eq!(config.scale, 3.0);
        assert_eq!(config.volume, 0.0);
        assert_eq!(config.form, 1);
    }
}
