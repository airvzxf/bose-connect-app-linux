# Linux packaging artefacts

Files KDE Plasma 6 (and any other freedesktop-compliant
desktop) need to find the application and render it in the
application launcher, the notifications flow, and the
`Discover` / GNOME Software store.

* `bose-connect-gui.desktop`, installed as
  `/usr/share/applications/com.airvzxf.bose-connect-gui.desktop`.
  The launcher entry: `Name`, `Exec`, `Icon`, `StartupWMClass`,
  `DBusActivatable`, and the `Actions=` list. KDE Plasma 6 reads the
  `Actions` to populate the right-click "Show / Refresh" menu in the
  launcher.
* `com.airvzxf.bose-connect-gui.metainfo.xml`, installed as
  `/usr/share/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml`.
  AppStream metadata. KDE Discover, GNOME Software, elementary
  AppCenter, and Plasma's app launcher "About" pane all read this for
  description, screenshots, version, and changelog.
* `bose-connect-gui.bash-completion`, installed as
  `/usr/share/bash-completion/completions/bose-connect-gui`.
  Bash tab-completion for the binary's flags. The AUR recipe and the
  Debian package install it; the binary itself ships without a
  builtin completion to keep `bose-connect-gui -h` discoverable.

## Verifying on the build host

```bash
# Validate the AppStream XML against the spec
appstreamcli validate \
    --no-net \
    packaging/linux/com.airvzxf.bose-connect-gui.metainfo.xml

# Validate the .desktop against the freedesktop spec
desktop-file-validate packaging/linux/bose-connect-gui.desktop
```

The `cargo` smoke-test (`--headless`) does not exercise these
files; they only matter at install time, when a packager runs
`meson` / `CMake` / `cargo-deb` and the resulting .deb / .rpm
drops the files under `/usr/share/`. A proper AUR recipe and a
proper `cargo-deb` / `cargo-rpm` config are tracked in
`AGENTS.md` under "Portability" and are out of scope for the
first ship.
