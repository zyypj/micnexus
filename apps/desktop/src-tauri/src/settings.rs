//! Settings persisted as JSON in the app config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::devices::DefaultMics;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trusted {
    pub id: String,
    pub name: String,
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Shown to phones; empty = the computer name.
    pub pc_name: String,
    /// Render endpoint that gets the phone's audio; None = best virtual cable.
    pub output_id: Option<String>,
    /// Make the cable the Windows default microphone while a phone is connected.
    pub set_default_mic: bool,
    /// Also play the phone on the speakers ("listen").
    pub monitor: bool,
    /// None = Windows default output.
    pub monitor_id: Option<String>,
    /// Look for phones on the USB cable (runs the adb server).
    pub usb_enabled: bool,
    /// Open MicNexus on the phone when it is plugged in (USB debugging on).
    pub usb_auto_launch: bool,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub start_with_windows: bool,
    pub mute_hotkey: Option<String>,
    pub notify_on_connect: bool,
    pub trusted: Vec<Trusted>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pc_name: String::new(),
            output_id: None,
            set_default_mic: true,
            monitor: false,
            monitor_id: None,
            usb_enabled: true,
            usb_auto_launch: true,
            wifi_enabled: true,
            bluetooth_enabled: true,
            start_with_windows: true,
            mute_hotkey: Some("Ctrl+Shift+M".into()),
            notify_on_connect: true,
            trusted: Vec::new(),
        }
    }
}

/// Internal state that survives restarts but is not a user setting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Persisted {
    /// Stable id announced to phones.
    pub pc_id: String,
    /// Default microphones before we switched to the cable; restored on
    /// disconnect (and at startup, after a crash).
    pub restore_mics: Option<DefaultMics>,
    /// The phone that connected last (USB auto-launch skips others).
    pub last_device: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub settings: Settings,
    pub persisted: Persisted,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        let _ = std::fs::create_dir_all(dir);
        Self {
            path: dir.join("settings.json"),
        }
    }

    pub fn load(&self) -> Stored {
        let mut stored: Stored = std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if stored.persisted.pc_id.is_empty() {
            stored.persisted.pc_id = uuid::Uuid::new_v4().to_string();
            self.save(&stored);
        }
        stored
    }

    /// Atomic write (temp file + rename).
    pub fn save(&self, stored: &Stored) {
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(stored)
            && std::fs::write(&tmp, bytes).is_ok()
        {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_get_defaults_for_new_fields() {
        let s: Settings = serde_json::from_str(r#"{"monitor":true}"#).unwrap();
        assert!(s.monitor);
        assert!(s.set_default_mic);
        assert_eq!(s.mute_hotkey.as_deref(), Some("Ctrl+Shift+M"));
    }

    #[test]
    fn store_creates_a_pc_id_once() {
        let dir = std::env::temp_dir().join(format!("micnexus-test-{}", uuid::Uuid::new_v4()));
        let store = Store::new(&dir);
        let a = store.load().persisted.pc_id;
        let b = store.load().persisted.pc_id;
        assert!(!a.is_empty());
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(dir);
    }
}
