#include "logging.h"

#include <Arduino.h>

// Output uses the Arduino-ESP32 log_* macros (esp32-hal-log.h). Level is set by
// CORE_DEBUG_LEVEL in platformio.ini; anything above it is stripped at compile
// time. Each macro is printf-style, auto-appends a newline, and prefixes the
// line with timestamp, level, and source location.

// Print one temperature/humidity reading.
void logThReading(const THReading &r)
{
  if (!r.ok)
  {
    log_e("DHT read failed (NaN)");
    return;
  }

  log_i("Humidity %.1f %%  |  Temp %.1f C", r.humidity, r.temperatureC);
}

// Print one particulate-matter reading.
void logPmReading(const PMReading &r)
{
  if (!r.ok)
  {
    log_e("PM sensor read failed: %s", r.error ? r.error : "unknown");
    return;
  }

  log_i("PM1.0 %u, PM2.5 %u, PM10 %u [ug/m3]", r.pm01, r.pm25, r.pm10);

  if (r.hasNumberConc)
  {
    log_d("N0.3 %u, N0.5 %u, N1.0 %u, N2.5 %u, N5.0 %u, N10 %u [#/100cc]",
          r.n0p3, r.n0p5, r.n1p0, r.n2p5, r.n5p0, r.n10p0);
  }
}

// Print the computed US AQI.
void logAqi(const AqiResult &a)
{
  log_i("US AQI %d (%s), driven by %s",
        a.aqi, aqiCategoryName(a.category),
        a.dominant == AQI_PM25 ? "PM2.5" : "PM10");
}
