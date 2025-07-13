# Information, Theories, and Facts

This document contains the foundational knowledge, assumptions, and technical details gathered for the Bose-Connect-App-Linux project.

## 1. Core Objective

The primary goal is to create a command-line interface (CLI) application in Rust that can communicate with Bose Bluetooth headphones. 

- **Target:** A Rust binary for Linux.
- **Input:** A physical Bluetooth address (e.g., `00:11:22:33:AA:BB`).
- **Output:** Structured data, likely JSON, which can be used by other applications or scripts.

## 2. Core Technology: Bluetooth in Rust on Linux

### The Stack

- **Linux:** The underlying operating system uses **BlueZ** as its official Bluetooth stack.
- **BlueZ D-Bus API:** BlueZ exposes its services to applications through a D-Bus interface, which is the standard way for applications to interact with Bluetooth on Linux.
- **Rust Crate: `bluer`:** We use the `bluer` crate, an `async`-native library that provides safe and idiomatic bindings to the BlueZ D-Bus API.

### Key Bluetooth Concepts (RFCOMM)

Communication with Bose headphones for control commands is done using the **Radio Frequency Communication (RFCOMM)** protocol, which emulates a serial port over Bluetooth.

- **RFCOMM Channel:** Each RFCOMM connection uses a specific channel. Based on reverse-engineering from other projects (e.g., `based-connect`), we know that **RFCOMM Channel 8** is used for control commands.

## 3. Communication Protocol

The communication protocol is a custom command-response system over RFCOMM, reverse-engineered from projects like `based-connect`.

1.  **Initial Handshake:** Before sending commands, a specific handshake is required.
    *   **Send:** `[0x00, 0x01, 0x01, 0x00]`
    *   **Receive ACK:** `[0x00, 0x01, 0x03, 0x05]`
    *   **Receive Payload:** Followed by 5 bytes. **Correction:** This is not the firmware version. It appears to be a static **protocol version** (`1.0.4`), which is consistent across different models.

2.  **Command-Response Flow:** After the handshake, commands are sent and responses are received.
    *   The application **writes** a `SEND` byte array.
    *   The headphones send back an `ACK` byte array.
    *   Finally, the headphones send the actual data payload.

**Example: Get Battery Level (Simple Response)**
- **Send:** `[0x02, 0x02, 0x01, 0x00]`
- **Receive ACK:** `[0x02, 0x02, 0x03, 0x01]`
- **Receive Payload:** A single byte representing the battery level.

**Example: Get Device Status (Multi-Part Response)**
The `get-device-status` command shows a more complex pattern where the device sends multiple, length-prefixed data chunks after the initial ACK.
- **Send:** `[0x01, 0x01, 0x05, 0x00]`
- **Receive ACK:** `[0x01, 0x01, 0x07, 0x00]`
- **Receive Payload Part 1 (Device Name):**
    - First, receive a header (e.g., 4 bytes) where the last byte indicates the length of the name.
    - Then, receive N bytes for the name itself.
- **Receive Payload Part 2 (Language):**
    - Receive another length-prefixed header.
    - Receive N bytes for the language data.
- ...and so on for other status fields like Auto-Off and Noise-Cancelling.

## 4. Assumptions and Risks

- **CRITICAL ASSUMPTION: RFCOMM Channel 8:** We are relying on the finding that RFCOMM Channel 8 is the correct channel. If this varies by model or firmware, the application may fail.
- **Assumption: BlueZ is Available:** The application assumes the `bluez` service is installed and running.
- **Assumption: Device is Paired:** The logic assumes the headphones are already paired and trusted by the OS. The app will focus on connecting to a known device, not discovery or pairing.
- **Assumption: `async` Runtime:** The `bluer` crate is asynchronous, so we will use the `tokio` runtime.

## 5. Bugs

- **Blocking the device**: Sometimes trying to connect to get the battery, the device turn off for the speakers and for headset it not response. Manually, I need to turn off and turn on again.
    - **Mitigation (Implemented):** To prevent the application from hanging indefinitely, all read/write operations now have a 5-second timeout. If the device does not respond within that window, the operation will fail with a `Timeout` error.

## 6. Research and review.

- **Device Model Identification**: The initial 5-byte payload after the handshake is a protocol version, not a firmware version. To correctly apply command byte-codes, the specific device model (e.g., `QC35`, `SoundLink`) must be identified. The strategy is to first attempt auto-detection using the Bluetooth Device ID Profile or the device's advertised name, with a manual `--model` CLI flag as a fallback.
- **Reverse Engineering**: To get the byte codes for commands, we will need to reverse-engineer the communication protocol. This may involve:
    - Using tools like Wireshark to capture Bluetooth traffic.
    - Analyzing existing open-source projects that interact with Bose devices (e.g., `based-connect`).
    - Consulting the BlueZ documentation for any relevant RFCOMM details.
