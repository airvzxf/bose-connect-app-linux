Project Details
===============

Firmware Updates
----------------

Here are some details on where firmware lookup details can be found:

- Staging firmware lists: https://downloads-test.bose.com/lookup.xml
- Beta firmware lists: https://downloads-beta.bose.com/lookup.xml
- Production firmware lists: https://downloads.bose.com/lookup.xml

At this point, lookup.xml can then be used to find index.xml files
corresponding to your product. The index.xml files should give *.dfu and *.xuv
files for each version and revision.

Unfortunately, I don't know much about what the purpose of what each file does,
nor do I know how to properly use them to upgrade the device.

I do have some sniffed packets of the upgrade process but, unfortunately, I
started sniffing after my headphones were connected, so they didn't capture the
headphone's address. As a result, Wireshark doesn't recognise any of the
packets as SPP packets. Instead, it sees them as L2CAP packets where the 'Frame
is out of any "connection handle" session'. If someone could help me fix this
or send in their own sniffed packets, that would be great.

Media Related
-------------

`>` means packet sent and `<` means packet received

I currently have the following details about media keys, but I cannot seem to
get them to work. I believe that you need to send a packet or somehow confirm
to the headphones that you wish to control a particular device. I haven't
discovered a packet that makes that does this yet, though.

Media Keys:

```text
# Send play key to connected device (02 may mean pause; currently unsure)
> 05 03 05 01 xx xx = 01
xx = 03 # Send next key
xx = 04 # Send previous key
```

Volume:

```text
> 05 05 02 01 xx 00 <= xx <= 18
# Where xx is the volume
```

It also appears that each device has a unique volume associated with it.

Active Device:

```text
> 05 01 01 00
< 05 01 03 09 00 02 01 xx xx xx xx xx xx
# xx xx xx xx xx xx is the address of the current active device
```

Currently, unsure if any other bytes may or may not vary other than the
address.

Get Music Status:

```text
> 05 02 05 00
< 05 02 07 00 05 03 03 02 01 fe 05 04 03 03 xx yy yy
  05 05 03 02 19 0b zz 06 07 00 xx = 02
# Paused xx = 01
# Playing xx = 00
# Unknown yyyy
# Elapsed time of music (may be ffff for unknown?)
# 00 <= zz <= 18: volume
```

```text
< 05 02 04 01 0c
# Sent if unknown music
```

QuietComfort Ultra Headphones
-----------------------------

Captured on firmware 1.6.7 (device id `0x4066`). `>` means packet sent and
`<` means packet received.

The protocol is the same, but it listens on RFCOMM channel 2 (SDP service
"SPPS De", channels 1 and 2 both answer), not 8. The device refuses a new
connection for about a second after the previous one closed.

Replies use the header `block function operator length`; the operators seen
are `03` (status), `04` (error, 1-byte error code), `06` (result) and `07`
(processing). Several replies are longer than on the QC35:

```text
> 00 05 01 00                       # Firmware version
< 00 05 03 0e 31 2e 36 2e 37 2b 67 36 65 62 61 62 64 32
# "1.6.7+g6ebabd2"

> 02 02 01 00                       # Battery
< 02 02 03 04 64 ff ff 00
# 0x64 = 100 %, the last 3 bytes are unknown

> 01 03 01 00                       # Prompt language
< 01 03 03 07 e1 00 01 81 5e 01 01
# Low 5 bits of the first byte = language, bit 5 = voice prompts on.
# The meaning of bit 6 and of the other bytes is unknown.

> 01 04 01 00                       # Auto-off
< 01 04 03 03 a0 00 05
# Unknown layout (the QC35 sends 1 byte of minutes)

> 01 07 01 00                       # Equalizer, 3 bands
< 01 07 03 0c f6 0a 00 00 f6 0a 00 01 f6 0a 00 02
# Per band: min (-10), max (10), current value, band (0 bass, 1 mid, 2 treble)

> 01 0b 01 00                       # Self voice, same layout as the QC35
< 01 0b 03 03 01 02 0f
```

The status dump (`01 01 05 00`) sends `01 00`, `01 02`, `01 03`, `01 04`,
`01 05`, `01 07`, `01 09`, `01 0a`, `01 0b`, `01 0c`, `01 18` and `01 1b`
between `01 01 07 00` and `01 01 06 00`. There is no `01 06`
(noise cancelling): it is replaced by the audio modes of block `1f`.

Audio modes:

```text
> 1f 03 01 00                       # Current audio mode
< 1f 03 03 01 xx
# xx = slot index

> 1f 06 01 01 xx                    # Audio mode of slot xx
< 1f 06 03 2f xx 00 01 00 00 01 51 75 69 65 74 00 ...
# 47-byte payload: slot index, 5 bytes of settings, the name NUL-padded
# to 32 bytes ("Quiet"), then 9 more bytes of settings.
# Slots 0, 1, 2 are Quiet, Aware, Immersion; slots 3 to 9 are named "None".
< 1f 06 04 01 08                    # Error for slot 10 and above

> 1f 03 05 02 xx 00                 # Switch to the audio mode of slot xx
< 1f 03 06 01 xx
```
