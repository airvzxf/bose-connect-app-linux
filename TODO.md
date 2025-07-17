# To-Do List

## Build

- [x] Update to the latest the versions of the packages in Cargo.toml. Change one by one and review if everything works.

## Core

- [ ] Remove condition to get the device firmware, using the name of the device.

## Operations

- [ ] Add connect operation to the CLI.
  - [ ] Attempt to connect to the device at address.
  - [ ] Disconnect the device at address.
  - [ ] Remove the device at address from the pairing list.
- [x] Add the set operations for the Bose device.
  - [x] Set device name.
  - [x] Set auto-off.
  - [x] Set noise-cancelling.
  - [x] Set prompt language.
  - [x] Set voice prompts.
  - [x] Set self voice.
  - [x] Set pairing.
- [ ] Set the share music mode.
- [ ] Set the function button.
- [ ] Set the party mode.
  - [ ] Get the party mode status.
  - [ ] Initialize the party mode.
  - [ ] Join device to the party mode.
  - [ ] Leave device from the party mode.
  - [ ] Exit from the party mode.

# Connection

- [ ] The Bose device sometimes does not connect properly. Add a retry mechanism to the connection process depending on the error code.

## Output

- [ ] Add a `--output` option to the `run` command to specify the output JSON file.
- [ ] Add CLI argument to output all the information for the device: Battery, DeviceStatus, Name, Language, AutoOff, NoiseCancelling, DeviceId, FirmwareVersion, SerialNumber, PairedDevices, and device information for each paired device: DeviceInformation.

## Reverse Engineering

- [ ] Analyze the firmware binary file to identify if it is encrypted or not.
- [ ] Analyze if we can unpack and decrypt the firmware binary file.
- [ ] Decrypt the firmware binary file if it is encrypted.
- [ ] Analyze the firmware binary file to identify the protocol used for communication.
- [ ] Analyze the firmware binary for the operations that can be performed on the paired devices.
  - [ ] Analyze the current operations in the source code `src/firmware/baywolf.rs` to search in the firmware binary file.
- [ ] Search for the party mode operation in the firmware binary file.
