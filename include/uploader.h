#pragma once

#include <Arduino.h>

#include "sensors.h"

// ---------------------------------------------------------------------------
// Telemetry uploader
//
// Sends one full sensor reading (in practice the current upload window's
// average — see Aggregator in main.cpp) to the AIQ backend as
// POST {BACKEND_BASE_URL}/data, authenticated with the device credentials in
// `config` (X-Device-ID / X-Device-Key headers). The backend timestamps the
// reading on arrival, so only current data is uploaded — no backlog.
// ---------------------------------------------------------------------------

// Outcome of one upload attempt. `httpCode` is the backend's HTTP status
// (201 = stored), or a negative HTTPClient error when the request never
// completed; 0 when the attempt was skipped (no WiFi / no credentials / bad
// reading).
struct UploadResult
{
  bool ok;      // true only on HTTP 201
  int httpCode; // see above
};

// Upload one reading. Requires pm.ok && th.ok (the backend rejects partial
// payloads), WiFi, and provisioned device credentials; otherwise skips with a
// log and returns ok=false, httpCode=0.
UploadResult uploadReading(const SensorReadings &readings);
