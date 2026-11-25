//! Removal of the icon-theme index that versions up to 0.6.0
//! wrote into the user's data directory.
//!
//! Those versions installed a `.desktop` file and the icons under
//! `$XDG_DATA_HOME` at every start, and created
//! `$XDG_DATA_HOME/icons/hicolor/index.theme` when it was missing.
//! Desktops read only the first `hicolor/index.theme` they find, so
//! that file hid the system index: icons of other applications in
//! directories it does not list (`48x48/apps`, …) stopped resolving.
//!
//! The GUI no longer writes anything there; installing the
//! `.desktop` file and the icons is the package's job. At start it
//! removes the index only when it is byte-for-byte the one those
//! versions wrote, together with the `icon-theme.cache` that was
//! built from it. The `.desktop` file and the icons they installed
//! are harmless and stay, so a launcher entry keeps working.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The exact `hicolor/index.theme` that versions up to 0.6.0 wrote.
const LEGACY_INDEX: &[u8] = b"[Icon Theme]\n\
Name=Hicolor\n\
Comment=Fallback Icon Theme\n\
Directories=scalable/apps,symbolic/apps\n\
\n\
[scalable/apps]\n\
Size=64\n\
Type=Scalable\n\
MinSize=8\n\
MaxSize=512\n\
Context=Applications\n\
\n\
[symbolic/apps]\n\
Size=16\n\
Type=Scalable\n\
MinSize=8\n\
MaxSize=512\n\
Context=Applications\n";

/// `$XDG_DATA_HOME` for the current user, or `$HOME/.local/share`
/// as the freedesktop fallback.
pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}

/// Remove the legacy icon-theme index (and its cache) from the
/// user's data directory. Returns `true` when it was removed.
pub fn remove_legacy_icon_index() -> Result<bool> {
    remove_legacy_icon_index_in(&data_home())
}

fn remove_legacy_icon_index_in(home: &Path) -> Result<bool> {
    let dir = home.join("icons/hicolor");
    let index = dir.join("index.theme");
    match fs::read(&index) {
        Ok(bytes) if bytes == LEGACY_INDEX => {}
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("read {}", index.display())),
    }
    fs::remove_file(&index).with_context(|| format!("remove {}", index.display()))?;
    // Built by `gtk-update-icon-cache` from that index, so it only
    // knows the two directories the index listed.
    let cache = dir.join("icon-theme.cache");
    match fs::remove_file(&cache) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("remove {}", cache.display())),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hicolor(home: &Path) -> PathBuf {
        let dir = home.join("icons/hicolor");
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn removes_the_legacy_index_and_its_cache() {
        let home = tempfile::tempdir().unwrap();
        let dir = hicolor(home.path());
        fs::write(dir.join("index.theme"), LEGACY_INDEX).unwrap();
        fs::write(dir.join("icon-theme.cache"), b"cache").unwrap();
        fs::create_dir_all(dir.join("48x48/apps")).unwrap();
        fs::write(dir.join("48x48/apps/other.png"), b"png").unwrap();

        assert!(remove_legacy_icon_index_in(home.path()).unwrap());
        assert!(!dir.join("index.theme").exists());
        assert!(!dir.join("icon-theme.cache").exists());
        assert!(dir.join("48x48/apps/other.png").exists());
        // A second start finds nothing to do.
        assert!(!remove_legacy_icon_index_in(home.path()).unwrap());
    }

    #[test]
    fn keeps_an_index_that_is_not_ours() {
        let home = tempfile::tempdir().unwrap();
        let dir = hicolor(home.path());
        let mut edited = LEGACY_INDEX.to_vec();
        edited.extend_from_slice(b"\n[48x48/apps]\nSize=48\n");
        fs::write(dir.join("index.theme"), &edited).unwrap();
        fs::write(dir.join("icon-theme.cache"), b"cache").unwrap();

        assert!(!remove_legacy_icon_index_in(home.path()).unwrap());
        assert_eq!(fs::read(dir.join("index.theme")).unwrap(), edited);
        assert!(dir.join("icon-theme.cache").exists());
    }

    #[test]
    fn nothing_to_do_without_an_index() {
        let home = tempfile::tempdir().unwrap();
        assert!(!remove_legacy_icon_index_in(home.path()).unwrap());
    }

    #[test]
    fn legacy_index_is_the_258_bytes_that_0_6_0_wrote() {
        assert_eq!(LEGACY_INDEX.len(), 258);
        assert!(LEGACY_INDEX.starts_with(b"[Icon Theme]\nName=Hicolor\n"));
    }
}
