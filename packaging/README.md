# Packaging

The GUI needs GTK 4.22, libadwaita 1.9 and GLib 2.88 or newer. Arch
Linux (and CachyOS, Manjaro), Ubuntu 26.04 and Fedora 44 ship them;
Debian 13 does not.

The Debian package and the Arch package `bose-connect-gui` install:

* the window: `/usr/bin/bose-connect-gui`
* the command line tool: `/usr/bin/bose-connect-app-linux` (Debian
  only; on Arch it is the `bose-connect-cli` package)
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

## Arch Linux (`aur/`)

Two recipes, one per AUR package; both build the release tarball of
the tag `v<pkgver>`:

* `aur/bose-connect-gui/PKGBUILD`: the window. It does not use the
  command line tool and does not depend on it.
* `aur/bose-connect-cli/PKGBUILD`: the command line tool alone,
  `/usr/bin/bose-connect-app-linux`. It needs neither GTK nor
  libadwaita.

From the recipe's folder:

```bash
makepkg -si
```

The release PR sets `pkgver` and `sha256sums=('SKIP')`, because the
tarball exists only after the tag. Once the tag is pushed, run
`updpkgsums` and `makepkg --printsrcinfo > .SRCINFO` in each folder
and commit the result before the AUR upload.

## Debian and Ubuntu (`cargo deb`)

The configuration is the `[package.metadata.deb]` table of
`crates/bose-connect-gui/Cargo.toml`. Build it with:

```bash
packaging/debian/build-deb.sh
# → target/debian/bose-connect_<version>-1_amd64.deb
sudo apt install ./target/debian/bose-connect_*.deb
```

The script installs the pinned `cargo-deb` if needed, builds both
binaries and packs them. CI builds the package on every pull request
(job `deb`), and the release workflow attaches it to the GitHub
Release.

Build it on the oldest release it should install on (Ubuntu 26.04):
`$auto` takes the library versions from the build system.

## Fedora (`rpm/`)

One spec, `rpm/bose-connect.spec`, builds the release tarball of the
tag `v<Version>` into two packages, split like the AUR ones:

* `bose-connect-gui`: the window, with the files listed above.
* `bose-connect-cli`: the command line tool alone.

Fedora 44 is the first release with the GTK and libadwaita the window
needs. Cargo downloads the crates while building, so the build needs
network access. Build it in a Fedora 44 container:

```bash
podman run --rm -it -v "$PWD/packaging/rpm:/work:Z" \
    registry.fedoraproject.org/fedora:44 bash
dnf -y install rpm-build rpmdevtools dnf-plugins-core
cd /work && rpmdev-setuptree
spectool -g -R bose-connect.spec
dnf -y builddep bose-connect.spec
rpmbuild -ba bose-connect.spec
# → ~/rpmbuild/RPMS/x86_64/bose-connect-{gui,cli}-<version>-1.fc44.x86_64.rpm
```

On COPR, the project needs "Enable internet access during builds",
and a new build takes the raw URL of the spec on `main`, which COPR
downloads together with the tarball it names:

```bash
copr-cli build bose-connect \
    https://raw.githubusercontent.com/airvzxf/bose-connect-app-linux/main/packaging/rpm/bose-connect.spec
```

On each release, set `Version`, reset `Release` to 1 and add a
`%changelog` entry.

## Check the metadata

```bash
appstreamcli validate --no-net \
    packaging/linux/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml
desktop-file-validate packaging/linux/desktop/bose-connect-gui.desktop
```
