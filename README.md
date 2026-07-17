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
| Button: Settings    | 36          | **disabled** (see note below)  |
| Button: Boot        | 39          | **disabled** (see note below)  |

> Note: GPIO 34/35/36/39 are input-only and have **no internal pull-ups** — make sure
> each button has an external pull-up resistor.

> **Settings/Boot are disabled in firmware:** GPIO36/39 are RTC-domain pins and an
> ESP32 erratum makes them glitch on every WiFi modem-sleep wake, firing phantom
> interrupts. WiFi power-save was kept instead of these buttons; rewire them to
> non-RTC pins (and re-enable in `src/buttons.cpp`) to bring them back.

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

1. Copy the secrets template and fill in your WiFi details and device
   credentials (register a device against the backend to get `DEVICE_ID` and
   the one-time `DEVICE_KEY`):

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

## Configuration & provisioning

- Secrets live in `include/secrets.h`: `WIFI_SSID`, `WIFI_PASSWORD`,
  `DEVICE_ID`, `DEVICE_KEY`, `MOCK_LAT`, `MOCK_LON`.
- Board, monitor speed, and library dependencies are set in `platformio.ini`.
- The backend base URL is `BACKEND_BASE_URL` in `include/config.h`
  (default `https://backend-h5v6.onrender.com/api/v1`; override with a
  `-DBACKEND_BASE_URL=...` build flag).

### Provisioning (app vs. mock)

The device is designed to be provisioned by a mobile app over BLE (Nordic UART
service, advertised as `AirMonitor-Setup`) with line commands:

```
SSID:<name>  PASS:<secret>  LAT:<deg>  LON:<deg>  DEVID:<dev_...>  DEVKEY:<sk_...>
SAVE   LOAD   CLEAR   PING   STATUS
```

Until the app exists, **mock app provisioning** stands in for it:
`MOCK_APP_PROVISIONING` (default `1`, `include/config.h`) seeds the config from
`secrets.h` on first boot — exactly the values the app would have sent — and
saves them to flash. BLE provisioning stays fully functional and overwrites the
mock values at any time. Set the flag to `0` once the real app ships.

Mock provisioning only fires when flash is empty. After changing `secrets.h`,
either send `CLEAR` over BLE or build once with `WIPE_CONFIG_ON_BOOT` set to
`1` (`include/config.h`) — it wipes the saved config on every boot so the
re-seed happens. Set it back to `0` afterwards, or the device forgets BLE
provisioning on every restart.

## Telemetry

Sensors are sampled every **20 seconds**; the display always shows the latest
sample. Every **10 minutes** (first upload right after boot) the firmware
averages the window's samples (~30 per upload; AQI recomputed from the averaged
concentrations) and POSTs the result to `{BACKEND_BASE_URL}/data` with
`X-Device-ID` / `X-Device-Key` headers:

```json
{
  "pms_data":         { "pm1_0": 5, "pm2_5": 12, "pm10_0": 18, "provider": "pms5003" },
  "aqi": 57,
  "temperature_data": { "temperature": 31.2, "humidity": 64.5, "heat_index": 36.8, "provider": "dht22" },
  "location":         { "lat": 24.8607, "lon": 67.0011, "provider": "mobile" }
}
```

`location` is omitted while lat/lon are unset (0/0). Uploads are skipped (and
logged) when WiFi is down, credentials are missing, or the reading is
incomplete; the next tick retries. TLS currently uses `setInsecure()` — cert
pinning is on the roadmap.

## Display & buttons

Three screens on the 16×2 LCD, refreshed with each 20 s sensor sample using
fixed-width padded row writes (no flicker, no stale characters):

1. **AQI** (home) — `AQI 54 Moderate` / `PM2.5 25ug/m3`
2. **Climate** — `T 29.1C HI 33.5C` / `Humidity 61%`
3. **Status** — `WiFi <ip>` or `WiFi DOWN` / `Up 201 3m ago`

Buttons: **Right/Left** cycle through all screens. (Settings/Boot are disabled
— see the pin-map note; a real settings menu is on the roadmap.)

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
