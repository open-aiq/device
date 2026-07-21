#pragma once

#include <Arduino.h>

// ---------------------------------------------------------------------------
// Bluetooth Low Energy (BLE) provisioning
//
// Turns the ESP32 into a tiny "wireless serial port" using the Nordic UART
// Service (NUS). A phone connects and sends newline-terminated WiFi/location
// commands (SSID:, PASS:, LAT:, LON:, SAVE, ...). Responses are also newline
// terminated and may span multiple 20-byte notifications. See ble.cpp.
// ---------------------------------------------------------------------------

// Build and start the whole BLE service and begin advertising.
void setupBluetooth();

// Send one newline-terminated, MTU-safe line to the phone (no-op if none connected).
void bleSend(const String &msg);
