# Idea: device profiles

> **Status: idea, not a decision.** This is a sketch of a possible
> direction, written down so it is not lost. Nothing here is
> implemented, agreed, or validated. Before anyone (human or AI agent)
> builds it, it needs a spike, alternatives, and review. Do not treat
> this page as a spec.

## Why think about this at all

Bose devices speak one family of packet protocol, but each product and
firmware bends it differently. A single hardware session on 2026-10-05
with three devices found differences in almost every area:

Devices: QC35 II (product `0x4020`, fw 4.8.1), QC Ultra (`0x4066`,
fw 1.6.7) and SoundLink Color II (`0x400d`, fw 4.0.1).

| Area                | QC35 II       | QC Ultra        | SoundLink Color II |
| ------------------- | ------------- | --------------- | ------------------ |
| RFCOMM channel      | 8             | 2               | 8                  |
| Device status       | extra packets | other layout    | as in the C code   |
| Volume scale byte   | 25, accepted  | 31, clamps over | 100, max is 99     |
| Media keys          | work          | rejected (`0c`) | work               |
| Audio modes         | no            | yes             | no                 |
| Connection limit    | none seen     | none seen       | off after the 9th  |

The library today mostly assumes one behaviour and patches differences
in place (`has_noise_cancelling(device_id)`, "refused on the QC Ultra"
checks in the CLI). That works for three devices, but every new product
adds more scattered `if` checks.

## The idea in one sentence

Ask the device who it is, look up what is known about that product,
and let that knowledge decide which commands are offered and how the
few differing ones behave, while every command keeps **one** shared
implementation.

## Business rules

1. **Identity comes from the device, never from its name.** The device
   reports its product id and index (`get_device_id`, `00 03 01 00`)
   and its firmware version. Names are user-editable ("Bose QC35 II 🐺")
   and must not be used to identify a product.
2. **One implementation per command.** There is a single
   "set volume", a single "get battery", and so on, written against the
   common protocol. A product never gets its own full copy of the
   command set. (A previous attempt duplicated every command per device,
   including the generic ones, and became unmaintainable. That is the
   failure mode to avoid.)
3. **A profile describes only differences.** For each known product, a
   profile records:
   * **capabilities**: which features exist (audio modes, noise
     cancelling, media keys, …);
   * **parameters**: values that differ (RFCOMM channel, how the volume
     scale byte is interpreted, payload layouts already decoded);
   * **quirks**: behaviour clients must respect (for example, "keep one
     connection open; it switches off after 9 connections").

   Anything a profile does not mention falls back to the common
   behaviour.
4. **A feature has three states, not two:** *supported* (verified on
   hardware), *unsupported* (verified that the device rejects it), and
   *unknown* (never tested). Unknown is not the same as unsupported. A
   client may offer an unknown feature, clearly marked as experimental.
5. **Unknown devices still work.** A product with no profile gets the
   common behaviour with every feature *unknown*, not an error.
6. **Evidence over assumption.** A profile entry is only "supported" or
   "unsupported" with a hardware capture behind it (device, firmware,
   date), in the spirit of `DEVELOPMENT.md`. Firmware matters: a
   profile can narrow an entry to a firmware range when behaviour
   changes between versions.
7. **Device errors are information.** When a device answers with an
   ERROR packet, the client shows a clear "not supported by this
   device" message instead of a generic failure, and the result is
   worth recording as evidence for the profile.

## Process flow

```text
 user picks a device (CLI address / GUI list)
        │
        ▼
 connect ──► channel from profile if known, else try the common ones
        │
        ▼
 identify ─► product id + index + firmware, asked from the device
        │
        ▼
 resolve profile
   exact product + firmware range ─► known profile
   product only                   ─► known profile, firmware unverified
   nothing                        ─► "generic": everything unknown
        │
        ▼
 present ──► CLI: flags for unsupported features fail early, with a
             clear message
             GUI: show supported, hide unsupported, mark unknown
        │
        ▼
 run a command
   shared implementation
     + parameters from the profile (e.g. volume scale meaning)
     + quirks respected (e.g. reuse the open connection)
        │
        ▼
 read the reply (header-based, as in #60)
   success      ─► result
   ERROR packet ─► "not supported here" + evidence for the profile
```

## Data that would flow

* **From the device**: product id, index, firmware version, and the
  replies themselves (including ERROR codes).
* **From the profile catalogue**: capabilities, parameters, quirks, and
  the evidence behind each.
* **To the user**: only what this device can do, and honest messages
  when it cannot.
* **Back into the catalogue** (manually, through PRs): new captures
  that turn *unknown* into *supported* or *unsupported*.

## Open questions for the spike

* Where does the catalogue live: Rust code, a data file compiled in, or
  something users can extend without rebuilding?
* How fine-grained are firmware ranges in practice? Do we need them
  now, or only product-level profiles?
* Can identification happen before the first real command on every
  device? (The QC Ultra only listens on channel 2, so "connect" may
  itself need a guess.)
* How do connection quirks (the 9-connection limit) shape the CLI,
  which today opens one connection per invocation?
* Should "unknown" features be probed automatically? Probing costs
  connections and can upset fragile devices, so probably not by
  default.
* How does this map onto the C FFI, which exposes plain functions
  today?

## What already exists to build on

* `get_device_id` reads the product id and index from the device.
* `has_noise_cancelling`, `has_self_voice`, `has_pairing_toggle` (and,
  with #60, `has_audio_modes`, `has_legacy_settings`) are per-product
  capability checks, scattered: a natural seed for a capability table.
* #60's header-based reply reader turns ERROR packets into
  `BoseError::DeviceError`, which is what rule 7 needs.
* `DEVELOPMENT.md` already records captures per device and firmware:
  the evidence rule 6 asks for.
