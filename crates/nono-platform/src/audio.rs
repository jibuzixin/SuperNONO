//! Minimal sound playback for the pet's action sounds.
//!
//! macOS is implemented with `NSSound` (no audio decoder dependency).  Other
//! platforms get a no-op until their backend lands.

use std::path::Path;

#[cfg(target_os = "macos")]
pub struct AudioPlayer {
    enabled: bool,
    volume: f32,
    current: Option<objc2::rc::Retained<objc2_app_kit::NSSound>>,
}

#[cfg(target_os = "macos")]
impl AudioPlayer {
    pub fn new() -> Self {
        Self { enabled: true, volume: 0.7, current: None }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.stop();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sound) = &self.current {
            sound.setVolume(self.volume);
        }
    }

    /// Starts `path`.  `loop_until_stop` mirrors the asset manifests' "999999
    /// loops" notation: play until the next animation replaces this one.
    pub fn play(&mut self, path: &Path, loop_until_stop: bool) {
        if !self.enabled {
            return;
        }
        self.stop();
        let Some(path) = path.to_str() else {
            tracing::warn!("sound path is not valid UTF-8");
            return;
        };
        use objc2::AllocAnyThread;
        use objc2_foundation::NSString;

        let ns_path = NSString::from_str(path);
        let Some(sound) = objc2_app_kit::NSSound::initWithContentsOfFile_byReference(
            objc2_app_kit::NSSound::alloc(),
            &ns_path,
            true,
        ) else {
            tracing::warn!(path, "NSSound could not load the file");
            return;
        };
        sound.setVolume(self.volume);
        sound.setLoops(loop_until_stop);
        if !sound.play() {
            tracing::warn!(path, "NSSound refused to play");
            return;
        }
        self.current = Some(sound);
    }

    pub fn stop(&mut self) {
        if let Some(sound) = self.current.take() {
            sound.stop();
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default)]
pub struct AudioPlayer;

#[cfg(not(target_os = "macos"))]
impl AudioPlayer {
    pub fn new() -> Self {
        Self
    }
    pub fn set_enabled(&mut self, _enabled: bool) {}
    pub fn set_volume(&mut self, _volume: f32) {}
    pub fn play(&mut self, _path: &Path, _loop_until_stop: bool) {}
    pub fn stop(&mut self) {}
}
