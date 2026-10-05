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

SoundLink Color II findings
---------------------------

Verified on two SoundLink Color II speakers (firmware 4.0.1) with a raw
RFCOMM client that reads *every* byte of each reply.

**The speaker powers off after its 9th RFCOMM connection.** Counting from
power-on, a few seconds after the 9th connection to channel 8 is closed
the speaker beeps and switches itself off; it has to be put on charge to
power on again. Reproduced four times. It does not depend on what is
sent (read-only battery queries are enough), on the spacing between
connections (7 s and 30 s both fail at the 9th), or on the Bluetooth
link: disconnecting and reconnecting the speaker does not reset the
count, only powering it off does. A single connection kept open for
minutes with many requests is fine.

Every CLI invocation opens and closes one connection, so on this speaker
a handful of commands in a row is enough to switch it off. Long-lived
clients (the GUI) should keep one connection open.

Media keys (`05 03 05 01 xx`): every key is answered with two packets,
`05 03 07 00` (processing) then `05 03 06 00` (result). `01` toggles
play/pause, `03` is next, `04` is previous (restarts the current track
when it has been playing for a few seconds, like a physical button).
`02` is acknowledged the same way but has no visible effect on the
player, whether it is playing or paused.

Volume (`05 05 02 01 xx`):

```text
> 05 05 02 01 14
< 05 05 03 02 64 14
# 64: number of volume steps (100, so the range is 00..63)
# 14: the volume now set
> 05 05 02 01 64
< 05 05 04 01 06
# ERROR packet (operator 04): out-of-range level
```

The `64` byte is not the battery level: it stays `64` whatever the level,
and `64` itself is the first value rejected. The QC35 value above
(`19`, 25 steps for a `00..18` range) fits the same reading.
