#pragma once

#include <Arduino.h>
#include <PMserial.h> // SerialPM driver + PMS_ERROR_* / STATUS definitions
#include <DHT.h>

#include "aqi.h" // the Sensors facade derives the AQI from the PM reading

// ---------------------------------------------------------------------------
// Sensor layer
//
// Each sensor class owns its hardware handle and returns a plain data struct.
// It does NO output: no Serial, no LCD, no BLE. The caller decides what to do
// with a reading (log it, show it, transmit it). This keeps the sensors
// decoupled from however we happen to surface the data today.
// ---------------------------------------------------------------------------

// One particulate-matter sample. `ok` says whether the read succeeded; when it
// is false only `status`/`error` are meaningful.
struct PMReading
{
  const char *sensor; // which device produced this, e.g. "PMS5003"
  bool ok;
  SerialPM::STATUS status; // raw driver status code
  const char *error;       // human-readable error, nullptr when ok

  // Mass concentration [ug/m3].
  uint16_t pm01, pm25, pm10;

  // Number concentration [#/100cc]; valid only when hasNumberConc is true.
  bool hasNumberConc;
  uint16_t n0p3, n0p5, n1p0, n2p5, n5p0, n10p0;
};

// One temperature/humidity sample. `ok` is false if the DHT returned NaN.
struct THReading
{
  const char *sensor; // which device produced this, e.g. "DHT22"
  bool ok;
  float temperatureC;
  float humidity;
  float heatIndexC; // "feels like" temperature derived from temp + humidity
};

// One full sample from every on-device sensor, plus the AQI derived from it.
// `aqi` is computed from `pm` and is only meaningful when `pm.ok` — with no
// valid PM there is no index. Check `pm.ok` before using `aqi`.
struct SensorReadings
{
  PMReading pm;  // particulate matter
  THReading th;  // temperature + humidity
  AqiResult aqi; // US AQI derived from `pm`; valid only when pm.ok
};

// Plantower PMSx003 particulate-matter sensor on a hardware serial port.
class PMSensor
{
public:
  // `serial` is the UART wired to the sensor (e.g. Serial2); rx/tx are the ESP32
  // pins for that UART.
  PMSensor(HardwareSerial &serial, uint8_t rxPin, uint8_t txPin, uint32_t baud = 9600);

  void begin(); // brings up the UART and the sensor
  PMReading read();

private:
  HardwareSerial &serial;
  SerialPM pms;
  uint8_t rxPin, txPin;
  uint32_t baud;
};

// DHT-series temperature/humidity sensor on a single GPIO.
class ThermalHumiditySensor
{
public:
  ThermalHumiditySensor(uint8_t pin, uint8_t type);

  void begin();
  THReading read();

private:
  DHT dht;
  const char *name; // device name stamped into each reading, e.g. "DHT22"
};

// Facade over all on-device sensors. Owns each sensor by value and gives
// main.cpp a single entry point.
class Sensors
{
public:
  Sensors(HardwareSerial &pmSerial, uint8_t pmRx, uint8_t pmTx,
          uint8_t dhtPin, uint8_t dhtType);

  void begin();

  // Read every sensor and compute the AQI from the PM reading.
  SensorReadings read();

private:
  PMSensor pm;
  ThermalHumiditySensor th;
};
