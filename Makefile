# Top-level Makefile for the `bose-connect-app-linux` workspace.
#
# Goals:
#   * GNU autotools conventions: PREFIX, DESTDIR, BINDIR, DATADIR.
#   * Idempotent installs: install / uninstall / reinstall all work.
#   * Sane defaults: `make` builds release; `sudo make install` ships to /usr/local.
#   * No hidden side effects: every target declares what it touches.
#
# Run `make help` for a per-target summary; `make` with no args is
# `make build` because that's what 99 % of first-time contributors want.

# --- Location & target detection ------------------------------------------
SHELL              := /usr/bin/env bash
CARGO              ?= cargo
ARCH               := $(shell uname -m)

BUILD              ?= release
PROFILE_DIR        := $(if $(filter release,$(BUILD)),release,debug)
TARGET_DIR         := target/$(PROFILE_DIR)
BINARY_NAME        := bose-connect-gui
BINARY_PATH        := $(TARGET_DIR)/$(BINARY_NAME)

CRATE_TOP          := crates/bose-connect-gui
RESOURCE_DIR       := $(CRATE_TOP)/resources
PACKAGING_LINUX    := packaging/linux
DESKTOP_FILE       := $(PACKAGING_LINUX)/desktop/$(BINARY_NAME).desktop
METAINFO_FILE      := $(PACKAGING_LINUX)/metainfo/com.airvzxf.bose-connect-gui.metainfo.xml
BASH_COMPLETION    := $(PACKAGING_LINUX)/bash-completion/$(BINARY_NAME)
ICON_SCALABLE      := $(PACKAGING_LINUX)/icons/hicolor/scalable/apps/$(BINARY_NAME).svg
ICON_SYMBOLIC      := $(PACKAGING_LINUX)/icons/hicolor/symbolic/apps/$(BINARY_NAME).svg
# The crate's resources/icons/ directory is a symlink farm
# pointing back into $(PACKAGING_LINUX)/icons/. We rebuild
# the links just in case a fresh checkout is missing them.
RESOURCE_SYMLINKS := \
    $(RESOURCE_DIR)/icons/hicolor/scalable/apps/$(BINARY_NAME).svg \
    $(RESOURCE_DIR)/icons/hicolor/symbolic/apps/$(BINARY_NAME).svg

# --- Install layout (FHS-ish) ---------------------------------------------
# Override PREFIX or DESTDIR as needed, e.g.
#   sudo make install PREFIX=/usr
#   make install DESTDIR=./staging
PREFIX             ?= /usr/local
EXEC_PREFIX        ?= $(PREFIX)
BINDIR             ?= $(EXEC_PREFIX)/bin
DATAROOTDIR        ?= $(PREFIX)/share
DATADIR            ?= $(DATAROOTDIR)
SYSCONFDIR         ?= $(PREFIX)/etc
LOCALSTATEDIR      ?= $(PREFIX)/var
MANDIR             ?= $(DATAROOTDIR)/man
INFODIR            ?= $(DATAROOTDIR)/info

ICON_DIR           := $(DATADIR)/icons/hicolor
APPS_DIR           := $(DATADIR)/applications
METAINFO_DIR       := $(DATADIR)/metainfo
BASH_COMP_DIR      := $(DATAROOTDIR)/bash-completion/completions

# Compose every absolute final path under DESTDIR so a staging tree
# (`make install DESTDIR=./staging`) stays consistent across the
# install / uninstall pair.
DEST_BINDIR        := $(DESTDIR)$(BINDIR)
DEST_APPS_DIR      := $(DESTDIR)$(APPS_DIR)
DEST_METAINFO_DIR  := $(DESTDIR)$(METAINFO_DIR)
DEST_ICON_SCAL_DIR := $(DESTDIR)$(ICON_DIR)/scalable/apps
DEST_ICON_SYM_DIR  := $(DESTDIR)$(ICON_DIR)/symbolic/apps
DEST_BASH_COMP_DIR := $(DESTDIR)$(BASH_COMP_DIR)
DEST_MAN_DIR       := $(DESTDIR)$(MANDIR)/man1

# Files we will (re)move on uninstall. Listed here as the single
# source of truth so the install / uninstall pair stay in lockstep.
INSTALLED_FILES := \
    $(DEST_BINDIR)/$(BINARY_NAME) \
    $(DEST_APPS_DIR)/$(BINARY_NAME).desktop \
    $(DEST_METAINFO_DIR)/com.airvzxf.bose-connect-gui.metainfo.xml \
    $(DEST_ICON_SCAL_DIR)/$(BINARY_NAME).svg \
    $(DEST_ICON_SYM_DIR)/$(BINARY_NAME).svg \
    $(DEST_BASH_COMP_DIR)/$(BINARY_NAME) \

# --- Tooling discovery -----------------------------------------------------
# Each helper is checked at runtime so `make install` on a host
# without `update-desktop-database` doesn't fail loudly; we just
# skip the cache refresh and warn.
HAVE_DBUS_SEND          := $(shell command -v dbus-send           >/dev/null 2>&1 && echo yes)
HAVE_GTK_UPDATE_ICON    := $(shell command -v gtk-update-icon-cache >/dev/null 2>&1 && echo yes)
HAVE_UPDATE_DESKTOP     := $(shell command -v update-desktop-database >/dev/null 2>&1 && echo yes)
HAVE_CARGO_AUDIT        := $(shell command -v cargo-audit           >/dev/null 2>&1 && echo yes)
HAVE_CARGO_DENY         := $(shell command -v cargo-deny            >/dev/null 2>&1 && echo yes)
HAVE_CARGO_DIST         := $(shell command -v cargo-nextest        >/dev/null 2>&1 && echo yes)
HAVE_CARGO_OUTDATED     := $(shell command -v cargo-outdated       >/dev/null 2>&1 && echo yes)

# Conventional cargo flags re-used throughout.
COMMON_BUILD_FLAGS  := --workspace --locked $(BUILD_FLAGS)
COMMON_TEST_FLAGS   := --workspace --lib

# Default goal = `build`. Putting it first means a bare `make` lands
# there.
.DEFAULT_GOAL := build

# ----------------------------------------------------------------------------
# Build pipeline
# ----------------------------------------------------------------------------

.PHONY: build
build: $(BINARY_PATH)
# `make` with no args = `make build`.
build: ## Build the release binary (default target)
	@echo "build: $(BINARY_PATH)"

$(BINARY_PATH): Cargo.lock $(shell find crates -type f -name '*.rs') $(shell find crates -type f -name '*.svg') \
	$(shell find $(PACKAGING_LINUX)/icons -type f) \
	$(RESOURCE_FILES)
	@echo "  + syncing icons from $(PACKAGING_LINUX)/icons/ into resources/ (hardlinks)"
	@mkdir -p $(RESOURCE_DIR)/icons/hicolor/scalable/apps
	@mkdir -p $(RESOURCE_DIR)/icons/hicolor/symbolic/apps
	@# Hardlink the SVG so glib-compile-resources sees a regular
	@# file at the expected path. hardlinks share the same inode
	@# — any update to $(PACKAGING_LINUX) is seen on rebuild.
	@cp -fl $(ICON_SCALABLE) $(RESOURCE_DIR)/icons/hicolor/scalable/apps/$(BINARY_NAME).svg
	@cp -fl $(ICON_SYMBOLIC) $(RESOURCE_DIR)/icons/hicolor/symbolic/apps/$(BINARY_NAME).svg
	$(CARGO) build --workspace --release --locked
	@echo "✓ $(BINARY_PATH) ready"
	@echo "  size: $$(stat --printf='%s' $(BINARY_PATH)) bytes"
	@echo "  deps: $$(ldd $(BINARY_PATH) 2>/dev/null | wc -l) shared libraries"

.PHONY: build-debug
build-debug: ## Build the debug binary
	$(MAKE) build BUILD=debug

.PHONY: build-all
build-all: build build-debug ## Build every profile
	@true

.PHONY: build-deps
build-deps: ## Print the runtime shared libraries the binary needs
	$(MAKE) build
	@ldd $(BINARY_PATH) | awk 'NR>1 {print $1}' | sort -u

# ----------------------------------------------------------------------------
# Validation gauntlet — mirrors AGENTS.md's ci.yml
# ----------------------------------------------------------------------------

.PHONY: test
test: ## Run cargo test on the workspace
	$(CARGO) test $(COMMON_TEST_FLAGS)

.PHONY: smoke
smoke: build ## The release-build smoke test (`tests/release_smoke.sh`)
	bash tests/release_smoke.sh

.PHONY: doc
doc: ## Build crate docs (no-deps)
	$(CARGO) doc -p bose-connect-gui --no-deps

.PHONY: fmt
fmt: ## Run cargo fmt --check (no write)
	$(CARGO) fmt --all -- --check

.PHONY: fmt-write
fmt-write: ## Run cargo fmt (write if needed)
	$(CARGO) fmt --all

.PHONY: clippy
clippy: ## Run cargo clippy -D warnings across the workspace
	$(CARGO) clippy $(COMMON_BUILD_FLAGS) --all-targets -- -D warnings

.PHONY: audit
audit: ## Cargo-audit + cargo-deny (skipped if not installed)
ifneq ($(HAVE_CARGO_AUDIT),)
	cargo audit
else
	@echo "audit: cargo-audit not installed; skipping"
endif
ifneq ($(HAVE_CARGO_DENY),)
	cargo deny check
else
	@echo "deny: cargo-deny not installed; skipping"
endif

.PHONY: outdated
outdated: ## Print outdated dependencies (skipped if cargo-outdated is not installed)
ifneq ($(HAVE_CARGO_OUTDATED),)
	cargo outdated --workspace
else
	@echo "outdated: cargo-outdated not installed; skipping"
endif

.PHONY: qa
qa: fmt clippy test smoke ## Run every gate in sequence
	@echo "✓ all gates green"

.PHONY: ci
ci: qa ## Same as `make qa`; alias for the GitHub Actions runner
	@true

# ----------------------------------------------------------------------------
# Installation (idempotent + sudo-friendly)
# ----------------------------------------------------------------------------

.PHONY: install
install: ## Install binary, .desktop, icons, bash completion, metainfo
	$(MAKE) build
	@echo "install: prefix=$(PREFIX)  destdir=$(DESTDIR)"
	@mkdir -p $(DEST_BINDIR)
	@mkdir -p $(DEST_APPS_DIR)
	@mkdir -p $(DEST_METAINFO_DIR)
	@mkdir -p $(DEST_ICON_SCAL_DIR)
	@mkdir -p $(DEST_ICON_SYM_DIR)
	@mkdir -p $(DEST_BASH_COMP_DIR)
	install -m 0755 $(BINARY_PATH)  $(DEST_BINDIR)/$(BINARY_NAME)
	install -m 0644 $(DESKTOP_FILE)   $(DEST_APPS_DIR)/$(BINARY_NAME).desktop
	install -m 0644 $(METAINFO_FILE) $(DEST_METAINFO_DIR)/com.airvzxf.bose-connect-gui.metainfo.xml
	install -m 0644 $(ICON_SCALABLE) $(DEST_ICON_SCAL_DIR)/$(BINARY_NAME).svg
	install -m 0644 $(ICON_SYMBOLIC) $(DEST_ICON_SYM_DIR)/$(BINARY_NAME).svg
	install -m 0644 $(BASH_COMPLETION) $(DEST_BASH_COMP_DIR)/$(BINARY_NAME)
	@echo "  ↳ binary           $(BINARY_NAME) → $(DEST_BINDIR)/$(BINARY_NAME)"
	@echo "  ↳ .desktop         $(DESKTOP_FILE) → $(DEST_APPS_DIR)/$(BINARY_NAME).desktop"
	@echo "  ↳ icons (2 sizes)  → $(ICON_DIR)/{{scalable,symbolic}}/apps/"
	@echo "  ↳ metainfo         $(METAINFO_FILE) → $(DEST_METAINFO_DIR)/"
	@echo "  ↳ bash-completion  → $(BASH_COMP_DIR)/$(BINARY_NAME)"
	@$(MAKE) install-post

.PHONY: install-post
install-post: ## Refresh icon + desktop caches after install (best-effort)
ifneq ($(HAVE_GTK_UPDATE_ICON),)
	@echo "  → gtk-update-icon-cache -f $(ICON_DIR)"
	@gtk-update-icon-cache -f $(DESTDIR)$(ICON_DIR) || true
endif
ifneq ($(HAVE_UPDATE_DESKTOP),)
	@echo "  → update-desktop-database $(APPS_DIR)"
	@update-desktop-database $(DEST_APPS_DIR) || true
endif

.PHONY: uninstall
uninstall: ## Remove everything `install` created
	@for f in $(INSTALLED_FILES); do \
	    if [ -e "$$f" ]; then \
	        echo "  - $$f"; \
	        rm -f "$$f"; \
	    fi; \
	done
	@$(MAKE) uninstall-post
	@echo "✓ uninstalled"

.PHONY: uninstall-post
uninstall-post: ## Refresh icon + desktop caches after uninstall (best-effort)
ifneq ($(HAVE_GTK_UPDATE_ICON),)
	@gtk-update-icon-cache -f $(DESTDIR)$(ICON_DIR) || true
endif
ifneq ($(HAVE_UPDATE_DESKTOP),)
	@update-desktop-database $(DEST_APPS_DIR) || true
endif

.PHONY: reinstall
reinstall: ## Uninstall + install in one step. Idempotent.
	@$(MAKE) uninstall
	@$(MAKE) install

.PHONY: install-user
install-user: ## Install to ~/.local (no sudo needed)
	$(MAKE) install PREFIX=$(HOME)/.local

.PHONY: uninstall-user
uninstall-user: ## Uninstall from ~/.local
	$(MAKE) uninstall PREFIX=$(HOME)/.local

.PHONY: reinstall-user
reinstall-user: ## Reinstall into ~/.local
	$(MAKE) reinstall PREFIX=$(HOME)/.local

# ----------------------------------------------------------------------------
# Packaging (deb / Arch)
# ----------------------------------------------------------------------------

.PHONY: deb
deb: build ## Build a Debian package (requires cargo-deb)
	cargo deb -p bose-connect-gui --no-build

.PHONY: arch
arch: ## Build an Arch package (requires cargo-aur)
	@echo "Use the existing PKGBUILD in packaging/aur/ — `cd packaging/aur && makepkg`"

# ----------------------------------------------------------------------------
# Cleaning
# ----------------------------------------------------------------------------

.PHONY: clean
clean: ## Remove build artefacts (keeps target dir structure)
	$(CARGO) clean

.PHONY: distclean
distclean: clean ## Remove target/ and generated gresource files
	rm -rf target
	@find . -name 'Cargo.lock.bak' -delete

# ----------------------------------------------------------------------------
# Diagnostics
# ----------------------------------------------------------------------------

.PHONY: tree
tree: ## Show the dependency tree
	$(CARGO) tree --workspace --all-features

.PHONY: audit-wire-sni
audit-wire-sni: ## Snapshot the SNI's IconPixmap + Status from a 4 s run
	$(MAKE) build
	@dbus-monitor --session "interface='org.kde.StatusNotifierItem'" > /tmp/sni-snap.log 2>&1 & \
	    MON=$$!; \
	    trap "kill $$MON 2>/dev/null" EXIT; \
	    timeout 4s ./target/release/$(BINARY_NAME) --headless --mock-tick-ms 250 --low-battery-test > /tmp/$(BINARY_NAME)-snap.log 2>&1 || true
	@echo "  ↳ /tmp/sni-snap.log captured"
	@gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher \
	    --method org.freedesktop.DBus.Properties.GetAll org.kde.StatusNotifierWatcher 2>&1 | head -10

.PHONY: version
version: ## Print the binaries' reported version
	./target/release/$(BINARY_NAME) --version 2>/dev/null || echo "(run \`make build\` first)"

.PHONY: help
help: ## Show this message
	@awk 'BEGIN {FS = ":.*##"; printf "Targets in this Makefile:\n\n"} \
	     /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# Suppress noisy intermediate file targets.
.PHONY: all
all: build ## Alias for `make build`
	@true
