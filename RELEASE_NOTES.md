# v0.0.0-internal.1 — Pre-alpha internal testing

⚠️ **Not for production.** This is an internal testing build — earlier than alpha.
No stability, API, or OTA guarantees. Expect breaking changes without notice.

## What's in this build

- **Sensors:** PMS5003 (particulate matter), DHT22 (temperature/humidity), I2C LCD display.
- **Input:** physical button interrupts with debounce.
- **Networking:** WiFi with local-network communication; public-IP lookup over HTTPS (TLS).
- **Bluetooth (BLE):** remote control — query WiFi status (tested with the BlueFruit app).
- **Persistent config (NVS):** WiFi credentials + device location stored in flash and
  provisioned over BLE (`SSID:` / `PASS:` / `LAT:` / `LON:` then `SAVE`, plus `LOAD` / `CLEAR`).
  Settings survive reboots. WiFi connect is non-blocking with a timeout, so a wrong password
  no longer freezes the device — it stays reachable over BLE for re-provisioning.

## Hardware / build notes

- Target board: `esp32doit-devkit-v1` (classic ESP32).
- Partition layout: `huge_app.csv` (3 MB app, **no OTA**) — BLE + WiFi + TLS overflow the
  default 1.3 MB app partition.
- HTTPClient URL-parser workaround documented in
  [`docs/upstream-bug-httpclient-url-parser.md`](docs/upstream-bug-httpclient-url-parser.md).

## Flashing

Flash the individual images at their offsets, or use the single combined image:

- `merged-firmware.bin` — write to offset `0x0`:
  ```
  esptool.py --chip esp32 write_flash 0x0 merged-firmware.bin
  ```
- Or the separate parts: `bootloader.bin` @ `0x1000`, `partitions.bin` @ `0x8000`,
  `firmware.bin` @ `0x10000`.

## Known issues / TODO

- HTTPClient URL-parser bug still worked around (the `/` before `?`), not root-caused.
- No OTA, and limited app headroom — investigate NimBLE instead of Bluedroid to slim the build.
