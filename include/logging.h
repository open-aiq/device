#pragma once

#include "sensors.h"
#include "aqi.h"

// ---------------------------------------------------------------------------
// Output layer: kept out of the sensor classes so they stay decoupled from
// however we surface readings (Serial today; LCD/BLE/HTTP later).
// ---------------------------------------------------------------------------

void logThReading(const THReading &r);
void logPmReading(const PMReading &r);
void logAqi(const AqiResult &a);
