//! Persisted user configuration (`config.toml`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::geometry::Point;
use crate::speed;
use crate::trail;

/// Writes the opacity as a clean two-decimal number (`0.22`, not
/// `0.2199999988079071`) so the tuning file stays hand-editable.
fn serialize_opacity<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64((*value as f64 * 100.0).round() / 100.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Active form (1–5).
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
    /// Global movement speed in logical points per second.
    pub move_speed: f32,
    /// Form 1's walk animation playback speed (1.0 = asset fps, 0.25–4.0).
    /// This scales the walking/idle cycle only, not the movement speed.
    #[serde(default = "default_walk_anim_speed")]
    pub form1_walk_anim_speed: f32,
    /// Follow-mouse speed override.  `None` uses [`Config::move_speed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_speed: Option<f32>,
    /// Idle/free-roam speed override.  `None` uses [`Config::move_speed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_speed: Option<f32>,
    /// Fading afterimages while walking.
    pub trail_enabled: bool,
    /// How often a new afterimage is spawned (controls density).
    pub trail_interval_ms: u32,
    /// How long each afterimage fades in/out (controls the trail length).
    pub trail_lifetime_ms: u32,
    /// Peak opacity of an afterimage (controls how strong the trail is).
    #[serde(serialize_with = "serialize_opacity")]
    pub trail_opacity: f32,
    /// Screen-space (logical global points) position of the pet's feet.
    ///
    /// Written on drag/summon/quit and used at startup when
    /// [`Config::start_at_center`] is `false`.
    pub anchor: Option<Point>,
    /// Displays the pet may roam on (`CGDirectDisplayID` values on macOS).
    /// `None`/empty means "primary display only", which is also the default for
    /// configs written by older versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_ids: Option<Vec<u32>>,
    /// Start at the primary display's centre (the default).  When `false` the
    /// last saved [`Config::anchor`] is restored instead.
    #[serde(default = "default_true")]
    pub start_at_center: bool,
    /// Play the form's summon action when the pet is summoned / started.
    #[serde(default = "default_true")]
    pub summon_animation: bool,
    /// Recolour the white body parts, without touching the assets.  `None`
    /// keeps the original white.
    pub tint: Option<[u8; 3]>,
    /// Runtime glow rebuilt from the assets' own filter parameters.
    pub glow: bool,
    /// Glow blur radius = source blur × this factor.
    pub glow_sigma: f32,
    /// Glow brightness multiplier (1.0 = the source strength).
    pub glow_alpha: f32,
}

fn default_walk_anim_speed() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
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
            move_speed: speed::DEFAULT_GLOBAL,
            form1_walk_anim_speed: default_walk_anim_speed(),
            follow_speed: speed::DEFAULT_FOLLOW,
            idle_speed: speed::DEFAULT_IDLE,
            trail_enabled: trail::DEFAULT_ENABLED,
            trail_interval_ms: trail::DEFAULT_INTERVAL_MS,
            trail_lifetime_ms: trail::DEFAULT_LIFETIME_MS,
            trail_opacity: trail::DEFAULT_OPACITY,
            anchor: None,
            display_ids: None,
            start_at_center: true,
            summon_animation: true,
            tint: None,
            glow: true,
            glow_sigma: 0.6,
            glow_alpha: 1.0,
        }
    }
}

impl Config {
    /// Loads the config, falling back to defaults on any error.
    pub fn load(path: &Path) -> Self {
        match Self::load_strict(path) {
            Ok(config) => config,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "config load failed, using defaults");
                Config::default()
            }
        }
    }

    /// Like [`Config::load`], but reports read/parse errors to the caller.  The
    /// debug hot-reload path uses this so a typo while editing the file does not
    /// silently reset the running settings.
    pub fn load_strict(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
    }

    /// Parses a config document.
    ///
    /// Configs written before speed settings existed have no `move_speed` /
    /// `follow_speed` / `idle_speed` keys.  For those only, keep the original
    /// faster follow speed instead of letting a missing `Option` mean "use the
    /// global speed".  Once any speed key is present, omitting `follow_speed`
    /// or `idle_speed` means "follow the global speed" as documented.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        let document: toml::Value = toml::from_str(text)?;
        let has_speed = ["move_speed", "follow_speed", "idle_speed"]
            .iter()
            .any(|key| document.get(key).is_some());
        let mut config: Config = toml::from_str(text)?;
        if !has_speed {
            config.follow_speed = speed::DEFAULT_FOLLOW;
        }
        config.sanitize();
        Ok(config)
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
        if !(1..=5).contains(&self.form) {
            self.form = 1;
        }
        self.roam_min_secs = self.roam_min_secs.clamp(2.0, 120.0);
        self.roam_max_secs = self.roam_max_secs.clamp(self.roam_min_secs, 600.0);
        self.move_speed = speed::sanitize(self.move_speed);
        self.form1_walk_anim_speed = self.form1_walk_anim_speed.clamp(0.25, 4.0);
        self.follow_speed = speed::sanitize_override(self.follow_speed);
        self.idle_speed = speed::sanitize_override(self.idle_speed);
        let (interval, lifetime) = trail::sanitize(self.trail_interval_ms, self.trail_lifetime_ms);
        self.trail_interval_ms = interval;
        self.trail_lifetime_ms = lifetime;
        self.trail_opacity = (trail::sanitize_opacity(self.trail_opacity) * 100.0).round() / 100.0;
        self.glow_sigma = self.glow_sigma.clamp(0.1, 2.0);
        self.glow_alpha = self.glow_alpha.clamp(0.0, 2.0);
        if let Some(ids) = &mut self.display_ids {
            ids.sort_unstable();
            ids.dedup();
            if ids.is_empty() {
                self.display_ids = None;
            }
        }
    }

    /// Follow speed in pt/s, falling back to the global speed.
    pub fn effective_follow_speed(&self) -> f32 {
        self.follow_speed.unwrap_or(self.move_speed)
    }

    /// Idle/free-roam speed in pt/s, falling back to the global speed.
    pub fn effective_idle_speed(&self) -> f32 {
        self.idle_speed.unwrap_or(self.move_speed)
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

/// Cache directory for generated tinted Lottie copies.
///
/// `NONO_CACHE` overrides it.  These files are large but regenerable, so they
/// live in the OS cache location rather than next to the config.
pub fn cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("NONO_CACHE") {
        return PathBuf::from(path);
    }
    let base = if cfg!(target_os = "macos") {
        home_dir().map(|home| home.join("Library/Caches"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".cache")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("NonoPet")
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
        config.trail_interval_ms = 120;
        config.trail_lifetime_ms = 640;
        config.trail_opacity = 0.35;
        config.move_speed = 90.0;
        config.follow_speed = None;
        config.idle_speed = Some(120.0);
        config.display_ids = Some(vec![3, 1, 3]);
        config.start_at_center = false;
        config.summon_animation = false;
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(text.contains("trail_opacity = 0.35"), "{text}");
        assert!(text.contains("idle_speed = 120.0"), "{text}");
        assert!(
            !text.contains("follow_speed"),
            "None override must be omitted: {text}"
        );
        assert!(text.contains("start_at_center = false"), "{text}");
        assert!(text.contains("summon_animation = false"), "{text}");
        let back = Config::from_toml(&text).unwrap();
        assert_eq!(back.form, 2);
        assert_eq!(back.scale, config.scale);
        assert_eq!(back.anchor, config.anchor);
        assert_eq!(back.tint, Some([255, 150, 180]));
        assert_eq!(back.trail_interval_ms, 120);
        assert_eq!(back.trail_lifetime_ms, 640);
        assert!((back.trail_opacity - 0.35).abs() < 1e-6);
        assert!((back.move_speed - 90.0).abs() < 1e-6);
        assert_eq!(back.follow_speed, None);
        assert_eq!(back.idle_speed, Some(120.0));
        assert_eq!(
            back.display_ids,
            Some(vec![1, 3]),
            "ids are deduplicated/sorted"
        );
        assert!(!back.summon_animation);
        assert!(!back.start_at_center);
        assert!((back.effective_follow_speed() - 90.0).abs() < 1e-6);
        assert!((back.effective_idle_speed() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn speed_overrides_fall_back_to_the_global_speed() {
        let mut config = Config::default();
        assert!((config.effective_idle_speed() - 60.0).abs() < 1e-6);
        assert!((config.effective_follow_speed() - 140.0).abs() < 1e-6);
        config.move_speed = 100.0;
        config.follow_speed = None;
        config.idle_speed = None;
        assert!((config.effective_idle_speed() - 100.0).abs() < 1e-6);
        assert!((config.effective_follow_speed() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn speedless_legacy_config_keeps_the_original_follow_speed() {
        let config = Config::from_toml("form = 1\nscale = 1.5\n").unwrap();
        assert!((config.move_speed - 60.0).abs() < 1e-6);
        assert_eq!(config.follow_speed, Some(140.0));
        assert_eq!(config.idle_speed, None);
        assert!((config.effective_follow_speed() - 140.0).abs() < 1e-6);
    }

    #[test]
    fn once_a_speed_key_exists_omitted_overrides_mean_global() {
        let config = Config::from_toml("form = 1\nmove_speed = 90.0\n").unwrap();
        assert_eq!(config.follow_speed, None);
        assert_eq!(config.idle_speed, None);
        assert!((config.effective_follow_speed() - 90.0).abs() < 1e-6);
        assert!((config.effective_idle_speed() - 90.0).abs() < 1e-6);

        let config = Config::from_toml("form = 1\nidle_speed = 120.0\n").unwrap();
        assert_eq!(config.follow_speed, None);
        assert_eq!(config.idle_speed, Some(120.0));
    }

    #[test]
    fn old_configs_get_the_default_trail_settings() {
        let back: Config = toml::from_str("form = 1\nscale = 1.5\n").unwrap();
        assert!(back.trail_enabled);
        assert_eq!(back.trail_interval_ms, trail::DEFAULT_INTERVAL_MS);
        assert_eq!(back.trail_lifetime_ms, trail::DEFAULT_LIFETIME_MS);
        assert!((back.trail_opacity - trail::DEFAULT_OPACITY).abs() < 1e-6);
        // Multi-display and summon settings are new: old files mean "primary
        // display only" and "play the summon animation".
        assert_eq!(back.display_ids, None);
        assert!(back.start_at_center);
        assert!(back.summon_animation);
    }

    #[test]
    fn legacy_trail_preset_field_is_ignored_without_losing_the_config() {
        // The pre-release build wrote `trail = "normal"`; the new timing fields
        // must still load and everything else must be preserved.
        let back: Config =
            toml::from_str("form = 3\ntrail = \"normal\"\ntrail_interval_ms = 40\n").unwrap();
        assert_eq!(back.form, 3);
        assert_eq!(back.trail_interval_ms, 40);
        assert_eq!(back.trail_lifetime_ms, trail::DEFAULT_LIFETIME_MS);
        assert!((back.trail_opacity - trail::DEFAULT_OPACITY).abs() < 1e-6);
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut config = Config {
            scale: 99.0,
            volume: -1.0,
            form: 9,
            ..Config::default()
        };
        config.sanitize();
        assert_eq!(config.form, 1);
        let mut walk = Config {
            form1_walk_anim_speed: 99.0,
            ..Config::default()
        };
        walk.sanitize();
        assert_eq!(walk.form1_walk_anim_speed, 4.0);
        assert_eq!(config.scale, 3.0);
        assert_eq!(config.volume, 0.0);
        assert_eq!(config.form, 1);
    }

    #[test]
    fn sanitize_repairs_trail_timing() {
        let mut config = Config {
            trail_interval_ms: 10,
            trail_lifetime_ms: 10,
            trail_opacity: 9.0,
            ..Config::default()
        };
        config.sanitize();
        assert_eq!(config.trail_interval_ms, trail::MIN_INTERVAL_MS);
        assert_eq!(config.trail_lifetime_ms, trail::MIN_LIFETIME_MS);
        assert!((config.trail_opacity - trail::MAX_OPACITY).abs() < 1e-6);
    }

    #[test]
    fn sanitize_repairs_movement_speeds() {
        let mut config = Config {
            move_speed: 0.0,
            follow_speed: Some(9999.0),
            idle_speed: Some(-1.0),
            ..Config::default()
        };
        config.sanitize();
        assert!((config.move_speed - speed::MIN_SPEED).abs() < 1e-6);
        assert_eq!(config.follow_speed, Some(speed::MAX_SPEED));
        assert_eq!(config.idle_speed, Some(speed::MIN_SPEED));
    }
}
