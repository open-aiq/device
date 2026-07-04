# AQS Platform — Product Plan

## 1. Product

An open-source, community air-quality monitoring platform. Users run one or a few
ESP32 monitors, see their own data privately, and can opt any device into a public
community dashboard (live map + list, indoor/outdoor tagged). Self-hostable; small
team; MIT-licensed.

## 2. Tech decisions

| Tier    | Stack |
|---------|-------|
| Device  | ESP32 + PMS5003/DHT22, MQTT over TLS to Mosquitto, sensor fan duty-cycling, LCD-only local display, NTP + offline buffer |
| Backend | Go, Mosquitto broker, plain Postgres, Clerk auth (behind a swappable interface), REST API (clients poll), managed PaaS for our instance + Docker Compose for self-host |
| Web     | Vite + React SPA, Leaflet + OpenStreetMap map |
| App     | Flutter, BLE provisioning + viewing, FCM push |
| Repos   | `aqs-device` (exists), `aqs-backend`, `aqs-web`, `aqs-app` (separate, MIT) |
| Board   | org `AQS Device Tracking` (public); Epic custom field; tier labels `device`/`backend`/`web`/`app` |

Telemetry decision: MQTT, because the MCU stays awake with a continuous WiFi
connection (only the sensors are power-cycled to extend PMS5003 life). A persistent
connection amortizes the TLS handshake, enables push-based remote control, and gives
accurate offline detection via LWT.

## 3. Cross-tier contracts

MQTT topics:

```
aqs/<device_id>/telemetry   device -> backend, QoS 1
aqs/<device_id>/status      device -> backend, LWT "offline"
aqs/<device_id>/cmd         backend -> device, remote control
```

Telemetry payload:

```json
{ "device_id": "aqs-abc123", "ts": 1718450000,
  "pm25": 12.0, "pm10": 18.0, "temp": 24.5, "hum": 55.0,
  "aqi": 51, "fw": "0.1.0" }
```

Device auth: per-device token issued at provisioning, used as MQTT username/password.
Client API auth: Clerk JWT over REST.

## 4. Vertical slices (epics)

Order = build order. Each epic ships and demos on its own.

### Epic 0 — Foundations

- device: sensor fan duty-cycling (spin-up, 30s warm-up, read, power down), NTP time sync
- backend: Go skeleton, docker-compose (Go + Mosquitto + Postgres), DB schema (`users`, `devices`, `readings`), CI, MIT
- web: Vite + React scaffold, routing, CI, MIT
- app: Flutter scaffold, CI, MIT
- Demo: `docker-compose up` brings the stack online; all repos build in CI.

### Epic 1 — Accounts & ownership

- backend: Clerk behind `AuthProvider` interface; `users`/`devices` ownership; protected routes
- web: login/signup, authed shell
- app: login/signup
- Demo: a user logs in on web and app and sees an empty device list.

### Epic 2 — Device provisioning

- device: reuse BLE `SSID:`/`PASS:`/`SAVE` flow; add claim-token exchange + device identity
- backend: `POST /devices` (register, issue device token + MQTT creds), claim-to-user
- app: BLE onboarding flow — scan, connect, send WiFi + claim, device appears under account
- web: device appears in list (rename, set location lat/lon, indoor/outdoor)
- Demo: app provisions a device over BLE; it shows up in the account.

### Epic 3 — Telemetry pipeline (spine)

- device: publish reading to `aqs/<id>/telemetry` over MQTT/TLS; on-device AQI calc; offline ring buffer replays on reconnect; LWT status
- backend: MQTT ingestion service -> validate -> write `readings`; `GET /devices/{id}/latest`
- web: live device dashboard (AQI + PM/temp/hum), polling
- app: live readings + AQI
- Demo: a real sensor reading appears on web and app within one poll interval.

### Epic 4 — History & charts

- backend: `GET /devices/{id}/readings?from&to&bucket` (time-bucketed SQL aggregation); retention/downsample job
- web: historical charts with range picker (24h / 7d / 30d)
- app: historical charts
- Demo: scrub the last 7 days of PM2.5 on web and app.

### Epic 5 — Public dashboard & privacy

- backend: per-device `visibility` (public/private) + indoor/outdoor; `GET /public/devices` + `GET /public/devices/{id}` (no auth, public only)
- web: public map (Leaflet + OSM) with AQI-colored indoor/outdoor pins + list; public device detail page; owner public/private toggle
- app: privacy toggle per device; optional browse public map
- Demo: flipping a device to public makes it appear on the public map; private removes it.

### Epic 6 — Alerts & notifications

- device: local threshold -> LCD warning + AQI color (no buzzer)
- backend: alert rules (threshold cross) -> FCM push; per-user alert prefs
- web: threshold/alert configuration UI
- app: FCM push notifications + alert settings
- Demo: PM2.5 crossing a threshold triggers phone push, LCD warning, and web banner.

### Epic 7 — Remote control

- device: subscribe `aqs/<id>/cmd` (set report interval, set thresholds, reboot, force-read)
- backend: `POST /devices/{id}/commands` -> publish to `cmd` topic
- web/app: device controls UI
- Demo: changing report interval from the web is obeyed by the device within seconds.

### Backlog (post-MVP)

OTA (needs NimBLE + partition rework to regain an OTA slot); buzzer hardware; extra
sensors (CO2/VOC); SSR/prerender for public pages (SEO/link previews); device
sharing/teams; fleets/grouping; data export (CSV); billing.

## 5. Milestones

- M1 — Spine alive: Epics 0, 1, 2, 3. Provision a device and see live data.
- M2 — Useful: Epics 4, 5. History plus the public community dashboard.
- M3 — Smart: Epics 6, 7. Alerts plus remote control.
