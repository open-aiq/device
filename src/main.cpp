// main.cpp: top-level wiring for the air-quality monitor (ESP32).
// Sensors, BLE provisioning, WiFi, uploads, buttons, display and logging each
// live in their own module; this file just owns the globals, the screen state
// and the setup()/loop() flow.

#include <Arduino.h>
#include <Wire.h>
#include <WiFi.h>

#include "secrets.h"
#include "sensors.h"
#include "aqi.h"
#include "pins.h"
#include "config.h"
#include "wifi_manager.h"
#include "ble.h"
#include "buttons.h"
#include "logging.h"
#include "display.h"
#include "uploader.h"

// Older secrets.h files predate the device-credential / mock-location fields;
// default them so the firmware still compiles (uploads are skipped until real
// credentials are provisioned).
#ifndef DEVICE_ID
#define DEVICE_ID ""
#endif
#ifndef DEVICE_KEY
#define DEVICE_KEY ""
#endif
#ifndef MOCK_LAT
#define MOCK_LAT 0.0
#endif
#ifndef MOCK_LON
#define MOCK_LON 0.0
#endif

Display display(0x3F, 16, 2); // 16x2 I2C LCD at address 0x3F
Sensors sensors(Serial2, PMS_RX_PIN, PMS_TX_PIN, DHT_PIN, DHT22);

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

enum Screen
{
  SCREEN_AQI,     // home: AQI + PM2.5
  SCREEN_CLIMATE, // temperature / humidity / heat index
  SCREEN_STATUS,  // WiFi + last upload
  SCREEN_COUNT,
};

static Screen currentScreen = SCREEN_AQI;

// Latest sample plus upload bookkeeping, shared between loop() and the screens.
static SensorReadings lastReadings{};
static bool haveReadings = false;
static UploadResult lastUpload{false, 0};
static uint32_t lastUploadAt = 0; // millis() of the last attempt; 0 = never

// Render the current screen from the latest known state. Uses fixed-width row
// writes (Display::showLines), so frequent redraws don't flicker or leave
// stale characters behind.
static void renderScreen()
{
  switch (currentScreen)
  {
  case SCREEN_AQI: {
    if (!haveReadings || !lastReadings.pm.ok)
    {
      display.showLines("AQI --", haveReadings ? "PMS err" : "warming up...");
      return;
    }
    String line1 = "AQI " + String(lastReadings.aqi.aqi) + " ";
    line1 += aqiCategoryName(lastReadings.aqi.category);
    String line2 = "PM2.5 " + String(lastReadings.pm.pm25) + "ug/m3";
    display.showLines(line1, line2);
    return;
  }
  case SCREEN_CLIMATE: {
    if (!haveReadings || !lastReadings.th.ok)
    {
      display.showLines("Temp --", haveReadings ? "DHT err" : "warming up...");
      return;
    }
    String line1 = "T " + String(lastReadings.th.temperatureC, 1) + "C";
    line1 += " HI " + String(lastReadings.th.heatIndexC, 1) + "C";
    String line2 = "Humidity " + String(lastReadings.th.humidity, 0) + "%";
    display.showLines(line1, line2);
    return;
  }
  case SCREEN_STATUS: {
    String line1 = (WiFi.status() == WL_CONNECTED)
                       ? "WiFi " + WiFi.localIP().toString()
                       : String("WiFi DOWN");
    String line2;
    if (lastUploadAt == 0)
    {
      line2 = "Up: never";
    }
    else
    {
      uint32_t ageMin = (millis() - lastUploadAt) / 60000;
      line2 = "Up " + String(lastUpload.httpCode) + " " + String(ageMin) + "m ago";
    }
    display.showLines(line1, line2);
    return;
  }
  default:
    return;
  }
}

// Apply one button press to the screen state. Right/Left cycle, Boot returns
// home, Settings jumps to the status screen (a real settings menu is on the
// roadmap).
static void handleButton(ButtonEvent event)
{
  switch (event)
  {
  case BTN_RIGHT:
    currentScreen = static_cast<Screen>((currentScreen + 1) % SCREEN_COUNT);
    break;
  case BTN_LEFT:
    currentScreen = static_cast<Screen>((currentScreen + SCREEN_COUNT - 1) % SCREEN_COUNT);
    break;
  case BTN_BOOT:
    currentScreen = SCREEN_AQI;
    break;
  case BTN_SETTINGS:
    currentScreen = SCREEN_STATUS;
    break;
  default:
    return;
  }
  renderScreen();
}

// ---------------------------------------------------------------------------
// Provisioning
// ---------------------------------------------------------------------------

// Stand-in for the mobile app: on first boot (nothing saved in flash), seed the
// config from secrets.h exactly as the app would over BLE. The BLE commands
// (SSID:/PASS:/LAT:/LON:/DEVID:/DEVKEY:/SAVE) keep working and overwrite this.
static void applyProvisioning()
{
#if WIPE_CONFIG_ON_BOOT
  log_w("WIPE_CONFIG_ON_BOOT is set — clearing saved config");
  clearConfig();
#endif

  loadConfig();

#if MOCK_APP_PROVISIONING
  if (config.ssid.length() == 0)
  {
    log_w("No saved config — mock app provisioning from secrets.h");
    config.ssid = WIFI_SSID;
    config.password = WIFI_PASSWORD;
    config.deviceId = DEVICE_ID;
    config.deviceKey = DEVICE_KEY;
    config.lat = MOCK_LAT;
    config.lon = MOCK_LON;
    saveConfig();
  }
#else
  if (config.ssid.length() == 0)
  {
    log_w("No saved WiFi — provision over BLE (SSID:/PASS:/SAVE)");
  }
#endif
}

void setup()
{
  Serial.begin(9600);
  log_i("Booted");

  Wire.begin(I2C_SDA_PIN, I2C_SCL_PIN);
  display.begin();
  display.showMessage("Starting");

  setupButtons();
  sensors.begin();
  setupBluetooth();

  applyProvisioning();

  // WiFi failure is not fatal: the device keeps sampling and stays reachable
  // over BLE so credentials can be fixed; uploads retry on their own cadence.
  if (!connectWiFi())
  {
    log_w("WiFi not connected — connect over BLE and send SSID:/PASS:/SAVE");
  }

  renderScreen();
}

// How often to sample the sensors (drives the display refresh).
static const uint32_t SENSOR_INTERVAL_MS = 3000;
// How often to upload a reading. 10 min = half the backend's 20-minute offline
// threshold, so one missed upload doesn't flip the device to "offline".
static const uint32_t UPLOAD_INTERVAL_MS = 10 * 60 * 1000;

// True once `intervalMs` has elapsed since the last time it returned true for
// that timer; resets the timer when it fires. The unsigned subtraction is
// millis()-rollover safe (~49 days). `fireImmediately` makes the first call
// fire at once (used so the first upload happens right after boot).
static bool intervalElapsed(uint32_t &lastFired, uint32_t intervalMs, bool fireImmediately)
{
  uint32_t now = millis();
  if (lastFired == 0 && fireImmediately)
  {
    lastFired = now;
    return true;
  }
  if (now - lastFired < intervalMs)
  {
    return false;
  }
  lastFired = now;
  return true;
}

// Read every sensor once, log the results and refresh the display.
static void sampleSensors()
{
  lastReadings = sensors.read();
  haveReadings = true;

  logThReading(lastReadings.th);
  logPmReading(lastReadings.pm);
  if (lastReadings.pm.ok)
  {
    logAqi(lastReadings.aqi);
  }

  renderScreen();
}

// Push the latest reading to the backend, reconnecting WiFi first if it
// dropped. Result is kept for the status screen.
static void uploadLatest()
{
  if (!haveReadings)
  {
    return;
  }

  if (WiFi.status() != WL_CONNECTED)
  {
    log_w("Upload tick: WiFi down, reconnecting first");
    connectWiFi();
  }

  lastUpload = uploadReading(lastReadings);
  lastUploadAt = millis();

  if (currentScreen == SCREEN_STATUS)
  {
    renderScreen();
  }
}

void loop()
{
  ButtonEvent event = nextButtonEvent();
  if (event != BTN_NONE)
  {
    handleButton(event);
  }

  static uint32_t lastSample = 0;
  if (intervalElapsed(lastSample, SENSOR_INTERVAL_MS, false))
  {
    sampleSensors();
  }

  // First upload fires as soon as a reading exists, so the device shows up
  // online right after boot instead of 10 minutes later.
  static uint32_t lastUploadTick = 0;
  if (haveReadings && intervalElapsed(lastUploadTick, UPLOAD_INTERVAL_MS, true))
  {
    uploadLatest();
  }

  // Yield ~1 ms back to the RTOS each pass. This isn't a pacing delay — the
  // cadences above are non-blocking — it just lets the idle task feed the
  // task watchdog and gives WiFi/BLE housekeeping CPU time, which a tight
  // busy-loop would starve. Too small to affect button latency.
  delay(1);
}
