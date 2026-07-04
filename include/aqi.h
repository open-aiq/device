#pragma once

#include <Arduino.h>

// ---------------------------------------------------------------------------
// US EPA Air Quality Index
//
// Pure transform: pollutant concentration in, index out. No hardware, no I/O.
// This is intentionally sensor-agnostic — the AQI is a standard defined on
// ug/m3, independent of whatever sensor produced the reading.
//
// v1 is INSTANTANEOUS: the current PM reading is fed straight into the 24-hour
// breakpoint table. That is simple and works on boot, but overstates short
// spikes and is not the standards-correct "current AQI" (which would be
// NowCast over recent hours). The breakpoint math lives in a pure function so
// a NowCast layer can be added later by feeding it a smoothed concentration.
// ---------------------------------------------------------------------------

enum AqiCategory
{
  AQI_GOOD,                // 0-50
  AQI_MODERATE,            // 51-100
  AQI_UNHEALTHY_SENSITIVE, // 101-150
  AQI_UNHEALTHY,           // 151-200
  AQI_VERY_UNHEALTHY,      // 201-300
  AQI_HAZARDOUS,           // 301-500 (and "beyond the AQI", capped at 500)
};

enum AqiPollutant
{
  AQI_PM25,
  AQI_PM10,
};

struct AqiResult
{
  int aqi;               // overall index, 0..500 (capped)
  AqiCategory category;  // band for `aqi`
  AqiPollutant dominant; // pollutant that set the overall index
};

// Instantaneous US AQI from atmospheric PM concentrations [ug/m3], using the
// 2024 EPA breakpoints. The overall AQI is the max of the PM2.5 and PM10
// sub-indices; `dominant` reports which one won.
AqiResult computeUsAqi(uint16_t pm25, uint16_t pm10);

// Human-readable name for a category (e.g. "Good", "Unhealthy").
const char *aqiCategoryName(AqiCategory category);
