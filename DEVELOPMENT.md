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

Protocol
--------

The packet-level notes that used to live here (media keys, volume, the
SoundLink Color II and QuietComfort Ultra captures) are now in
[`docs/protocol.md`](docs/protocol.md), organised by command, with the
device and firmware of each observation and the Rust call that sends it.
