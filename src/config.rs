use crate::models::PersistedState;
use std::path::PathBuf;

/// Return the base configuration directory for the application, adhering to
/// OS conventions (Windows AppData, Linux XDG, macOS Application Support).
pub fn state_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("eParna");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }

    if let Ok(home) = std::env::var("HOME") {
        #[cfg(target_os = "macos")]
        {
            let dir = PathBuf::from(&home)
                .join("Library")
                .join("Application Support")
                .join("eParna");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let config_base = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(&home).join(".config"));
            let dir = config_base.join("eParna");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }
    }

    PathBuf::from(".")
}

/// Return the directory used to cache book cover thumbnails.
pub fn covers_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("eParna").join("covers");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }

    if let Ok(home) = std::env::var("HOME") {
        #[cfg(target_os = "macos")]
        {
            let dir = PathBuf::from(&home)
                .join("Library")
                .join("Caches")
                .join("eParna")
                .join("covers");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let cache_base = std::env::var("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(&home).join(".cache"));
            let dir = cache_base.join("eParna").join("covers");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }
    }

    let dir = PathBuf::from("eParna_covers");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_path() -> PathBuf {
    state_dir().join("state.json")
}

/// Location of the state file used before the app was renamed from ePustak,
/// so existing libraries/settings can be migrated on first launch.
pub fn legacy_config_path() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("ePustak").join("state.json");
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home)
            .join(".config")
            .join("ePustak")
            .join("state.json");
        if p.exists() {
            return Some(p);
        }
    }
    let local = PathBuf::from("ePustak_state.json");
    if local.exists() {
        return Some(local);
    }
    None
}

pub fn load_state() -> PersistedState {
    let p = config_path();
    let raw = std::fs::read_to_string(&p)
        .ok()
        .or_else(|| legacy_config_path().and_then(|lp| std::fs::read_to_string(lp).ok()));
    match raw {
        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
        None => PersistedState::default(),
    }
}

pub fn save_state(state: &PersistedState) {
    let p = config_path();
    if let Ok(s) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&p, s);
    }
}
