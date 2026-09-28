//! First-run desktop / icon installation.
//!
//! On the host KDE Plasma 6 (and every other freedesktop
//! notification daemon) cannot route a notification into the
//! bell until it has an entry at
//! `~/.local/share/applications/<desktop-entry>.desktop`,
//! and it cannot resolve an icon name until the matching
//! `.svg` lives under `~/.local/share/icons/hicolor/`.
//!
//! Without those two files present at runtime, even a perfect
//! `Notify` call with all four standard hints (DesktopEntry,
//! ImagePath, Category, Urgency) is treated as an unknown-app
//! transient overlay.
//!
//! This module installs both on first launch, idempotently.
//! It is a *best-effort* helper: when the host filesystem is
//! read-only (snap, AppImage, restricted containers) every
//! `install` call returns a `Result::Err` that we log at
//! `warn` level — never panic, never block startup.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Basename of the `.desktop` file. KDE Plasma 6 keys the
/// `bell` and the notification daemon key the persistence
/// layer (one config per app) on this string.
pub const DESKTOP_BASENAME: &str = "bose-connect-gui";

/// Theme-relative icon name. Same as `DESKTOP_BASENAME` per
/// the convention used by `update-desktop-database`.
pub const ICON_NAME: &str = "bose-connect-gui";

#[derive(Debug, Clone)]
struct Asset {
    relative_target: &'static str,
    bytes: &'static [u8],
}

const DESKTOP_FILE: Asset = Asset {
    relative_target: "applications/bose-connect-gui.desktop",
    bytes: include_bytes!("../../../../packaging/linux/bose-connect-gui.desktop"),
};

const SCALABLE_SVG: Asset = Asset {
    relative_target: "icons/hicolor/scalable/apps/bose-connect-gui.svg",
    bytes: include_bytes!("../../resources/icons/hicolor/scalable/apps/bose-connect-gui.svg"),
};

const SYMBOLIC_SVG: Asset = Asset {
    relative_target: "icons/hicolor/symbolic/apps/bose-connect-gui.svg",
    bytes: include_bytes!("../../resources/icons/hicolor/symbolic/apps/bose-connect-gui.svg"),
};

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

/// `bool := true`  -> the file at `target` had the same bytes,
/// so we can short-circuit the `gtk-update-icon-cache` /
/// `update-desktop-database` calls.
fn install_once(home: &Path, asset: &Asset) -> Result<bool> {
    let target = home.join(asset.relative_target);
    if let Ok(existing) = fs::read(&target) {
        if existing == asset.bytes {
            return Ok(true);
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let mut f =
        fs::File::create(&target).with_context(|| format!("create {}", target.display()))?;
    f.write_all(asset.bytes)
        .with_context(|| format!("write {}", target.display()))?;
    Ok(false)
}

/// Ensure a minimal `hicolor/index.theme` exists under
/// `~/.local/share/icons/`. Without it,
/// `gtk-update-icon-cache` cannot generate a cache file, which
/// KDE Plasma 6's icontheme loader depends on.
fn ensure_hicolor_index(home: &Path) -> Result<()> {
    let index = home.join("icons/hicolor/index.theme");
    if index.exists() {
        return Ok(());
    }
    fs::create_dir_all(index.parent().unwrap())?;
    fs::write(
        &index,
        b"[Icon Theme]\n\
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
          Context=Applications\n",
    )?;
    Ok(())
}

/// Run `gtk-update-icon-cache` and `update-desktop-database`
/// when they're on `$PATH`. Both exits are logged but never
/// fail the install — Plasma 6 lazily rebuilds the cache the
/// first time it loads the theme.
fn refresh_caches(home: &Path) {
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .arg("-f")
        .arg(home.join("icons/hicolor"))
        .output();
    let _ = std::process::Command::new("update-desktop-database")
        .arg(home.join("applications"))
        .output();
}

/// Install everything the binary needs to integrate with KDE.
/// Returns `true` if at least one asset was freshly written —
/// callers may use this to nudge the cache builders.
pub fn install_all() -> Result<bool> {
    let home = data_home();
    ensure_hicolor_index(&home)?;
    let d = install_once(&home, &DESKTOP_FILE)?;
    let s = install_once(&home, &SCALABLE_SVG)?;
    let sym = install_once(&home, &SYMBOLIC_SVG)?;
    let changed = d || s || sym;
    if changed {
        refresh_caches(&home);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_all_is_idempotent() {
        // We can't write to the real data_home in CI, so
        // exercise the helpers against a tempdir.
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();

        fs::create_dir_all(home.join("applications")).unwrap();
        fs::create_dir_all(home.join("icons/hicolor/scalable/apps")).unwrap();
        fs::create_dir_all(home.join("icons/hicolor/symbolic/apps")).unwrap();

        // First write — all paths return Ok(false).
        let d = install_once(
            home,
            &Asset {
                relative_target: "applications/bose-connect-gui.desktop",
                bytes: b"hello",
            },
        )
        .unwrap();
        assert!(!d, "first call: file did not exist before");

        // Second write — same bytes, Ok(true).
        let d = install_once(
            home,
            &Asset {
                relative_target: "applications/bose-connect-gui.desktop",
                bytes: b"hello",
            },
        )
        .unwrap();
        assert!(d, "second call: file matched, no rewrite");

        // Third write — different bytes, Ok(false).
        let d = install_once(
            home,
            &Asset {
                relative_target: "applications/bose-connect-gui.desktop",
                bytes: b"world",
            },
        )
        .unwrap();
        assert!(!d, "third call: bytes differed, must rewrite");
    }
}
