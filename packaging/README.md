# Packaging

Two distros are supported out of the box: Debian / Ubuntu (via
`cargo deb`) and Arch Linux (via the `PKGBUILD`). KDE Plasma 6
discovers the application through the `desktop` and
`metainfo` files in `linux/`.

## Debian / Ubuntu (`.deb`)

The configuration lives in `packaging/debian/cargo-deb.toml`.
To produce a working `.deb`:

```bash
# 1. Install cargo-deb if you don't have it already.
cargo install cargo-deb

# 2. Build and pack. The output lands in target/debian/.
cargo deb --workspace --no-build
# → target/debian/bose-connect-gui_0.2.0_amd64.deb
```

Install locally for testing:

```bash
sudo dpkg -i target/debian/bose-connect-gui_0.2.0_amd64.deb
bose-connect-gui   # should appear in the Plasma launcher
```

The `.deb` declares the runtime dependencies the binary needs
(`libgtk-4-1t64`, `libadwaita-1-0t64`, `libnotify4`,
`libglib2.0-0t64`) so a fresh Ubuntu or KDE Neon install will
pull them in via `apt`.

## Arch / Manjaro (`.pkg.tar.zst`)

The recipe lives in `packaging/aur/PKGBUILD`. To build:

```bash
# 1. From a clean checkout at the v0.2.0 tag:
git clone https://github.com/airvzxf/bose-connect-app-linux.git
cd bose-connect-app-linux
git checkout v0.2.0

# 2. Drop the PKGBUILD into the source tree at packaging/aur/
#    and the .SRCINFO next to it. Then from the source root:
cp packaging/aur/PKGBUILD .
cp packaging/aur/.SRCINFO .

# 3. Build + install. The .pkg.tar.zst lives in the same directory.
makepkg -si
```

To publish to AUR:

```bash
git clone ssh://aur@aur.archlinux.org/bose-connect-gui.git
cp packaging/aur/PKGBUILD .
cp packaging/aur/.SRCINFO .
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "upgpkg 0.2.0"
git push
```

## What the package installs

* the GTK4 + libadwaita GUI: `/usr/bin/bose-connect-gui`
* the original CLI driver: `/usr/bin/bose-connect-app-linux`
* the sanity-probe binary: `/usr/bin/bose-connect-probe`
* the C ABI library: `/usr/lib/libbose_connect.so`
* launcher entry:
  `/usr/share/applications/com.airvzxf.bose-connect-gui.desktop`
* AppStream metadata:
  `/usr/share/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml`
* the main app icon:
  `/usr/share/icons/hicolor/scalable/apps/com.airvzxf.bose-connect-gui.svg`
* the symbolic icon (KDE / GNOME tray):
  `/usr/share/icons/hicolor/symbolic/apps/com.airvzxf.bose-connect-gui-symbolic.svg`
* tab-completion:
  `/usr/share/bash-completion/completions/bose-connect-gui`

The screenshot artefacts in `screenshots/` and the metadata in
`linux/` are the same files that the packagers bundle.
