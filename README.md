# Device Firmware — Air Quality Monitor

ESP32 firmware for an air-quality monitor that reads particulate matter, temperature,
and humidity, drives a 16×2 I2C LCD, handles four push buttons via interrupts, and
reports over WiFi.

## Hardware

- **MCU:** ESP32 (DOIT ESP32 DevKit v1)
- **PM sensor:** Plantower PMS5003 (UART, on `Serial2`)
- **Temp/Humidity:** DHT22
- **Display:** 16×2 character LCD over I2C (PCF8574 backpack, address `0x3F`)
- **Input:** 4 push buttons (Right / Left / Settings / Boot)

## Pin map

| Function            | GPIO        | Notes                          |
|---------------------|-------------|--------------------------------|
| I2C SDA / SCL       | 21 / 22     | LCD                            |
| PMS5003 RX / TX     | 16 / 17     | `Serial2`, 9600 baud           |
| DHT22 data          | 14          |                                |
| Button: Right       | 34          | input-only pin, FALLING edge   |
| Button: Left        | 35          | input-only pin, FALLING edge   |
| Button: Settings    | 36          | input-only pin, FALLING edge   |
| Button: Boot        | 39          | input-only pin, FALLING edge   |

> Note: GPIO 34/35/36/39 are input-only and have **no internal pull-ups** — make sure
> each button has an external pull-up resistor.

## Project layout

```
.
├── platformio.ini          # build config, board, library deps
├── src/main.cpp            # firmware source
├── include/
│   ├── secrets.h           # WiFi credentials (gitignored — create from template)
│   └── secrets.example.h   # template to copy
└── README.md
```

## Setup

This is a [PlatformIO](https://platformio.org/) project.

1. Copy the secrets template and fill in your WiFi details:

   ```bash
   cp include/secrets.example.h include/secrets.h
   # then edit include/secrets.h
   ```

2. Build:

   ```bash
   pio run
   ```

3. Upload to the board:

   ```bash
   pio run -t upload
   ```

4. Open the serial monitor (9600 baud):

   ```bash
   pio device monitor
   ```

## Configuration

- WiFi credentials live in `include/secrets.h` (`WIFI_SSID`, `WIFI_PASSWORD`).
- Board, monitor speed, and library dependencies are set in `platformio.ini`.

## Build & release (Makefile)

A `Makefile` wraps the common PlatformIO and release steps. The version and
release notes are **inputs you pass on the command line** — they are not baked
into the Makefile.

| Target  | What it does                                                            |
|---------|-------------------------------------------------------------------------|
| `build` | Compile the firmware (`pio run`).                                        |
| `merge` | Build, then merge bootloader + partitions + app into one flashable bin. |
| `tag`   | Create and push a git tag (`VERSION` required).                         |
| `release` | Build, merge, tag, and publish a GitHub release with the artifacts.   |
| `clean` | Remove build artifacts.                                                 |

### Release inputs

| Variable     | Default            | Purpose                                                  |
|--------------|--------------------|----------------------------------------------------------|
| `VERSION`    | _(required)_       | Release/tag name, e.g. `v0.1.0`. No default — must be set.|
| `NOTES_FILE` | `RELEASE_NOTES.md` | File whose contents become the release notes.            |
| `NOTES`      | _(unset)_          | Inline release notes; overrides `NOTES_FILE` if given.   |
| `PRERELEASE` | `true`             | Mark the GitHub release as a pre-release. Set `false` for a full release. |

Inputs are validated up front (`check-release-inputs`), so a missing version or
notes file fails immediately — before any git tag is pushed.

### Examples

```bash
# Just build
make build

# Cut a pre-release, notes read from RELEASE_NOTES.md
make release VERSION=v0.1.0

# Notes from a specific file
make release VERSION=v0.1.0 NOTES_FILE=notes/v0.1.0.md

# Inline notes
make release VERSION=v0.1.0 NOTES="Fixed BLE provisioning"

# Full (non-pre) release
make release VERSION=v1.0.0 PRERELEASE=false
```

## Known issues / TODO

- [ ] **Fix the HTTPClient URL-parser bug.** The query string is currently only
  reachable by adding a `/` before `?` in the request URL, as a workaround for an
  upstream parser bug. Track down and fix the root cause (or upstream a patch).
  See [docs/upstream-bug-httpclient-url-parser.md](docs/upstream-bug-httpclient-url-parser.md).
- [ ] **Reduce firmware size.** Adding BLE (Bluedroid) on top of WiFi + TLS pushed
  the binary past the default 1.3 MB app partition, so the build now uses the
  `huge_app.csv` partition scheme (3 MB app, no OTA). Investigate slimming the
  build (e.g. NimBLE instead of Bluedroid) to recover headroom and re-enable OTA.
