#include "sensors.h"

// Map a driver status code to a human-readable string. Kept here (not in the
// caller) because the status->meaning mapping is sensor-domain knowledge; it
// just doesn't do any I/O.
static const char *pmStatusText(SerialPM::STATUS status)
{
  switch (status)
  {
  case SerialPM::OK:
    return nullptr;
  case SerialPM::ERROR_TIMEOUT:
    return PMS_ERROR_TIMEOUT;
  case SerialPM::ERROR_MSG_UNKNOWN:
    return PMS_ERROR_MSG_UNKNOWN;
  case SerialPM::ERROR_MSG_HEADER:
    return PMS_ERROR_MSG_HEADER;
  case SerialPM::ERROR_MSG_BODY:
    return PMS_ERROR_MSG_BODY;
  case SerialPM::ERROR_MSG_START:
    return PMS_ERROR_MSG_START;
  case SerialPM::ERROR_MSG_LENGTH:
    return PMS_ERROR_MSG_LENGTH;
  case SerialPM::ERROR_MSG_CKSUM:
    return PMS_ERROR_MSG_CKSUM;
  case SerialPM::ERROR_PMS_TYPE:
    return PMS_ERROR_PMS_TYPE;
  }
  return "Unknown sensor error";
}

// ---- PMSensor ----

PMSensor::PMSensor(HardwareSerial &serial, uint8_t rxPin, uint8_t txPin, uint32_t baud)
    : serial(serial), pms(PMSx003, serial), rxPin(rxPin), txPin(txPin), baud(baud)
{
}

void PMSensor::begin()
{
  serial.begin(baud, SERIAL_8N1, rxPin, txPin);
  pms.init();
}

PMReading PMSensor::read()
{
  PMReading r{};
  r.sensor = "PMS5003";
  pms.read();

  if (!pms)
  {
    r.ok = false;
    r.status = pms.status;
    r.error = pmStatusText(pms.status);
    return r;
  }

  r.ok = true;
  r.status = pms.status;

  r.pm01 = pms.pm01;
  r.pm25 = pms.pm25;
  r.pm10 = pms.pm10;

  r.hasNumberConc = pms.has_number_concentration();
  if (r.hasNumberConc)
  {
    r.n0p3 = pms.n0p3;
    r.n0p5 = pms.n0p5;
    r.n1p0 = pms.n1p0;
    r.n2p5 = pms.n2p5;
    r.n5p0 = pms.n5p0;
    r.n10p0 = pms.n10p0;
  }

  return r;
}

// ---- ThermalHumiditySensor ----

// Map a DHT type code to its model name. The codes are the values DHT.h defines
// for DHT11 / DHT21 / DHT22.
static const char *dhtName(uint8_t type)
{
  switch (type)
  {
  case DHT11:
    return "DHT11";
  case DHT21:
    return "DHT21";
  case DHT22:
    return "DHT22";
  default:
    return "DHT";
  }
}

ThermalHumiditySensor::ThermalHumiditySensor(uint8_t pin, uint8_t type)
    : dht(pin, type), name(dhtName(type))
{
}

void ThermalHumiditySensor::begin()
{
  dht.begin();
}

THReading ThermalHumiditySensor::read()
{
  THReading r{};
  r.sensor = name;
  float h = dht.readHumidity();
  float t = dht.readTemperature(); // Celsius

  if (isnan(h) || isnan(t))
  {
    r.ok = false;
    return r;
  }

  r.ok = true;
  r.humidity = h;
  r.temperatureC = t;
  r.heatIndexC = dht.computeHeatIndex(t, h, /*isFahrenheit=*/false);
  return r;
}

// ---- Sensors facade ----

Sensors::Sensors(HardwareSerial &pmSerial, uint8_t pmRx, uint8_t pmTx,
                 uint8_t dhtPin, uint8_t dhtType)
    : pm(pmSerial, pmRx, pmTx), th(dhtPin, dhtType)
{
}

void Sensors::begin()
{
  pm.begin();
  th.begin();
}

SensorReadings Sensors::read()
{
  SensorReadings r{};
  r.pm = pm.read();
  r.th = th.read();

  // The AQI is only meaningful with a valid PM reading; otherwise leave it
  // zero-initialized (callers gate on r.pm.ok).
  if (r.pm.ok)
  {
    r.aqi = computeUsAqi(r.pm.pm25, r.pm.pm10);
  }

  return r;
}
