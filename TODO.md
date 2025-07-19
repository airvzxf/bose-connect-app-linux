# To-Do List

## Now

- [ ] Open WireShark and execute every operation to see the packets and the payloads. Test all the operations for the Bose QC35 II and the Bose SLC II.
- [ ] Move the enumerator `AutoOffValue` from `src/cli.rs` to the correct place in `src/bose_api/operations/device_status.rs`.
- [ ] Check if is the best practice to create a directory for `device.rs` and split the code in different files.

## Build

- [x] Update to the latest the versions of the packages in Cargo.toml. Change one by one and review if everything works.

## Core

- [ ] Remove condition to get the device firmware, using the name of the device.
- [ ] The firmware pattern is not working because not all devices has the same operations. Rethink the firmware pattern to get the device firmware.
  - [ ] An idea is to create the command with the name of the device and the subcommand with the operation to perform. On this way, we can have the same command for all devices and the operations will be different depending on the device.
  - [ ] The best example is the operation `get_device_status`, it returns different payloads depending on the device.
  - [ ] Check how to solve the problem about the version of the firmware, because we are not covering if the operations change at byte code level or CRUD the operations.
- [ ] Add the correct timeout functionality to the operations. Remove the timeout from the `get_device_status` operation.
- [ ] Create a pattern to create all the received byte codes for the operations. The idea is validated or compare to match with some of these, then get the operation based on this match. For example, it receives the buffer `[0x00, 0x03, 0x03, 0x03]` then I search, and it matches for `get_device_id_command`, for this reason it need to validate and read the values.

## Operations

- [x] Add connect operation to the CLI.
  - [x] Attempt to connect to the device at address.
  - [x] Disconnect the device at address.
  - [x] Remove the device at address from the pairing list.
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
