#include "wifi_manager.h"

#include <WiFi.h>

#include "config.h"

bool connectWiFi(uint32_t timeoutMs)
{
  if (config.ssid.length() == 0)
  {
    log_w("WiFi: no SSID configured, skipping connect");
    return false;
  }

  log_i("WiFi: connecting to '%s'", config.ssid.c_str());

  WiFi.persistent(false);      // we manage credentials in NVS ourselves
  WiFi.setAutoReconnect(true); // radio re-associates by itself after drops
  WiFi.begin(config.ssid.c_str(), config.password.c_str());

  // WiFi.begin() is asynchronous — poll until associated or the timeout hits.
  uint32_t start = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - start < timeoutMs)
  {
    delay(250);
  }

  if (WiFi.status() == WL_CONNECTED)
  {
    log_i("WiFi: connected, IP = %s, RSSI = %d", WiFi.localIP().toString().c_str(), WiFi.RSSI());
    return true;
  }

  log_e("WiFi: connection timed out");
  return false;
}
