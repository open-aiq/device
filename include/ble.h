#pragma once

#include <Arduino.h>

// ---------------------------------------------------------------------------
// Bluetooth Low Energy (BLE) provisioning
//
// Turns the ESP32 into a tiny "wireless serial port" using the Nordic UART
// Service (NUS). A phone connects and sends WiFi/location settings as text
// commands (SSID:, PASS:, LAT:, LON:, SAVE, ...). See ble.cpp for the protocol.
// ---------------------------------------------------------------------------

// Build and start the whole BLE service and begin advertising.
void setupBluetooth();

// Send one line of text back to the connected phone (no-op if none connected).
void bleSend(const String &msg);
