# Builds the release tarball of the tag v<Version> into two packages,
# like the AUR recipes:
#   - bose-connect-gui: the GTK 4 + libadwaita window
#   - bose-connect-cli: the command line tool, without GTK
#
# Cargo downloads the crates in %%prep, so the build needs network
# access (on COPR: enable "internet access during builds").
#
# Test locally: `spectool -g -R bose-connect.spec` and
# `rpmbuild -ba bose-connect.spec`.

Name:           bose-connect
Version:        0.8.0
Release:        1%{?dist}
Summary:        Control Bose Bluetooth headphones and speakers
License:        AGPL-3.0-or-later
URL:            https://github.com/airvzxf/bose-connect-app-linux
Source0:        %{url}/archive/refs/tags/v%{version}/bose-connect-app-linux-%{version}.tar.gz

ExclusiveArch:  x86_64 aarch64

BuildRequires:  cargo >= 1.95
BuildRequires:  rust >= 1.95
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.22
BuildRequires:  pkgconfig(libadwaita-1) >= 1.9
BuildRequires:  pkgconfig(glib-2.0) >= 2.88
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

# The release profile has no debug info, so there is no debuginfo
# package; Cargo strips the binaries instead.
%global debug_package %{nil}

%description
Control Bose Bluetooth headphones and speakers on Linux: noise
control, volume, paired devices, party mode and more.

%package gui
Summary:        Control Bose Bluetooth headphones and speakers (GTK 4 window)
Requires:       bluez
Requires:       hicolor-icon-theme
Recommends:     pulseaudio-utils
Recommends:     wireplumber

%description gui
The window of Bose Connect for Linux, built with GTK 4. It does not
need the command line tool.

%package cli
Summary:        Control Bose Bluetooth headphones and speakers from the command line
Requires:       bluez

%description cli
The command line tool of Bose Connect for Linux,
bose-connect-app-linux. It does not need GTK.

%prep
%autosetup -n bose-connect-app-linux-%{version}
cargo fetch --locked

%build
export CARGO_TARGET_DIR=target
export CARGO_PROFILE_RELEASE_STRIP=symbols
cargo build --frozen --release -p bose-connect-gui -p bose-connect-cli

%install
linux=packaging/linux
install -Dm755 target/release/bose-connect-gui -t %{buildroot}%{_bindir}/
install -Dm755 target/release/bose-connect-app-linux -t %{buildroot}%{_bindir}/
install -Dm644 ${linux}/desktop/bose-connect-gui.desktop \
    %{buildroot}%{_datadir}/applications/com.airvzxf.bose-connect-gui.desktop
install -Dm644 ${linux}/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml \
    %{buildroot}%{_metainfodir}/com.airvzxf.bose-connect-gui.metainfo.xml
install -Dm644 ${linux}/icons/hicolor/scalable/apps/bose-connect-gui.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/com.airvzxf.bose-connect-gui.svg
install -Dm644 ${linux}/icons/hicolor/symbolic/apps/bose-connect-gui.svg \
    %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps/com.airvzxf.bose-connect-gui-symbolic.svg
install -Dm644 ${linux}/bash-completion/bose-connect-gui \
    %{buildroot}%{_datadir}/bash-completion/completions/bose-connect-gui

%check
export CARGO_TARGET_DIR=target
export CARGO_PROFILE_RELEASE_STRIP=symbols
cargo test --frozen --release -p bose-connect -p bose-connect-gui -p bose-connect-cli
desktop-file-validate %{buildroot}%{_datadir}/applications/com.airvzxf.bose-connect-gui.desktop
appstream-util validate-relax --nonet \
    %{buildroot}%{_metainfodir}/com.airvzxf.bose-connect-gui.metainfo.xml

%files gui
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/bose-connect-gui
%{_datadir}/applications/com.airvzxf.bose-connect-gui.desktop
%{_metainfodir}/com.airvzxf.bose-connect-gui.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/com.airvzxf.bose-connect-gui.svg
%{_datadir}/icons/hicolor/symbolic/apps/com.airvzxf.bose-connect-gui-symbolic.svg
%{_datadir}/bash-completion/completions/bose-connect-gui

%files cli
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/bose-connect-app-linux

%changelog
* Sat Nov 28 2026 Israel Roldan <israel.alberto.rv@gmail.com> - 0.8.0-1
- First RPM package: bose-connect-gui and bose-connect-cli.
