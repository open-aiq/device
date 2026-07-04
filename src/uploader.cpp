#include "uploader.h"

#include <ArduinoJson.h>
#include <WiFi.h>
#include <WiFiClientSecure.h>
#include <HTTPClient.h>

#include "config.h"
#include "logging.h"

// Build the JSON payload the backend expects (see backend POST /api/v1/data):
//   pms_data{pm1_0,pm2_5,pm10_0,provider} + aqi +
//   temperature_data{temperature,humidity,heat_index,provider} +
//   location{lat,lon,provider} (only when a location fix exists)
static String buildPayload(const SensorReadings &r)
{
  JsonDocument doc;

  JsonObject pms = doc["pms_data"].to<JsonObject>();
  pms["pm1_0"] = r.pm.pm01;
  pms["pm2_5"] = r.pm.pm25;
  pms["pm10_0"] = r.pm.pm10;
  pms["provider"] = "pms5003";

  doc["aqi"] = r.aqi.aqi;

  JsonObject th = doc["temperature_data"].to<JsonObject>();
  th["temperature"] = r.th.temperatureC;
  th["humidity"] = r.th.humidity;
  th["heat_index"] = r.th.heatIndexC;
  th["provider"] = "dht22";

  // 0/0 means "no fix yet" (Null Island is not a plausible install site).
  if (config.lat != 0.0 || config.lon != 0.0)
  {
    JsonObject location = doc["location"].to<JsonObject>();
    location["lat"] = config.lat;
    location["lon"] = config.lon;
    location["provider"] = "mobile";
  }

  String payload;
  serializeJson(doc, payload);
  return payload;
}

UploadResult uploadReading(const SensorReadings &r)
{
  UploadResult result{false, 0};

  if (!r.pm.ok || !r.th.ok)
  {
    log_w("Upload skipped: incomplete reading (pm.ok=%d th.ok=%d)", r.pm.ok, r.th.ok);
    return result;
  }
  if (config.deviceId.length() == 0 || config.deviceKey.length() == 0)
  {
    log_w("Upload skipped: no device credentials (provision DEVID/DEVKEY)");
    return result;
  }
  if (WiFi.status() != WL_CONNECTED)
  {
    log_w("Upload skipped: WiFi not connected");
    return result;
  }

  // TODO: replace setInsecure() with certificate pinning for the backend host.
  WiFiClientSecure client;
  client.setInsecure();

  HTTPClient https;
  if (!https.begin(client, BACKEND_BASE_URL "/data"))
  {
    log_e("Upload: HTTPS begin failed");
    result.httpCode = -1;
    return result;
  }

  https.addHeader("Content-Type", "application/json");
  https.addHeader("X-Device-ID", config.deviceId);
  https.addHeader("X-Device-Key", config.deviceKey);

  String payload = buildPayload(r);
  log_i("Upload: POST %s", BACKEND_BASE_URL "/data");
  log_d("Upload payload: %s", payload.c_str());

  int code = https.POST(payload);
  result.httpCode = code;
  result.ok = (code == 201);

  if (result.ok)
  {
    log_i("Upload OK (201)");
  }
  else if (code > 0)
  {
    // Backend answered but rejected it — its body says why (401 = bad key).
    log_e("Upload rejected: HTTP %d, body: %s", code, https.getString().c_str());
  }
  else
  {
    log_e("Upload failed: %d (%s)", code, https.errorToString(code).c_str());
  }

  https.end();
  return result;
}
