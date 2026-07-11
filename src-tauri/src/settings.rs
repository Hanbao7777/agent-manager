//! Read-only compatibility access to persisted path overrides.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    claude_config_dir: Option<String>,
    codex_config_dir: Option<String>,
    gemini_config_dir: Option<String>,
    opencode_config_dir: Option<String>,
    openclaw_config_dir: Option<String>,
    hermes_config_dir: Option<String>,
}

static SETTINGS: OnceLock<RwLock<PersistedSettings>> = OnceLock::new();

fn settings() -> &'static RwLock<PersistedSettings> {
    SETTINGS.get_or_init(|| RwLock::new(load_settings()))
}

fn load_settings() -> PersistedSettings {
    let Some(home) = dirs::home_dir() else {
        return PersistedSettings::default();
    };
    let path = home.join(".cc-switch").join("settings.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn resolve_override_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(relative) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(relative);
        }
    }
    PathBuf::from(raw)
}

fn get(path: &Option<String>) -> Option<PathBuf> {
    path.as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(resolve_override_path)
}

macro_rules! override_getter {
    ($name:ident, $field:ident) => {
        pub fn $name() -> Option<PathBuf> {
            settings()
                .read()
                .ok()
                .and_then(|settings| get(&settings.$field))
        }
    };
}

override_getter!(get_claude_override_dir, claude_config_dir);
override_getter!(get_codex_override_dir, codex_config_dir);
override_getter!(get_gemini_override_dir, gemini_config_dir);
override_getter!(get_opencode_override_dir, opencode_config_dir);
override_getter!(get_openclaw_override_dir, openclaw_config_dir);
override_getter!(get_hermes_override_dir, hermes_config_dir);
