//! Extension contracts for the NONO desktop pet.
//!
//! M1 ships the *contract* only: the manifest schema, the permission model and
//! the host trait.  The sandboxed WASM host (wasmi + per-plugin permission
//! enforcement + hot reload) lands in M3; nothing here requires wasmtime.
//!
//! A plugin is a directory:
//!
//! ```text
//! my_plugin/
//! ├─ plugin.toml    # id/name/version/author/permissions/entry
//! ├─ main.wasm      # wasm32-wasip1 module exporting init/on_event
//! └─ assets/        # optional plugin assets
//! ```

use serde::{Deserialize, Serialize};

/// Bump when the host ABI changes.  Plugins declare the version they target and
/// the host refuses anything newer than itself.
pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin.toml is invalid: {0}")]
    Manifest(#[from] toml::de::Error),
    #[error("plugin targets API v{plugin}, host supports v{host}")]
    ApiVersion { plugin: u32, host: u32 },
    #[error("plugin `{plugin}` requested permission `{permission}` that it did not declare")]
    PermissionDenied { plugin: String, permission: String },
    #[error("plugin entry missing: {0}")]
    MissingEntry(String),
}

/// Everything a plugin may ask the host to do.  Undeclared permissions are
/// rejected by the host; the plugin can never touch the OS directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Publish `PlayAnimation` / `ChangeState` events.
    SendAnimationEvent,
    /// Read the text the user typed (or pasted) for the pet.
    ReadUserText,
    /// Read/write files inside the plugin's own asset directory.
    AccessFiles,
    /// Make outbound HTTP requests (weather, etc.).
    HttpOutbound,
    ClipboardRead,
    ClipboardWrite,
    ShowNotification,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::SendAnimationEvent => "send_animation_event",
            Permission::ReadUserText => "read_user_text",
            Permission::AccessFiles => "access_files",
            Permission::HttpOutbound => "http_outbound",
            Permission::ClipboardRead => "clipboard_read",
            Permission::ClipboardWrite => "clipboard_write",
            Permission::ShowNotification => "show_notification",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// Host ABI the plugin was built against.
    pub api_version: u32,
    /// Path (relative to the plugin directory) of the wasm module.
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

fn default_entry() -> String {
    "main.wasm".to_string()
}

impl PluginManifest {
    pub fn parse(text: &str) -> Result<Self, PluginError> {
        let manifest: Self = toml::from_str(text)?;
        if manifest.api_version > PLUGIN_API_VERSION {
            return Err(PluginError::ApiVersion {
                plugin: manifest.api_version,
                host: PLUGIN_API_VERSION,
            });
        }
        Ok(manifest)
    }

    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Host-side guard used before every privileged host call.
    pub fn require(&self, permission: Permission) -> Result<(), PluginError> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied {
                plugin: self.id.clone(),
                permission: permission.as_str().to_string(),
            })
        }
    }
}

/// The host side of the extension contract.  A future `WasmiHost` implements
/// this; the application only ever talks to extensions through it.
pub trait ExtensionHost {
    /// Scans the plugin directory and loads every valid plugin.
    fn load_all(&mut self) -> Result<usize, PluginError>;
    fn load(&mut self, plugin_dir: &std::path::Path) -> Result<(), PluginError>;
    fn unload(&mut self, plugin_id: &str) -> Result<(), PluginError>;
    /// Delivers an event to every interested plugin and returns the events the
    /// plugins want to publish (the host validates permissions first).
    fn dispatch(&mut self, event: &serde_json::Value) -> Result<Vec<serde_json::Value>, PluginError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
id = "clock.chime"
name = "整点报时"
version = "0.1.0"
author = "community"
api_version = 1
permissions = ["send_animation_event", "show_notification"]
"#;

    #[test]
    fn manifest_parses_and_checks_permissions() {
        let manifest = PluginManifest::parse(MANIFEST).unwrap();
        assert_eq!(manifest.entry, "main.wasm");
        assert!(manifest.has(Permission::SendAnimationEvent));
        assert!(!manifest.has(Permission::HttpOutbound));
        assert!(manifest.require(Permission::ShowNotification).is_ok());
        let denied = manifest.require(Permission::ClipboardRead).unwrap_err();
        assert!(matches!(denied, PluginError::PermissionDenied { .. }));
    }

    #[test]
    fn newer_api_version_is_rejected() {
        let text = MANIFEST.replace("api_version = 1", "api_version = 99");
        let err = PluginManifest::parse(&text).unwrap_err();
        assert!(matches!(err, PluginError::ApiVersion { .. }));
    }
}
