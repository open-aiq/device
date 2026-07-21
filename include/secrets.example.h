#pragma once

// Template for device secrets.
// Copy this file to secrets.h and fill in your own values.
// secrets.h is gitignored so your real credentials stay out of the repo.
//
// These values are used only when the development-only MOCK_APP_PROVISIONING
// flag in config.h is enabled. First boot then seeds the device config from
// here exactly as the app would over BLE.

#define WIFI_SSID     "your-wifi-ssid"
#define WIFI_PASSWORD "your-wifi-password"

// Backend device credentials — create a device via POST /api/v1/devices (or
// the dashboard) and paste the returned device_id and one-time device_key.
#define DEVICE_ID  "dev_..."
#define DEVICE_KEY "sk_..."

// Location the mobile app would have reported (0.0/0.0 = no location; the
// upload then omits the location block).
#define MOCK_LAT 0.0
#define MOCK_LON 0.0
