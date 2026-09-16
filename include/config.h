#pragma once

#include <Arduino.h>

// ---------------------------------------------------------------------------
// Persistent device configuration (ESP32 NVS / flash key-value store)
//
// Settings survive reboot/power loss. The live working copy lives in `config`;
// load/save/clear move it to and from flash.
// ---------------------------------------------------------------------------

// Base URL of the AIQ backend (no trailing slash). The path segment matters:
// HTTPClient's URL parser needs at least one '/' after the host (see
// docs/upstream-bug-httpclient-url-parser.md).
#ifndef BACKEND_BASE_URL
#define BACKEND_BASE_URL "https://api.air-iq.net/api/v1"
#endif

// All the settings we store, kept together in one place.
struct DeviceConfig
{
  String ssid;      // WiFi network name
  String password;  // WiFi password
  double lat;       // location latitude
  double lon;       // location longitude
  String deviceId;  // backend device identity ("dev_...")
  String deviceKey; // backend device secret ("sk_..."), sent on every upload
};

// The live working copy, held in RAM. Defined in config.cpp.
extern DeviceConfig config;

// Read saved settings from flash into `config` (missing keys -> "" / 0).
void loadConfig();

// Write the current `config` to flash so it survives a reboot/power loss.
void saveConfig();

// Erase saved settings from flash and reset the working copy to blank.
void clearConfig();
