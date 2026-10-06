# Agent notes — Bose Connect for Linux (Rust port)

Notes for AI agents (and humans) working on this repository.
Conventions, validation gauntlet, and the migration-from-C
provenance for the protocol module.

## Validation gauntlet

Run all four locally before opening a PR. Mirrors `ci.yml`.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo doc --no-deps --workspace
cargo build --workspace --release --locked
```

Locally these also cover the graphical user interface (GUI) crate,
which needs the development files of the widget toolkit (GTK) 4.22,
libadwaita 1.9 and GLib 2.88. CI runs them on `ubuntu-latest` with
`--exclude bose-connect-gui` (its GTK is too old) and checks the GUI
separately in an Arch Linux container (the `gui` job).

The CI uses `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1`
(v7.0.1) and `dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772`
(master) pinned to SHAs; do not bump without a green CI run on
the new versions.

## Repository layout

See `README.md` for the layout diagram. The short version:

* `crates/bose-connect` — library crate. **Public Rust API** lives
  in `lib.rs` (the `BoseDevice` driver). The Bose protocol
  commands live in `protocol.rs` and are generic over the
  [`BoseIo`](crates/bose-connect/src/io.rs) trait so the integration
  tests can drive them over a `UnixStream` pair instead of needing
  a real headset. The C ABI lives in `ffi.rs` (gated behind
  `cfg(feature = "ffi")`, default-on).
* `crates/bose-connect-cli` — `bose-connect-app-linux` binary,
  clap-based CLI. Per-invocation connection lifecycle, just like
  the original `main.c`.
* `.github/workflows/` — see README.

## On-wire protocol provenance

The byte sequences in `protocol.rs` are translated line-for-line
from the C source files in the `main` branch's git history:

| Rust module                                   | C source                                |
| --------------------------------------------- | --------------------------------------- |
| `crates/bose-connect/src/protocol.rs`        | `src/library/based.c`                   |
| `crates/bose-connect/src/connection.rs`      | `src/main.c::get_socket` + `init_connection` |
| `crates/bose-connect/src/address.rs`          | `src/library/bluetooth.c`               |
| `crates/bose-connect/src/util.rs`             | `src/library/util.c`                    |

The replies are the exception: `protocol.rs` parses every reply
from its `block, function, operator, length` header instead of a
fixed-size acknowledgement (ACK), because the QuietComfort (QC)
Ultra Headphones (device id `0x4066`) send longer payloads for the
same functions. The
audio-mode functions (block `0x1f`), the RFCOMM channel fallback
(8, then 2) and the connection retry have no C counterpart; their
byte sequences were captured from a real QC Ultra Headphones on
firmware 1.6.7 and are recorded in `DEVELOPMENT.md` and in the
`qc_ultra_*` tests of `tests/protocol_roundtrip.rs`.

`git show <commit>:src/library/based.c` (the last commit that
touched the C source on `main`) is the source of truth. If a
packet byte disagrees between the two implementations, the C
version wins — the integration tests in
`crates/bose-connect/tests/protocol_roundtrip.rs` are there to
catch accidental drift, but they can only encode what *we
already documented*. When in doubt, run the binary against a
real Bose headphone and capture the on-wire bytes (the
`--send-packet` flag passes raw hex packets through for
ad-hoc fuzzing).

### Commands without a C counterpart

`set_volume`, `send_media_key`, `get_active_device` and
`get_device_bd_addr` do **not** exist in `based.c`, so the rule
above cannot apply to them. "Capture" below means a live capture
against a Bose SoundLink Color, second generation (II), referred to
as SoundLink Color II from here on. Their sources are:

| Command              | Request          | Source                    |
| -------------------- | ---------------- | ------------------------- |
| `set_volume`         | `05 05 02 01 xx` | `DEVELOPMENT.md`, capture |
| `send_media_key`     | `05 03 05 01 xx` | `DEVELOPMENT.md`, capture |
| `get_active_device`  | `05 01 01 00`    | `DEVELOPMENT.md`, capture |
| `get_device_bd_addr` | `00 06 01 00`    | capture only              |

For these, a real-hardware capture is the source of truth. Note in
the doc comment which device and firmware a response layout was
observed on; do not describe them as translated from C.

When testing against a SoundLink Color II, remember it switches
itself off after its 9th RFCOMM connection since power-on (see
"SoundLink Color II findings" in `DEVELOPMENT.md`). Batch requests
in one connection and count connections, or an unrelated packet
will look like the one that crashed it.

## Bluetooth address byte order

The `BdAddr` type stores bytes in **canonical MSB-first** order
(`b[0]` is the most-significant byte as printed in
"AA:BB:CC:DD:EE:FF"). The Bose device emits addresses in this
same order over the wire, so no byte swapping is needed. The
historical C names `reverse_ba2str` / `reverse_str2ba` are
misleading: both functions operate on the canonical MSB-first
representation; the `reverse_` prefix refers to them being
inverses of each other, not to any byte reversal.

## Toolchain split

* `fmt-check`, `clippy` → `dtolnay/rust-toolchain@stable` with
  `rustfmt` / `clippy` components. Determinism of formatter +
  lint output is the entire point.
* `test`, `doc`, `build` → `stable`. Build/test must keep passing
  against whatever stable is current at release time, otherwise
  the MSRV gate is a fiction.
* `release.yml` → `stable`. Reproducible release artefacts are a
  deliberate goal.

## Required CI checks

The ruleset `protect-main` (when enabled) requires:

* `fmt-check`
* `clippy`
* `test`
* `doc`
* `build`

`codeql` and `cargo-audit` are not blocking; they post to the
Security / code-scanning tabs only.

## Release flow

1. Land PR against `main`.
2. Tag the **merge commit** on `main` (not the branch tip):
   `git tag -s v0.1.0 $(git rev-parse origin/main)`
3. `git push origin v0.1.0`. This triggers `release.yml`.
4. The release workflow validates the tag shape, verifies
   reachability from `main`, builds the binary + C FFI library,
   publishes `bose-connect` to crates.io, and creates the GitHub
   Release with the binary + header attached.

Publishing to crates.io uses trusted publishing: the `publish` job
runs in the `release` environment (deployment rules: `v*` tags and
`main`) and exchanges its OpenID Connect (OIDC) token for a
30-minute crates.io token
with `rust-lang/crates-io-auth-action`. The crate's trusted publisher
on crates.io is bound to `airvzxf/bose-connect-app-linux`,
`release.yml` and the `release` environment; renaming the workflow
file or the environment breaks it. If that exchange fails, the job
falls back to a `CARGO_REGISTRY_TOKEN` repository secret when one is
configured.
