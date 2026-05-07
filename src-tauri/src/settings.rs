use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub local_urls_enabled: bool,
    pub local_url_tld: String,
    pub proxy_port: u16,
    #[serde(default)]
    pub editor: String,
    #[serde(default)]
    pub first_run_completed: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Optional override for the projects root directory. Empty string means
    /// "use the platform default" (`~/SailProjects` on macOS).
    #[serde(default)]
    pub projects_root: String,
    /// Serve `.<tld>` URLs over HTTPS in addition to HTTP. Requires a one-time
    /// keychain trust prompt the first time it's enabled (Sail Manager
    /// generates a local CA and installs it into the user's login keychain).
    #[serde(default)]
    pub local_urls_https: bool,
}

fn default_theme() -> String {
    "system".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            local_urls_enabled: false,
            local_url_tld: "sail".to_string(),
            proxy_port: 80,
            editor: String::new(),
            first_run_completed: false,
            theme: default_theme(),
            projects_root: String::new(),
            local_urls_https: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        let inner = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            Settings::default()
        };
        Ok(Self {
            path,
            inner: Mutex::new(inner),
        })
    }

    pub fn snapshot(&self) -> Settings {
        self.inner.lock().expect("poisoned").clone()
    }

    pub fn replace(&self, new: Settings) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&new)?;
        std::fs::write(&self.path, json)?;
        *self.inner.lock().expect("poisoned") = new;
        Ok(())
    }

    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> AppResult<Settings> {
        let mut current = self.snapshot();
        f(&mut current);
        self.replace(current.clone())?;
        Ok(current)
    }
}

pub fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_design() {
        let d = Settings::default();
        assert!(!d.local_urls_enabled);
        assert_eq!(d.local_url_tld, "sail");
        assert_eq!(d.proxy_port, 80);
        assert_eq!(d.editor, "");
        assert!(!d.first_run_completed);
        assert_eq!(d.theme, "system");
    }

    #[test]
    fn deserializes_minimal_v0_payload_with_field_defaults() {
        // Old settings files predate `editor`, `firstRunCompleted`, and
        // `theme`. Make sure they still load via #[serde(default)].
        let json = r#"{
            "localUrlsEnabled": false,
            "localUrlTld": "test",
            "proxyPort": 80
        }"#;
        let s: Settings = serde_json::from_str(json).expect("deserializes");
        assert_eq!(s.local_url_tld, "test");
        assert_eq!(s.editor, "");
        assert!(!s.first_run_completed);
        assert_eq!(s.theme, "system"); // default_theme()
    }

    #[test]
    fn deserializes_with_only_theme_missing_uses_system_default() {
        let json = r#"{
            "localUrlsEnabled": true,
            "localUrlTld": "sail",
            "proxyPort": 80,
            "editor": "phpstorm",
            "firstRunCompleted": true
        }"#;
        let s: Settings = serde_json::from_str(json).expect("deserializes");
        assert_eq!(s.theme, "system");
        assert!(s.local_urls_enabled);
        assert!(s.first_run_completed);
        assert_eq!(s.editor, "phpstorm");
    }

    #[test]
    fn deserializes_full_payload() {
        let json = r#"{
            "localUrlsEnabled": true,
            "localUrlTld": "test",
            "proxyPort": 8080,
            "editor": "cursor",
            "firstRunCompleted": true,
            "theme": "dark"
        }"#;
        let s: Settings = serde_json::from_str(json).expect("deserializes");
        assert!(s.local_urls_enabled);
        assert_eq!(s.local_url_tld, "test");
        assert_eq!(s.proxy_port, 8080);
        assert_eq!(s.editor, "cursor");
        assert!(s.first_run_completed);
        assert_eq!(s.theme, "dark");
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            local_urls_enabled: true,
            local_url_tld: "foo".to_string(),
            proxy_port: 8080,
            editor: "zed".to_string(),
            first_run_completed: true,
            theme: "light".to_string(),
            projects_root: String::new(),
            local_urls_https: false,
        };
        let json = serde_json::to_string(&s).expect("serializes");
        // camelCase across the wire, per the Rust/TS contract.
        assert!(json.contains("\"localUrlsEnabled\":true"));
        assert!(json.contains("\"firstRunCompleted\":true"));
        let back: Settings = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back.local_url_tld, "foo");
        assert_eq!(back.theme, "light");
    }

    #[test]
    fn settings_path_appends_filename() {
        let dir = std::path::Path::new("/tmp/sail-tests");
        let p = settings_path(dir);
        assert_eq!(p, dir.join("settings.json"));
    }
}
