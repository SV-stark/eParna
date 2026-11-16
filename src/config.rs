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

pub fn load_state_from(path: &std::path::Path, legacy: Option<&std::path::Path>) -> PersistedState {
    if let Ok(raw) = std::fs::read_to_string(path) {
        match serde_json::from_str(&raw) {
            Ok(state) => return state,
            Err(err) => {
                eprintln!("Failed to parse state file {}: {err}", path.display());
                let backup = path.with_extension("json.corrupt");
                if !backup.exists() {
                    let _ = std::fs::copy(path, &backup);
                }
                return PersistedState::default();
            }
        }
    }

    if let Some(lp) = legacy
        && let Ok(raw) = std::fs::read_to_string(lp)
        && let Ok(state) = serde_json::from_str(&raw)
    {
        return state;
    }

    PersistedState::default()
}

pub fn load_state() -> PersistedState {
    let p = config_path();
    let legacy = legacy_config_path();
    load_state_from(&p, legacy.as_deref())
}

pub fn save_state_to(path: &std::path::Path, state: &PersistedState) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let s = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, s)?;
    // On Windows std::fs::rename replaces an existing target on modern NTFS,
    // but remove first if rename fails due to target-exists on older filesystems.
    if let Err(err) = std::fs::rename(&tmp, path) {
        if path.exists() {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)?;
        } else {
            return Err(err);
        }
    }
    Ok(())
}

pub fn save_state(state: &PersistedState) {
    let p = config_path();
    if let Err(err) = save_state_to(&p, state) {
        eprintln!("Failed to save state to {}: {err}", p.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RecentBook;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("eparna_test_{name}_{nanos}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_save_and_load_roundtrip_atomic() {
        let dir = temp_test_dir("roundtrip");
        let path = dir.join("state.json");
        let mut state = PersistedState::default();
        state.recent.push(RecentBook {
            path: "/books/demo.epub".into(),
            title: "Demo Book".into(),
            ..Default::default()
        });

        save_state_to(&path, &state).expect("save_state_to should succeed");
        assert!(!path.with_extension("json.tmp").exists());

        let loaded = load_state_from(&path, None);
        assert_eq!(loaded.recent.len(), 1);
        assert_eq!(loaded.recent[0].title, "Demo Book");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_migrates_from_legacy_path() {
        let dir = temp_test_dir("legacy");
        let path = dir.join("new_state.json");
        let legacy = dir.join("legacy_state.json");

        let state = PersistedState {
            theme: "Sepia".into(),
            ..Default::default()
        };
        save_state_to(&legacy, &state).expect("legacy save should succeed");

        let loaded = load_state_from(&path, Some(&legacy));
        assert_eq!(loaded.theme, "Sepia");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_corrupt_state_is_backed_up() {
        let dir = temp_test_dir("corrupt");
        let path = dir.join("state.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let loaded = load_state_from(&path, None);
        assert!(loaded.recent.is_empty());
        assert!(path.with_extension("json.corrupt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
