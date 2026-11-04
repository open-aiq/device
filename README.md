# AirMonitor Rust firmware

This ESP32-S3 Rust experiment lives on the `rust` branch of the device
repository. The Arduino firmware for the ESP32 DOIT DevKit v1 remains on
`main`.

Rust/ESP-IDF firmware for the ESP32-S3 AirMonitor. It reads a PMS5003 and SHT30,
renders three screens on a 128x64 I2C OLED, provisions over the Nordic UART BLE
service, persists configuration in NVS, and uploads ten-minute averages.

## Wiring

[`src/board.rs`](src/board.rs) is the source of truth for GPIO selection and
hardware initialization: shared I2C at 400 kHz, PMS5003 UART at 9,600 baud,
and active-low buttons with internal pull-ups.

| Device | Signal | ESP32-S3 GPIO |
| --- | --- | --- |
| OLED + SHT30 | SDA | 8 |
| OLED + SHT30 | SCL | 9 |
| PMS5003 | sensor TX -> ESP RX | 18 |
| PMS5003 | sensor RX <- ESP TX | 17 |
| Button | Right, active-low | 4 |
| Button | Left, active-low | 5 |

Power the OLED and SHT30 at 3.3 V and ensure the shared I2C bus has pull-ups.
The PMS5003 normally uses 5 V power; verify the module's UART logic levels.

At boot the firmware accepts an SHT30 at `0x44`/`0x45` and OLED at
`0x3C`/`0x3D`. Missing or ambiguous devices are logged without stopping other
services. Controller type cannot be inferred from its address.

## Build and quality gates

The default controller is SH1106:

```sh
OLED_CONTROLLER=sh1106 cargo build --release
OLED_CONTROLLER=ssd1306 cargo build --release
make format
make verify HOST_TARGET=aarch64-apple-darwin
```

Use `HOST_TARGET=x86_64-unknown-linux-gnu` on Linux. `cargo fmt` rewrites source;
CI uses `cargo fmt -- --check`. Clippy uses `-D warnings`. Host tests cover the
portable library and separate ESP32-S3 jobs compile both OLED selections.

Flash only when hardware is attached:

```sh
OLED_CONTROLLER=sh1106 cargo run --release
```

## Provisioning

The BLE device is `AirMonitor-XXXX`. Its Nordic UART service accepts:

```text
SSID:<name>  PASS:<secret>  LAT:<deg>  LON:<deg>
DEVID:<dev_...>  DEVKEY:<sk_...>
SAVE  LOAD  CLEAR  PING  STATUS
```

Commands and responses are newline-terminated; notifications remain at most 20
bytes and receive commands are limited to 512 bytes. Secrets are never echoed.

An empty NVS can be seeded in a development build without committing secrets:

```sh
MOCK_WIFI_SSID=... MOCK_WIFI_PASSWORD=... \
MOCK_DEVICE_ID=dev_... MOCK_DEVICE_KEY=sk_... \
MOCK_LAT=24.8607 MOCK_LON=67.0011 cargo build
```

Set `WIPE_CONFIG_ON_BOOT=1` only for a deliberate reset build. Mock values are
compiled into that binary and must not be distributed. `BACKEND_BASE_URL`
defaults to `https://api.air-iq.net/api/v1`. HTTPS uses ESP-IDF's embedded CA
bundle.

## Manual hardware acceptance

For each OLED controller build:

1. Confirm the expected OLED and SHT30 addresses in the boot log.
2. Confirm PMS5003, temperature, and humidity samples every 20 seconds.
3. Confirm buttons cycle AQI, climate, and status screens without bounce.
4. Exercise `PING`, fragmented commands, `LOAD`, `SAVE`, and `CLEAR` over BLE.
5. Verify bad WiFi credentials leave BLE available, then provision valid ones.
6. Verify an authenticated upload returns HTTP 201 and survives a reboot.
7. Disconnect each sensor in turn and verify other services keep running.
