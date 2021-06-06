# Bose® Connect App for Linux

--- Not Official App ---

Based on [Denton-L project][Denton-L], looks like it is not maintained. I
created a copy to have an active repository. This project keeps the original
license GPL-3.0.

If you own a Bose device, you'll know that `Bose Connect` is not available on
Linux. This program attempts to reverse engineer that app to give the device
Linux support.

### Usage

```text
Usage: bose-connect-app-linux [options] <address>
  # address: The Bluetooth address of the Bose's device.

  -h, --help
    Print the help message.
  -i, --info
    Print all the device information.
  -d, --device-status
    Print the device status information. This includes its name, language,
    voice-prompts, auto-off and noise cancelling settings.
  -f, --firmware-version
    Print the firmware version on the device.
  -s, --serial-number
    Print the serial number of the device.
  -b, --battery-level
    Print the battery level of the device as a percent.
  -a, --paired-devices
    Print the devices currently connected to the device.
    !: indicates the current device
    *: indicates other connected devices
  --device-id
    Print the device id followed by the index revision.
  -n <name>, --name=<name>
    Change the name of the device.
  -o <minutes>, --auto-off=<minutes>
    Change the auto-off time.
    minutes: never, 5, 20, 40, 60, 180
  -c <level>, --noise-cancelling=<level>
    Change the noise cancelling level.
    level: high, low, off
  -l <language>, --prompt-language=<language>
    Change the voice-prompt language.
    language: en, fr, it, de, es, pt, zh, ko, nl, ja, sv
  -v <switch>, --voice-prompts=<switch>
    Change whether voice-prompts are on or off.
    switch: on, off
  -p <status>, --pairing=<status>
    Change whether the device is pairing.
    status: on, off
  -e, --self-voice=<level>
    Change the self voice level.
    level: high, medium, low, off
  --connect-device=<address>
    Attempt to connect to the device at address.
  --disconnect-device=<address>
    Disconnect the device at address.
  --remove-device=<address>
    Remove the device at address from the pairing list.
```

## Build and Installation

The executable produced will be called `./src/build/bose-connect-app-linux`.

### Dependencies

* BlueZ
    * `bluez-libs` on Arch Linux
    * `libbluetooth-dev` on Debian and Ubuntu

### Docker

Follow the next steps:

```bash
# Clean previous docker composes
docker-compose --project-directory ./src down

# Start the docker compose
export USER_ID=$(id -u ${USER})
export GROUP_ID=$(id -g ${USER})
docker-compose --project-directory ./src up --detach --build

# Build the application
docker exec \
  --user ${USER_ID} \
  -it bose-connect-app-linux \
   /root/bose-connect-app-linux/script/build-prod.bash

# Enjoy
./src/build/bose-connect-app-linux
```

*Note: I created this in `Arch Linux`. It should be crash in `Ubuntu` because
the library of bluetooth is different, if it fails some fixes will come soon.*

### Local

The local build require the installation of the follow packages: `gcc`, `make`,
`cmake`, `pkgconf`, and (`bluez-libs` or `libbluetooth-dev`).

```bash
# Execute the Bash script
./src/script/build-prod.bash

# Enjoy
./src/build/bose-connect-app-linux
```

### Install

Run `./src/script/install-prod.bash` to install the program, it will place in
the directory `/usr/local/bin/bose-connect-app-linux`. The
`PREFIX` and `DESTDIR` variables are assignable and have the traditional
meaning. For more information reefer to the [official web site][cmake-install].

Uninstall needs to run the script `./src/script/uninstall.bash`.

## Contribute

Check the file [CONTRIBUTING.md][contributing] for more information. It
includes the instructions for build with special configuration for development.

## To-Do's List

Visit the document with all the checkpoints in [TODO.md][todo.md].

## Development Notes

For more information about the details of how use the firmwares to found
functionality, please review the file [DEVELOPMENT.md][details-file].

## Disclaimer

This has only been tested on Bose `QuietComfort 35's` with firmware 1.3.2,
1.2.9, 1.06 and `SoundLink II's` with firmware 2.1.1. I cannot ensure that this
program works on any other devices.


[Denton-L]: https://github.com/Denton-L/based-connect

[details-file]: ./DEVELOPMENT.md

[todo.md]: ./TODO.md

[contributing]: https://github.com/airvzxf/bose-connect-app-linux/blob/main/DEVELOPMENT.md

[cmake-install]: https://cmake.org/cmake/help/latest/manual/cmake.1.html#install-a-project
