# Packaging

The GUI needs GTK 4.22, libadwaita 1.9 and GLib 2.88 or newer. Arch
Linux (and CachyOS, Manjaro), Ubuntu 26.04 and Fedora 44 ship them;
Debian 13 does not.

Both packages install:

* the window: `/usr/bin/bose-connect-gui`
* the command line tool: `/usr/bin/bose-connect-app-linux`
* launcher entry:
  `/usr/share/applications/com.airvzxf.bose-connect-gui.desktop`
* AppStream metadata:
  `/usr/share/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml`
* the icon:
  `/usr/share/icons/hicolor/scalable/apps/com.airvzxf.bose-connect-gui.svg`
* the symbolic icon:
  `/usr/share/icons/hicolor/symbolic/apps/com.airvzxf.bose-connect-gui-symbolic.svg`
* tab completion for the window:
  `/usr/share/bash-completion/completions/bose-connect-gui`

The C library (`libbose_connect.so`, `bose_connect.h`) stays a
release asset for developers; the packages do not install it.

The desktop file, the metainfo and the icons live in `linux/`. The
names matter: the window's application id and its notifications use
`com.airvzxf.bose-connect-gui`, and KDE Plasma finds the
notification's icon and name only through a desktop file of that
name.

## Arch Linux (`aur/PKGBUILD`)

The recipe builds the release tarball of the tag `v<pkgver>`. From
`packaging/aur/`:

```bash
makepkg -si
```

After a version change, update `pkgver`, `sha256sums` (`makepkg -g`)
and `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`).

## Debian and Ubuntu (`cargo deb`)

The configuration is the `[package.metadata.deb]` table of
`crates/bose-connect-gui/Cargo.toml`. Build both binaries, then pack:

```bash
cargo install cargo-deb
cargo build --release --locked -p bose-connect-gui -p bose-connect-cli
cargo deb -p bose-connect-gui --no-build
# → target/debian/bose-connect_<version>-1_amd64.deb
sudo apt install ./target/debian/bose-connect_*.deb
```

Build it on the oldest release it should install on (Ubuntu 26.04):
`$auto` takes the library versions from the build system.

## Check the metadata

```bash
appstreamcli validate --no-net \
    packaging/linux/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml
desktop-file-validate packaging/linux/desktop/bose-connect-gui.desktop
```
