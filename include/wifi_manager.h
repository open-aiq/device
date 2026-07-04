#pragma once

#include <Arduino.h>

// Connect to WiFi using the credentials in `config`. Gives up after `timeoutMs`
// instead of looping forever, so a wrong password can't freeze the device — it
// stays alive and reachable over BLE so you can fix the credentials.
// Returns true only if the connection succeeded.
bool connectWiFi(uint32_t timeoutMs = 15000);
