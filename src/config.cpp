#include "config.h"

#include <Preferences.h> // ESP32 internal flash key/value storage (NVS)

static Preferences preferences; // handle to the flash key/value store

DeviceConfig config; // the live working copy, held in RAM

// Read saved settings from flash into `config`. Anything not yet saved comes
// back as the given default ("" or 0).
void loadConfig()
{
  preferences.begin("config", true); // open storage area "config", read-only
  config.ssid = preferences.getString("ssid", "");
  config.password = preferences.getString("pass", "");
  config.lat = preferences.getDouble("lat", 0.0);
  config.lon = preferences.getDouble("lon", 0.0);
  config.deviceId = preferences.getString("devid", "");
  config.deviceKey = preferences.getString("devkey", "");
  preferences.end(); // close it (frees the handle)

  log_i("Config loaded from flash:");
  log_i("  SSID: %s", config.ssid.length() ? config.ssid.c_str() : "(none)");
  log_i("  LAT:  %.6f", config.lat);
  log_i("  LON:  %.6f", config.lon);
  log_i("  DEVID: %s", config.deviceId.length() ? config.deviceId.c_str() : "(none)");
  log_i("  DEVKEY: %s", config.deviceKey.length() ? "(set)" : "(none)");
}

// Write the current `config` to flash so it survives a reboot/power loss.
void saveConfig()
{
  preferences.begin("config", false); // open storage area "config", read-write
  preferences.putString("ssid", config.ssid);
  preferences.putString("pass", config.password);
  preferences.putDouble("lat", config.lat);
  preferences.putDouble("lon", config.lon);
  preferences.putString("devid", config.deviceId);
  preferences.putString("devkey", config.deviceKey);
  preferences.end();
  log_i("Config saved to flash");
}

// Erase saved settings from flash and reset the working copy to blank.
void clearConfig()
{
  preferences.begin("config", false);
  preferences.clear(); // wipe everything in this storage area
  preferences.end();
  config = DeviceConfig{"", "", 0.0, 0.0, "", ""};
  log_i("Config cleared");
}
