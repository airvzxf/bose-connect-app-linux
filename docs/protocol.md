# The Bose Connect protocol

What is known about the protocol, packet by packet, with the device and
firmware each observation comes from. The library implements it in
`crates/bose-connect/src/protocol.rs`; each command below names the
Rust call that sends it.

Firmware-update notes (not part of the control protocol) stay in
[`DEVELOPMENT.md`](../DEVELOPMENT.md).

## Contents

* [Conventions](#conventions)
* [Connection](#connection)
* [From Rust](#from-rust)
* [Block 00: device identity](#block-00-device-identity)
* [Block 01: settings](#block-01-settings)
* [Block 02: battery](#block-02-battery)
* [Block 04: paired devices](#block-04-paired-devices)
* [Block 05: media and volume](#block-05-media-and-volume)
* [Block 09: notifications](#block-09-notifications)
* [Block 1f: audio modes (QC Ultra, QC45)](#block-1f-audio-modes-qc-ultra-qc45)
* [Other blocks (QC Ultra)](#other-blocks-qc-ultra)
* [Device notes](#device-notes)

## Conventions

`>` means a packet sent to the device, `<` a packet received. Bytes are
hexadecimal.

Every packet is a 4-byte header followed by a payload:

```text
block  function  operator  length  payload (length bytes)
```

Operators seen:

| Operator | Meaning    | Direction                                   |
| -------- | ---------- | ------------------------------------------- |
| `01`     | GET        | request                                     |
| `02`     | SET        | request                                     |
| `03`     | STATUS     | reply with the current value                |
| `04`     | ERROR      | reply; the first payload byte is a code     |
| `05`     | (action)   | request that starts an action: status dump, connect, media key |
| `06`     | RESULT     | reply; the action finished                  |
| `07`     | PROCESSING | reply; more packets follow                  |

The library reads every reply from its header (`check_reply` and
`read_response` in `protocol.rs`), including the commands whose C code
compared a fixed acknowledgement; the bytes sent are the C ones. A
packet for another `block.function` is a notification and is skipped,
ERROR becomes `BoseError::DeviceError`, PROCESSING is skipped. Before
each request the bytes still waiting in the socket are dropped
(`BoseIo::bose_send`).

Error codes seen (first payload byte of an ERROR packet):

| Code | Meaning (inferred)                 | Where                                                    |
| ---- | ---------------------------------- | -------------------------------------------------------- |
| `03` | block does not exist               | GET `xx 00` of an unknown block (QC Ultra)               |
| `04` | function does not exist            | unknown functions (QC Ultra)                             |
| `05` | function cannot be read this way   | e.g. `00 04`, `1f 01` (QC Ultra)                         |
| `06` | value out of range                 | volume, equalizer; audio-mode slot ≥ 4 (QC45)            |
| `08` | invalid slot / not available now   | audio-mode slot ≥ 10; `01 05` while the mode is modified (QC Ultra) |
| `0b` | not a paired source                | connect to the device's own or an unknown address (all three) |
| `0c` | function locked / busy             | media keys and the equalizer on the QC Ultra, see below; music status with unknown music |
| `0d` | a started action failed            | connect to its own address, after PROCESSING (QC Ultra)  |
| `18` | access denied                      | blocks `08`, `16`–`1a` (QC Ultra); probably need authentication |

### Unsolicited packets

Devices send packets nobody asked for, on the same connection:

* the SoundLink Color II answers `05 05 02 01 14` (volume) twice;
* `05 01` active-source changes after a connect or disconnect;
* after a media key, the play state (`05 04`) and the new track
  (`05 06`: title `00`, artist `01`, album `02`), sometimes twice:

```text
< 05 03 07 00  05 03 06 00
< 05 04 03 03 01 00 c8
< 05 06 07 00
< 05 06 03 13 00 "Summertime Sadness"
< 05 06 03 0d 01 "Lana Del Rey"
< 05 06 03 35 02 "Born To Die - The Paradise Edition (Special Version)"
< 05 06 06 00
```

The QC Ultra sent none of these to the laptop while the user paused
and skipped tracks on the phone for 75 s (channel 1, Bose app on the
phone holding channel 2): it seems to notify only the client that
asked.

## Connection

RFCOMM. The library tries channel 8 first (QC35, SoundLink Color II),
then 2, then 1 (QC Ultra: SDP service "SPPS De" on channels 1 and 2,
one client per channel; the phone's Bose app holds one of them,
whichever it got first). It repeats that sequence up to 3 times, 1 s
apart. The receive timeout is 1 s.

Every connection starts with the handshake (C):

```text
> 00 01 01 00
< 00 01 03 05 ...
```

Limits observed:

* The QC Ultra refuses a new connection for about a second after the
  previous one closed.
* The SoundLink Color II switches itself off after its 9th connection
  since power-on. See [SoundLink Color II](#soundlink-color-ii).
* A SoundLink Color II keeps at most two connections. Asking it to
  connect a third source drops one of the others, the laptop included.

## From Rust

`BoseDevice` opens the connection, runs the handshake and exposes one
method per command. Keep one `BoseDevice` for a whole session instead
of reconnecting per command (see the SoundLink Color II limit).

```rust
use bose_connect::{BoseDevice, BoseError, MediaKey};

fn main() -> Result<(), BoseError> {
    let mut device = BoseDevice::open("AA:BB:CC:DD:EE:FF")?;

    let (product, _index) = device.device_id()?;
    println!("product {product:#06x}, firmware {}", device.firmware_version()?);
    println!("battery {} %", device.battery_level()?);

    match device.send_media_key(MediaKey::Next) {
        Err(BoseError::DeviceError { code, .. }) => println!("refused, code {code:#04x}"),
        other => other?,
    }
    Ok(())
}
```

The protocol functions are also public and generic over the `BoseIo`
transport, which is how the tests drive them over a `UnixStream`:

```rust
use std::os::unix::net::UnixStream;
use bose_connect::protocol::get_battery_level;

fn battery(stream: &mut UnixStream) -> bose_connect::BoseResult<u8> {
    get_battery_level(stream)
}
```

## Block 00: device identity

### `00 03` device id (C) — `device_id()`

```text
> 00 03 01 00
< 00 03 03 03 pp pp ii
# pppp: product id, big endian. ii: index.
```

Product ids seen: `4014`, `4020` (QC35 series), `400c` (noise
cancelling, from the C code), `400d` (SoundLink Color II), `4066` (QC
Ultra), `4039` (QC45).

### `00 05` firmware version — `firmware_version()`

Length taken from the header: 5 characters on the QC35 (`1.3.2`),
longer on the QC Ultra:

```text
> 00 05 01 00
< 00 05 03 0e 31 2e 36 2e 37 2b 67 36 65 62 61 62 64 32
# "1.6.7+g6ebabd2"
```

### `00 06` own Bluetooth address — `device_bd_addr()`

No C counterpart. Captured on a SoundLink Color II (4.0.1):

```text
> 00 06 01 00
< 00 06 03 06 04 52 c7 ba 68 0d
# the speaker's own address, 04:52:C7:BA:68:0D
```

### `00 07` serial number (C) — `serial_number()`

```text
> 00 07 01 00
< 00 07 03 ll <ll bytes of ASCII>
```

## Block 01: settings

### `01 01` status dump — `device_status()`

```text
> 01 01 05 00
< 01 01 07 00
< 01 xx 03 ll ...      # one STATUS packet per setting
< 01 01 06 00
```

The library reads the name (`01 02`), language (`01 03`), auto-off
(`01 04`) and noise cancelling (`01 06`) and skips the rest.

QC35 II, firmware 4.8.1 (more packets than the C parser expected, which
is why `--info` failed on it before #60):

```text
< 01 01 07 00
< 01 02 03 12 00 <name>
< 01 03 03 05 a6 00 04 cf de
< 01 04 03 01 14
< 01 06 03 02 01 0b
< 01 09 03 04 10 04 01 07
< 01 0b 03 03 01 01 0f
< 01 01 06 00
```

QC Ultra, firmware 1.6.7: `01 00`, `01 02`, `01 03`, `01 04`, `01 05`,
`01 07`, `01 09`, `01 0a`, `01 0b`, `01 0c`, `01 18` and `01 1b`. There is
no `01 06`: noise cancelling is replaced by the audio modes of block
`1f`.

### `01 02` name — `set_name(name)`

```text
> 01 02 02 ll <name>
< 01 02 03 ll 00 <name>
```

At most 31 bytes. The reply echoes the name after a `00` byte.

### `01 03` prompt language and voice prompts — `set_prompt_language(byte)`, `set_voice_prompts(on)`

```text
> 01 03 02 01 xx
< 01 03 03 ll xx ...
```

Low 5 bits = language, bit 5 (`0x20`) = voice prompts on. Languages:
`21` EN, `22` FR, `23` IT, `24` DE, `26` ES, `27` PT, `28` ZH, `29` KO,
`2a` RU, `2b` PL, `2e` NL, `2f` JA, `32` SV.

QC Ultra (1.6.7) sends 7 bytes:

```text
> 01 03 01 00
< 01 03 03 07 a6 00 01 81 5e 01 01
```

| Byte    | Meaning                                                      |
| ------- | ------------------------------------------------------------ |
| 1, low 5 bits | language (`06` Spanish; `01` English in an older capture `e1`) |
| 1, `0x20` | "Connections and phone calls" prompts (the voice-prompts bit) |
| 1, `0x80`, `0x40` | unknown (`0x80` always set; `0x40` seen with English) |
| 2–6     | unknown, constant (`00 01 81 5e 01`)                         |
| 7       | "Battery level at power-on" prompt — `set_battery_prompt(on)` |

QC45 (4.0.4) sends the same 7 bytes, `81 00 01 81 5e 00 00` (English,
prompts off), but has no battery prompt: `01 03 02 02 81 01` is
answered with the unchanged payload, and its app offers the battery
level only as the shortcut action (`01 09`).

Its byte 1 has the QC Ultra's layout: `0x80` always set (meaning
unknown), `0x20` voice prompts, low 5 bits the language. Its app lists
8 languages; English is `81` (`a1` with prompts on), German `a4`. The
writes keep `0x80`:

```text
> 01 03 02 01 a1                  # voice prompts on: no reply at all
> 01 03 01 00                     # GET 0.2 s later: still the old byte
< 01 03 03 07 81 00 01 81 5e 00 00
> 01 03 01 00                     # GET 0.4 s later: the new byte
< 01 03 03 07 a1 00 01 81 5e 00 00
> 01 03 02 01 a1                  # from German (a4) to English: answered
< 01 03 03 07 a1 00 01 81 5e 00 00
```

A voice-prompt write is never answered, and a GET sent right after it
may get no reply either; `set_prompt_language` then reads the byte back
with a GET until it matches (2 s at most). The headphones announce each
change. Right after a language change, a new connection was refused
twice; a few seconds later it worked.

Writes verified from a laptop: `01 03 02 01 86` (prompts off, the
1-byte QC35 form) and `01 03 02 02 a6 00` / `a6 01` (battery prompt off
/ on). The 7-byte write `01 03 02 07 …` changes byte 1 only. Changing
the language itself has not been tried on the QC Ultra, so the CLI
refuses `--prompt-language` there.

### `01 04` auto-off — `set_auto_off(minutes)`

```text
> 01 04 02 01 mm
< 01 04 03 01 mm
# mm: 00 never, 05, 14 (20), 28 (40), 3c (60), b4 (180) minutes
```

QC Ultra (1.6.7): 3 bytes, the minutes little-endian around a zero
byte. Each value below was set in the app and read back, and 40
minutes was then set from a laptop:

```text
> 01 04 01 00
< 01 04 03 03 05 00 00      # 5 minutes
< 01 04 03 03 14 00 00      # 20
< 01 04 03 03 b4 00 00      # 180
< 01 04 03 03 a0 00 05      # 1440 (24 hours)
< 01 04 03 03 00 00 00      # never
> 01 04 02 03 28 00 00      # set 40 minutes
< 01 04 03 03 28 00 00
```

`set_auto_off` reads the layout first and writes the matching form.

### `01 05` stepped noise cancelling (NC 700) — `noise_cancelling_steps()`, `set_noise_cancelling_level(level)`

```text
> 01 05 01 00
< 01 05 03 03 0b 0a 01          11 steps, raw step 10, enabled flag 1
> 01 05 02 02 00 01            raw step 0, flag 1
```

The payload is `steps`, `raw step`, `enabled`. **The raw step counts the
other way round to the Bose Music app:** raw `0` is the strongest
cancelling (the app's 10) and raw `10` is off (the app's 0). The
library takes the app's number and writes `10 - level`. The device sends
no reply to the write (firmware 1.8.2), so the value is read back with
`01 05 01 00`. The enabled flag is `1` in every capture and is left at
`1`. Only the NC 700 (`4024`) answers; on the QC35 this function does
not exist, and the NC 700 answers `01 06` with ERROR `04`.

### `01 0e` remember the last noise-cancelling step (NC 700) — `remember_noise_cancelling()`, `set_remember_noise_cancelling(on)`

`01 0e 02 01 xx`, `xx` `01` on, `00` off; reply `01 0e 03 01 xx`. Like
`01 05` the write gets no reply, so it is read back. The Bose Music app
calls it "Remember My Last Setting".

### `01 0f` noise-cancelling favourites (NC 700) — `noise_cancelling_favorites()`, `set_noise_cancelling_favorites(levels)`

```text
> 01 0f 01 00
< 01 0f 03 04 02 00 05 0a       index 2 selected; raw steps 0, 5, 10
> 01 0f 02 04 02 01 06 09       same index; raw steps 1, 6, 9
```

The payload is the selected index followed by one raw step per
favourite. The app showed this device's favourites as 10, 5 and 0, which
are the raw steps 0, 5 and 10. The library keeps the selected index when
it writes the three values. Out-of-range values are never sent.

### `01 14` motion auto-off (NC 700) — `motion_auto_off()`, `set_motion_auto_off(on)`

`01 14 02 01 xx`; the device echoes `01 14 03 01 xx`. When on, the
headphones switch off after an hour without motion whatever the
auto-off timer says (the app's "Motion Detection").

### `01 06` noise cancelling — `set_noise_cancelling(level)`

```text
> 01 06 02 01 xx
< 01 06 03 02 xx 0b
# xx: 00 off, 01 high, 03 low. The second byte (0b on a QC35 II) is unknown.
```

Only on product ids `4014`, `4020`, `400c` (`has_noise_cancelling`).

### `01 07` equalizer (QC Ultra, QC45) — `equalizer()`, `set_equalizer_band()`

```text
> 01 07 01 00
< 01 07 03 0c f6 0a 00 00 f6 0a 00 01 f6 0a 00 02
# Per band: min (-10), max (10), current value (signed), band (0 bass, 1 mid, 2 treble)
> 01 07 02 02 05 00         # bass +5
< 01 07 03 0c f6 0a 05 00 f6 0a 00 01 f6 0a 00 02
> 01 07 02 02 fd 02         # treble -3
```

The QC45 (4.0.4) answers the same bytes; `01 07 02 02 01 00` (bass +1)
and `01 07 02 02 00 00` were verified.

**Out of range locks it.** `01 07 02 02 0b 01` (mid +11) was answered
with ERROR `06`; from then on every `01 07` request, read or write,
got ERROR `0c`, and the Bose app showed "there was an error" for the
equalizer, until the headphones were switched off and on. The library
refuses values outside the reported range before sending.

### `01 09` buttons: shortcut (QC Ultra, QC45) — `shortcut()`, `set_shortcut(action)`

```text
> 01 09 01 00
< 01 09 03 07 80 09 03 00 09 40 0a
# 80: right earcup volume strip, 09: touch and hold, 03: action,
# 00 09 40 0a: unknown, constant
> 01 09 02 03 80 09 13      # action: cycle immersive audio
```

Actions: `01` voice assistant, `03` battery level, `10` Spotify, `13`
immersive audio, `0e` shortcut off. Turning the shortcut back on in
the app always gives `03`, whatever it was before. The write was
verified with the button itself.

QC45 (4.0.4): an 11-byte payload with the action at the same place. Its
app offers "Hear battery level" (`03`) and Spotify (`10`). Setting `03`
from a laptop was verified in the app and with the button.

```text
> 01 09 01 00
< 01 09 03 0b 80 09 10 00 01 40 08 00 00 00 80   # Spotify, set in the app
> 01 09 02 03 80 09 03                              # battery level
< 01 09 03 0b 80 09 03 00 01 40 08 00 00 00 80
```

### `01 0a`, `01 0c` (QC Ultra)

Read `07` and `01`; meaning unknown.

### `01 0b` self voice (C) — `set_self_voice(level)`

```text
> 01 0b 02 02 01 xx
< 01 0b 03 03 01 xx 0f
# xx: 00 off, 01 high, 02 medium, 03 low
```

The C request was `01 0b 02 02 01 xx 38`: it declares 2 payload bytes
but carries 3, and the device reads the trailing `38` as the start of
the next request (on a QC Ultra the following read was answered with
`38 01 04 01 03`, ERROR, block `38` does not exist). The library sends
the request without it, verified on a QC Ultra ("automatic voice" in
the app), a QC35 II and a QC45.

QC45 (4.0.4), low before; the app showed high afterwards. Self-voice
only acts during calls: in a phone call, off, low, medium and high set
from a laptop were heard as such.

```text
> 01 0b 02 02 01 01
< 01 0b 03 03 01 01 0f
```

### `01 18`, `01 1b` on-head detection (QC Ultra)

```text
> 01 18 02 01 xx    # play/pause automatically when taken off / put on
> 01 1b 02 01 xx    # answer calls when put on
< 01 18 03 01 xx
# xx: 00 off, 01 on
```

`auto_play_pause()`, `auto_answer_calls()` and their setters.

## Block 02: battery

### `02 02` battery level — `battery_level()`

```text
> 02 02 01 00
< 02 02 03 01 xx          # QC35: xx = percent
< 02 02 03 04 64 ff ff 00 # QC Ultra: 0x64 = 100 %, last 3 bytes unknown
```

## Block 04: paired devices

### `04 04` paired list (C) — `paired_devices()`

```text
> 04 04 01 00
< 04 04 03 ll cc <ll bytes: one 6-byte address per device>
# number of devices = ll / 6. cc: 01 one device connected, 03 two
```

The list is ordered by most recent use: on a SoundLink Color II the
laptop came first while it was the active source, the phone after it
was (2026-11-02).

### `04 05` device info (C) — `device_info(address)`

```text
> 04 05 01 06 <addr>
< 04 05 03 ll <addr> ss <extra> <name>
# ss bits 0-1: 00 disconnected, 01 connected, 03 this device.
# ss bit 2 (04): a Bose device; <extra> is then 3 bytes, else 2.
```

SoundLink Color II, firmware 4.0.1 (2026-11-02), asked about the three
devices in its list:

```text
< 04 05 03 20 2c41a10b7c82 c4 40 0d 01 "Bose SLC II Black 🐺"
< 04 05 03 26 d46d6d174da4 03 01 03 "GT73EVR-7RF-Titan-Pro-CachyOS"
< 04 05 03 1d 0062018c0cb7 01 02 03 "motorola edge 50 neo"
```

For the other speaker the 3 bytes start with its product id `40 0d`.
The meaning of the last byte (`01`) and of the 2 bytes of other
devices is unknown, and so are the high status bits (`c4` here, `54`
in an earlier capture of the same speaker).

### `04 01` connect a source — `connect_device(address)`

```text
> 04 01 05 07 00 <addr>
< 04 01 07 06 <addr>               # PROCESSING (the C acknowledgement)
  ... notifications (05 01 active source) ...
< 04 01 06 07 <addr> cc            # RESULT, up to ~8 s later
```

Captures on the SoundLink Color II and the QC35 II, reconnecting the
phone: one to three RESULTs with `cc` = `01`, `05`, `07`, mixed with
active-source notifications. A source that is already connected gets
PROCESSING and `RESULT … 07` at once. Errors:

```text
< 04 01 04 07 0b <addr>            # not a paired source (own or unknown address)
< 04 01 07 06 <addr>  04 01 04 07 0d <addr>   # QC Ultra, its own address
```

The library returns on the RESULT that carries the address, waiting
up to 15 s, and reports any ERROR.

**Side effects.** Asking a SoundLink Color II to connect a paired
source that is switched off made it drop the laptop to free a slot
and then announce "device not found". Asking the QC Ultra to connect
its own address dropped the phone. After a disconnect and reconnect
the phone's music stays paused, and once the speaker played nothing
until the volume was touched.

### `04 02` disconnect a source — `disconnect_device(address)`

```text
> 04 02 05 06 <addr>
< 04 02 07 06 <addr>               # C acknowledgement
< 04 02 07 00  04 02 07 06 <addr>  05 01 03 09 00 02 01 00 00 00 00 00 00  04 02 06 06 <addr>
                                   # SoundLink Color II
< 04 02 07 07 21 <addr>  05 01 …  04 02 06 06 <addr>     # QC35 II
```

### `04 03` forget a source — `remove_device(address)`

```text
> 04 03 05 06 <addr>
< 04 03 06 06 <addr>        # C acknowledgement
                            # SoundLink Color II 4.0.1: no reply at all
```

The library treats "no byte before the receive timeout" as success.
Whether a late reply follows is not known.

### `04 08` pairing mode (C) — `set_pairing(on)`

```text
> 04 08 05 01 xx
< 04 08 06 01 xx
# xx: 00 off, 01 on
```

Allowed on the QC35 series and the SoundLink Color II
(`has_pairing_toggle`). SoundLink Color II, firmware 4.0.1
(2026-11-02):

```text
> 04 08 05 01 01
< 04 08 07 00                       # PROCESSING
< 04 08 06 01 01                    # RESULT
< 05 01 03 09 00 02 01 00 00 00 00 00 00
< 04 02 06 06 <laptop>  04 02 06 06 <phone>
> 04 08 01 00
< 04 08 03 01 01
> 04 08 05 01 00
< 04 08 06 01 00
```

Entering pairing mode disconnected both audio sources (the laptop and
the phone); the control connection stayed open. Leaving it does not
reconnect them, and the speaker then took a few seconds to answer a
new connection.

## Block 05: media and volume

These packets were first listed by the original author, who could not
make media keys work; the replies were captured later on the devices
named below. None of them exists in the C code.

### `05 01` active source — `active_device()`

```text
> 05 01 01 00
< 05 01 03 09 00 02 01 xx xx xx xx xx xx
# xx…: the address of the source playing now
```

The meaning of `00 02 01` is unknown; it was the same on every capture
on the SoundLink Color II and the QC35 II; the QC Ultra sends `00 0f 01`.

### `05 02` music status (not implemented)

```text
> 05 02 05 00
< 05 02 07 00 05 03 03 02 01 fe 05 04 03 03 xx yy yy
  05 05 03 02 19 0b zz 06 07 00 xx = 02
# Paused xx = 01
# Playing xx = 00
# Unknown yyyy
# Elapsed time of music (may be ffff for unknown?)
# 00 <= zz <= 18: volume

< 05 02 04 01 0c
# Sent if unknown music
```

### `05 03` media keys — `send_media_key(key)`

```text
> 05 03 05 01 xx
# xx: 01 play/pause toggle, 03 next, 04 previous
< 05 03 07 00  05 03 06 00     # QC35 II, SoundLink Color II
```

SoundLink Color II (4.0.1): `04` restarts the current track when it has
been playing for a few seconds, like a physical button.

`02` pauses only when the source tells the speaker that it is playing.
With an Android phone playing Spotify (2026-11-02), `02` paused it and
`01` resumed it, each time within a second:

```text
> 05 03 05 01 02
< 05 03 07 00  05 03 06 00  05 04 03 03 02 00 51   # paused at 81 s
> 05 03 05 01 01
< 05 03 07 00  05 03 06 00  05 04 03 03 01 00 51   # playing
< 05 06 07 00  05 06 03 .. 00 <title>  ... 05 06 06 00
```

With the laptop as the source (an earlier test), `02` was acknowledged
the same way but the player saw nothing, whether it was playing or
paused. The library does not expose `02`.

QC Ultra (1.6.7, 2026-10-07), with multipoint on, the phone playing
and the laptop connected on channel 1:

```text
> 05 03 05 01 03               # next: the track changed
< 05 03 07 01 03  05 03 06 01 03
> 05 03 05 01 01               # play/pause: nothing happened
< 05 03 07 01 01               # PROCESSING, never a RESULT
> 05 03 05 01 xx               # any key afterwards, 00 to 0f
< 05 03 04 01 0c
```

After the unanswered `01`, every key got ERROR `0c`, from new
connections too, and the phone's own play, pause and skip stopped
working, until the phone and headphones were disconnected and
reconnected. The library refuses to send `01` to a QC Ultra from the
CLI. The user noticed that media control on the phone also stopped
once when the laptop connected, before any key was sent. A test on
2026-10-29 did not reproduce that: with the laptop disconnected,
cleanly connected, and connected without audio, the headphone button
and the phone's controls kept working. Key `01` remains the only
proven cause.

QC45 (4.0.4, 2026-10-31), the source playing music:

```text
> 05 03 05 01 01               # play/pause
< 05 03 07 01 01               # PROCESSING, no RESULT within 10 s
```

With the laptop as the active source, playback paused, and the laptop's
and the headphones' own controls kept working. With the phone as the
active source, nothing happened. The CLI refuses `01` on the QC45 too.

### `05 05` volume — `set_volume(level)`

```text
> 05 05 02 01 xx
< 05 05 03 02 mm xx     # mm: top of the scale, xx: the level now set
< 05 05 04 01 06        # ERROR: out of range
```

`mm` is not the battery level: it never changes with the level or the
battery. Devices treat it differently:

| Device                     | `mm`       | Accepted | Above the range        |
| -------------------------- | ---------- | -------- | ---------------------- |
| QC35 II, firmware 4.8.1    | `19` (25)  | `00..19` | ERROR `05 05 04 01 06` |
| QC Ultra, firmware 1.6.7   | `1f` (31)  | `00..1f` | clamped to `1f`        |
| SoundLink Color II, 4.0.1  | `64` (100) | `00..63` | ERROR `05 05 04 01 06` |

The original note gave the QC35 range as `00 <= xx <= 18`; `19` is
accepted too. Each source seems to keep its own volume.

SoundLink Color II capture:

```text
> 05 05 02 01 14
< 05 05 03 02 64 14
> 05 05 02 01 64
< 05 05 04 01 06
```

## Block 09: notifications

### `09 02` subscribe by function block — `enable_notifications(blocks)`, `read_notification()`

```text
> 09 02 02 04 01 05 02 37      mask 01 (enable), blocks 0, 1, 2, 4, 5, 9, 16, 18
> 09 02 02 02 01 04            mask 01, block 2 only
```

The payload is a mask byte (`00` overwrite, `01` enable, `02` disable)
and a big-endian bit set with block `n` at bit `n`; the Bose Music app
sends the first line. The subscription lasts for the connection. From
then on the device sends status packets (operator `3`) by itself.
Captured on the NC 700 (firmware 1.8.2) with the CLI's `--watch`:

* `01 05 03 03 0b <raw> 01`, then `01 0f 03 04 <index> <raw>…`: the
  noise-cancelling step changed, from the button or from the app.
* `05 05 03 02 10 <level>`: the volume changed (scale 16).
* A burst of about 40 packets when the playing source changes: name
  `01 02`, paired list `04 04`, device info `04 05` for each paired
  device, the active source `05 01` and now-playing `05 06`.

**There are no key-press packets on the NC 700.** `02 0c` (the app's
`StatusButton`) describes a button's configuration, and the NC 700
answers it with ERROR `04`; `02 01` answers ERROR `05`. The shortcut
button (tap and hold) produced no packet while a subscription was open.
A press can only be seen through what it changes.

## Block 1f: audio modes (QC Ultra, QC45)

Captured on a QC Ultra, firmware 1.6.7. The QC45 differences are below.

```text
> 1f 03 01 00                       # Current audio mode — audio_mode()
< 1f 03 03 01 xx
# xx = slot index; ff = the mode was modified (e.g. immersive audio
#      changed away from the mode's own setting)

> 1f 06 01 01 xx                    # Audio mode of slot xx — audio_modes()
< 1f 06 03 2f xx 00 01 00 00 01 51 75 69 65 74 00 ...
# 47-byte payload: slot index, 5 bytes of settings, the name NUL-padded
# to 32 bytes ("Quiet"), then 9 more bytes of settings.
< 1f 06 04 01 08                    # Error for slot 10 and above

> 1f 03 05 02 xx 00                 # Switch to slot xx — set_audio_mode(xx)
< 1f 03 06 01 xx
# The last request byte 00 switches without the voice-prompt announce.
```

The settings bytes of the three built-in modes:

| Slot | Name      | Bytes 1–5          | Last 9 bytes                    |
| ---- | --------- | ------------------ | ------------------------------- |
| 0    | Quiet     | `00 01 00 00 01`   | `00 00 00 00 00 00 00 00 00`    |
| 1    | Aware     | `00 02 00 00 01`   | `00 00 00 02 0a 00 00 00 00`    |
| 2    | Immersion | `00 22 00 00 01`   | `00 00 00 00 00 00 02 00 00`    |
| 3–9  | None      | `00 00 01 00 00`   | `00 00 00 0d 0a 00 00 01 00`    |

Creating or editing modes (`1f 06` SET) is not known yet.

Other functions of the block:

```text
> 1f 00 01 00   < 1f 00 03 05 "1.0.0"          # block version
> 1f 02 01 00   < 1f 02 03 07 03 07 00 00 00 1f 02   # unknown
> 1f 05 01 00   < 1f 05 03 01 01               # "Remember my mode" — remember_audio_mode()
> 1f 08 01 00   < 1f 08 03 03 0a 00 07         # unknown
```

`1f 05 02 01 xx` sets "Remember my mode" (verified).

### Audio modes on the QC45

Captured on firmware 4.0.4-4360+de6a887, 2026-10-31. Same requests and
the same 47-byte `1f 06` payload, with these differences:

```text
> 1f 06 01 01 04                    # slot 4 and above
< 1f 06 04 01 06                    # ERROR 06, not 08

> 1f 03 05 02 xx 00                 # Switch to slot xx
< 1f 03 07 00                       # PROCESSING only, no RESULT within 10 s
> 1f 03 01 00                       # sent right after: already the new slot
< 1f 03 03 01 xx
```

`set_audio_mode` reads the slot back with a GET after a PROCESSING
packet.

| Slot | Name    | Bytes 1–5          | Last 9 bytes                    |
| ---- | ------- | ------------------ | ------------------------------- |
| 0    | Quiet   | `00 01 00 00 01`   | `00 00 00 01 00 00 00 00 00`    |
| 1    | Aware   | `00 02 00 00 01`   | `00 00 00 01 0a 00 00 00 00`    |
| 2    | Commute | `00 07 01 01 01`   | `00 00 00 09 00 00 00 00 01`    |
| 3    | (empty) | `00 00 01 01 00`   | `00 00 00 09 05 00 00 00 00`    |

Slot 3 has an empty name instead of "None"; `audio_modes()` skips it.

```text
> 1f 00 01 00   < 1f 00 03 05 "1.0.0"          # block version
> 1f 02 01 00   < 1f 02 03 06 02 02 00 00 00 09   # unknown
> 1f 05 01 00   < 1f 05 03 01 00               # "Remember my mode"
> 1f 08 01 00   < 1f 08 03 02 04 07            # unknown
```

`1f 05 02 01 xx` sets "Remember my mode" (verified).

## Other blocks (QC Ultra)

A GET of function `00` of every block from `00` to `3f` (firmware
1.6.7) answers the block version for `00`–`07`, `09`, `0d`, `12` and
`1f` (all "1.1.0" except `03` "1.2.0" and `1f` "1.0.0"), ERROR `18`
for `08` and `16`–`1a`, and ERROR `03` for the rest. A GET of every
function `00`–`1f` of the existing blocks answered 111 times; the
known ones are in the sections above. Some others:

| Request    | Reply payload                                | Note                         |
| ---------- | -------------------------------------------- | ---------------------------- |
| `00 0c`    | 16 bytes                                     | an id                        |
| `00 0f`    | "Bose QC Ultra Headphones"                   | product name                 |
| `00 11`    | `00` + own address                           |                              |
| `04 09`    | the laptop's address                         | the requesting device?       |
| `05 0f`    | `00`/`01`/`02`                               | immersive audio, see below   |
| `05 11`    | 98 bytes                                     | unknown                      |
| `07 01`    | `08 73`                                      | unknown                      |
| `0d 01`    | `1f 83`                                      | unknown                      |
| `12 09`    | a PEM public key                             | authentication               |
| `12 0c`    | "Lonestarr"                                  | platform codename            |
| `12 0d`    | "OTG-QCC-384"                                | Qualcomm QCC384x chip        |

### `05 0f` immersive audio — `immersive_audio()`, `set_immersive_audio(setting)`

```text
> 05 0f 01 00
< 05 0f 03 01 xx
# xx: 00 off, 01 still ("Fijo"), 02 motion ("Movimiento")
> 05 0f 02 01 01    # set still: the app followed and the sound stayed in front
```

Any value other than the mode's own makes `1f 03` read `ff` and `01 05`
answer ERROR `08`.

## Device notes

### QC35 II

Product `4020`, firmware 4.8.1. Channel 8. Status dump with extra
packets (above). Volume scale 25. Media keys work.

### QC Ultra Headphones

Product `4066`, firmware 1.6.7, codename "Lonestarr", Qualcomm
QCC384x. Channels 2 and 1, one client each. Longer payloads for
firmware, battery, language and auto-off; no noise cancelling, audio
modes instead. Volume scale 31, clamped. "Next" works; play/pause
locks media control (see block 05). With multipoint off it accepts
one source at a time, so the laptop needs multipoint on to connect
next to the phone. Refuses a new connection for about a second after
the previous one. Sends no notifications to a second client.

### QC45

Product `4039`, firmware 4.0.4-4360+de6a887. Channel 8. No `01 06`:
`01 06 01 00` answers ERROR `04`, so no noise cancelling, audio modes
instead (four slots, see block 1f). The 7-byte `01 03` payload of the
QC Ultra, without its battery prompt; voice prompts and language
verified (see `01 03`). Equalizer, shortcut and self-voice as on the
QC Ultra (see `01 07`, `01 09`, `01 0b`). Play/pause never completes (see block
05). Immersive audio, auto play/pause and auto answer are not tested.

### SoundLink Color II

Product `400d`, firmware 4.0.1. Verified on two speakers with a raw
RFCOMM client that reads every byte of each reply.

**The speaker powers off after its 9th RFCOMM connection.** Counting
from power-on, a few seconds after the 9th connection to channel 8 is
closed the speaker beeps and switches itself off; it has to be put on
charge to power on again. Reproduced four times. It does not depend on
what is sent (read-only battery queries are enough), on the spacing
between connections (7 s and 30 s both fail at the 9th), or on the
Bluetooth link: disconnecting and reconnecting the speaker does not
reset the count, only powering it off does. A single connection kept
open for minutes with many requests is fine.

BlueZ's own RFCOMM connections do not count. After a fresh
power-on with the phone's Bluetooth off, BlueZ opened 13 Bluetooth
links and 24 RFCOMM channels of its own (hands-free profile and the
like) and the speaker stayed on (2026-10-29). After a disconnect the
speaker may not answer a new connection (Page Timeout) for 10 to 30 s.
The phone's Bose app connections do count. After a fresh power-on,
the app was opened and closed 6 times; the laptop's second control
connection worked, and the speaker was found switched off before the
third one could open (2026-11-02). That also explains a speaker that
switched off after only two CLI connections.

With a 3.5 mm cable plugged in, the speaker switches Bluetooth off
completely (its Bluetooth light goes off and connections fail with
`EHOSTDOWN`), so nothing can be read over RFCOMM in that mode.

Every CLI invocation opens and closes one connection, so on this speaker
a handful of commands in a row is enough to switch it off. Long-lived
clients (the GUI) should keep one connection open.

Media keys work; volume scale 100 with 99 the highest accepted level;
the volume reply comes twice; the paired-device replies differ from
the C acknowledgements (see block 04). Keeps at most two connections.
Pairing mode works (see block 04).

### NC 700 Headphones

Product `4024`, firmware 1.8.2-11524+e0f7590 (2026-11-04). Control
channel RFCOMM 8. The Bose Music app also talks BMAP to it over BLE;
that is not implemented here.

Every setting below was read from a laptop and compared with the Bose
Music app's screens. Each write was read back from the device and the
setting then put back. Noise cancelling, the favourites, the shortcut,
the equalizer, the volume and self voice were also checked by ear or in
the Bose Music app after a write from the laptop. The remember-last-setting
and motion auto-off switches, the voice prompts and the auto-off were
confirmed by read-back only.

* **Settings:** noise cancelling in steps (`01 05`), remember last
  setting (`01 0e`), favourites (`01 0f`), motion auto-off (`01 14`),
  equalizer (`01 07`, bass, mid and treble in `-10..=10`, same layout as
  the QC Ultra), self voice (`01 0b`, same reply `01 02 0f`), auto-off
  (`01 04`, 1-byte layout: 20 minutes and never were written; the app
  lists 5, 20, 40, 60, 180 minutes and never), voice prompts (`01 03`),
  volume (`05 05`, scale 16: `05 05 03 02 10 0a` is level 10 of 16;
  levels 9 and 10 were written), shortcut (`01 09`) and the paired list
  (`04 04`, read only).
* **Shortcut:** the reply is `01 09 03 0b 80 05 07 00 01 20 88 00 00 00 80`:
  button `80`, **event `05`** (the QC Ultra and QC45 use `09`) and the
  action `07`, the wake-word toggle. The action numbers are the same as
  on the QC models: `03` battery level, `10` Spotify, `0e` off, `01`
  voice assistant. `01 09 02 03 80 05 xx` was verified for battery,
  Spotify and the wake word.
* **The device does not answer some writes:** `01 04` (auto-off), `01 05`,
  `01 0e` and `01 0f` get no reply to a write, so the library reads the
  value back. `01 14` was echoed in the capture.
* **Voice prompts:** the payload is 5 bytes (`e1 00 01 81 5e`). Writing
  `01 03 02 01 xx` changes the language byte; the device stores it
  without the unknown flag in bit 6 (`e1` is read back as `a1`), so the
  confirmation compares the language and prompts bits only.
* **Not supported (ERROR `04` or `03`):** `01 06` (noise-cancelling
  levels of the QC35), `01 10` (on-head detection), `01 13` (LED
  brightness), `01 18` (auto play/pause), and block `1f` (audio modes).
* **Prompt language:** `--prompt-language fr` and back to `en` were
  written and read back (`a1` to `a2` to `a1`). The last two bytes of the
  reply, `81 5e`, look like a bit set of the supported languages, one bit
  per language code: English, French, Italian, German, Spanish, Chinese
  and Japanese.
* **Paired devices:** the list byte of `04 04` has one bit per listed
  device: `03` is both connected, `02` is only the second one (the phone
  was disconnected). A connect (`04 01 05 07 00 <address>`) is
  acknowledged with `04 01 07 06 <address>` (PROCESSING) and nothing
  else for at least 8 s. A disconnect (`04 02 05 06 <address>`) gets no
  reply at all. Both take effect, so the library accepts a PROCESSING
  without a RESULT and the CLI then checks the connection state. A phone
  usually reconnects by itself within seconds of a disconnect. The
  phone's Bose app holds the control channel when it is open: with the
  app running, the laptop gets "busy" or "connection refused".
* **Pairing mode (`04 08`):** `04 08 05 01 01` turns it on (the headphones
  say "ready to connect") and `04 08 05 01 00` turns it off. The write is
  answered with PROCESSING (`04 08 07 00`) or not at all, so the library
  reads `04 08 01 00` back: the reply is `04 08 06 02 <flag> <other>`, and
  the **first byte is the pairing flag**, `01` while pairing and `00`
  otherwise. The second byte changed between `00`, `01` and `03` and is
  not decoded. Mode stayed on for over a minute until it was turned off,
  and the off packet cleared the flag within 3 s. On the first try the
  headphones reset the control connection and dropped the laptop's
  Bluetooth link, which had to be reconnected with `bluetoothctl
  connect`; later runs did not.
* **Multipoint (`01 0a`):** reads `03`. The app's decoder takes bit 0 as
  enabled, bit 1 as supported and bit 2 as "disable supported", so
  multipoint is on and cannot be switched off on this model; nothing was
  written.
* **Remove a paired device (`04 03 05 06 <address>`):** removed a Pixel 7a
  from a list of three. The device sent no RESULT and no PROCESSING for the
  address (the call waited its full 15 s), but the list read back with two
  devices. The CLI therefore checks the list after a timeout instead of
  retrying.
