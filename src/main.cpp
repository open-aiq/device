// main.cpp: Read PMS5003 sensor on Serial2 (ESP32)

#include <Arduino.h>
#include <PMserial.h>  // Arduino library for PM sensors with serial interface
#include <Wire.h>
#include <LiquidCrystal_I2C.h>
#include <DHT.h>
#include <WiFi.h>
#include <WiFiClientSecure.h>
#include <HTTPClient.h>

#include "secrets.h"

LiquidCrystal_I2C lcd(0x3F, 16, 2);


#define DHTPIN 14
#define DHTTYPE DHT22
DHT dht(DHTPIN, DHTTYPE);

SerialPM pms(PMSx003, Serial2);

#define BTN_RIGHT 34
#define BTN_LEFT 35
#define BTN_SETTINGS 36
#define BTN_BOOT 39

volatile bool rightPressed = false;
volatile bool leftPressed = false;
volatile bool settingsPressed = false;
volatile bool bootPressed = false;

volatile uint32_t lastRightInterrupt = 0;
volatile uint32_t lastLeftInterrupt = 0;
volatile uint32_t lastSettingsInterrupt = 0;
volatile uint32_t lastBootInterrupt = 0;

const uint32_t DEBOUNCE_MS = 150;




void IRAM_ATTR rightISR() {
  uint32_t now = millis();

  if (now - lastRightInterrupt > DEBOUNCE_MS) {
    rightPressed = true;
    lastRightInterrupt = now;
  }
}

void IRAM_ATTR leftISR() {
  uint32_t now = millis();

  if (now - lastLeftInterrupt > DEBOUNCE_MS) {
    leftPressed = true;
    lastLeftInterrupt = now;
  }
}

void IRAM_ATTR settingsISR() {
  uint32_t now = millis();

  if (now - lastSettingsInterrupt > DEBOUNCE_MS) {
    settingsPressed = true;
    lastSettingsInterrupt = now;
  }
}

void IRAM_ATTR bootISR() {
  uint32_t now = millis();

  if (now - lastBootInterrupt > DEBOUNCE_MS) {
    bootPressed = true;
    lastBootInterrupt = now;
  }
}


void setup() {

  Wire.begin(21, 22);
  lcd.init();
  lcd.backlight();
  lcd.print("Starting");


  Serial.begin(9600);
  Serial2.begin(9600, SERIAL_8N1, 16, 17);


  pinMode(BTN_RIGHT, INPUT);
  pinMode(BTN_LEFT, INPUT);
  pinMode(BTN_SETTINGS, INPUT);
  pinMode(BTN_BOOT, INPUT);

  attachInterrupt(
    digitalPinToInterrupt(BTN_RIGHT),
    rightISR,
    FALLING);

  attachInterrupt(
    digitalPinToInterrupt(BTN_LEFT),
    leftISR,
    FALLING);

  attachInterrupt(
    digitalPinToInterrupt(BTN_SETTINGS),
    settingsISR,
    FALLING);

  attachInterrupt(
    digitalPinToInterrupt(BTN_BOOT),
    bootISR,
    FALLING);




  Serial.println(F("Booted"));
  pms.init();
  dht.begin();



  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  Serial.print("Connecting");

  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
  }

  Serial.println();
  Serial.println("WiFi Connected");


  Serial.println("\nConnected!");

  Serial.print("Local IP: ");
  Serial.println(WiFi.localIP());

  Serial.print("Gateway: ");
  Serial.println(WiFi.gatewayIP());

  Serial.print("Subnet: ");
  Serial.println(WiFi.subnetMask());

  Serial.print("RSSI: ");
  Serial.println(WiFi.RSSI());

    configTime(0, 0, "pool.ntp.org", "time.nist.gov");

  Serial.print("Waiting for time");

  struct tm timeinfo;

  // wait until time is valid (important for SSL)
  while (!getLocalTime(&timeinfo)) {
    Serial.print(".");
    delay(500);
  }

  Serial.println("\nTime synced");

  Serial.println(&timeinfo, "%Y-%m-%d %H:%M:%S");

  HTTPClient http;

  http.begin("http://192.168.1.8:8080/api/v1/air-quality/current");

  int httpCode = http.GET();

  if (httpCode > 0) {
    String payload = http.getString();

    Serial.println("Response:");
    Serial.println(payload);
  } else {
    Serial.printf("HTTP Error: %d\n", httpCode);
    Serial.println(http.errorToString(httpCode));
  }

  http.end();




  WiFiClientSecure client;
  client.setInsecure();  // safe for testing

  HTTPClient https;

  // NOTE: the '/' before '?' is required — HTTPClient's URL parser splits
  // host/path at the first '/', so without it the query string becomes part
  // of the hostname and DNS resolution fails.
  if (https.begin(client, "https://api.ipify.org/?format=json")) {

    int code = https.GET();

    Serial.print("HTTP code: ");
    Serial.println(code);

    if (code > 0) {
      String payload = https.getString();
      Serial.println("Response:");
      Serial.println(payload);
    } else {
      Serial.println("Request failed");
    }

    https.end();
  } else {
    Serial.println("HTTPS begin failed");
  }
}

void loop() {
  if (rightPressed) {
    rightPressed = false;

    Serial.println("RIGHT");
    // TODO: Next screen
  }

  if (leftPressed) {
    leftPressed = false;

    Serial.println("LEFT");
    // TODO: Previous screen
  }

  if (settingsPressed) {
    settingsPressed = false;

    Serial.println("SETTINGS");
    // TODO: Open settings menu
  }

  if (bootPressed) {
    bootPressed = false;

    Serial.println("BOOT");
    // TODO: Return to home screen
  }

  float h = dht.readHumidity();
  float t = dht.readTemperature();  // Celsius

  if (isnan(h) || isnan(t)) {
    Serial.println("Failed to read from DHT sensor");
  } else {
    Serial.print("Humidity: ");
    Serial.print(h);
    Serial.print(" %  |  Temperature: ");
    Serial.print(t);
    Serial.println(" °C");
  }

  pms.read();
  if (pms) {
    // print formatted results
    Serial.printf("PM1.0 %2d, PM2.5 %2d, PM10 %2d [ug/m3]\n",
                  pms.pm01, pms.pm25, pms.pm10);
    lcd.print("PM 2.5 " + pms.pm25);

    if (pms.has_number_concentration()) {

      Serial.printf("N0.3 %4d, N0.5 %3d, N1.0 %2d, N2.5 %2d, N5.0 %2d, N10 %2d [#/100cc]\n",
                    pms.n0p3, pms.n0p5, pms.n1p0, pms.n2p5, pms.n5p0, pms.n10p0);
    }

    if (pms.has_temperature_humidity() || pms.has_formaldehyde())
      Serial.printf("%5.1f °C, %5.1f %%rh, %5.2f mg/m3 HCHO\n",
                    pms.temp, pms.rhum, pms.hcho);

  } else {  // something went wrong
    switch (pms.status) {
      case pms.OK:  // should never come here
        break;      // included to compile without warnings
      case pms.ERROR_TIMEOUT:
        Serial.println(F(PMS_ERROR_TIMEOUT));
        break;
      case pms.ERROR_MSG_UNKNOWN:
        Serial.println(F(PMS_ERROR_MSG_UNKNOWN));
        break;
      case pms.ERROR_MSG_HEADER:
        Serial.println(F(PMS_ERROR_MSG_HEADER));
        break;
      case pms.ERROR_MSG_BODY:
        Serial.println(F(PMS_ERROR_MSG_BODY));
        break;
      case pms.ERROR_MSG_START:
        Serial.println(F(PMS_ERROR_MSG_START));
        break;
      case pms.ERROR_MSG_LENGTH:
        Serial.println(F(PMS_ERROR_MSG_LENGTH));
        break;
      case pms.ERROR_MSG_CKSUM:
        Serial.println(F(PMS_ERROR_MSG_CKSUM));
        break;
      case pms.ERROR_PMS_TYPE:
        Serial.println(F(PMS_ERROR_PMS_TYPE));
        break;
    }
  }

  // wait for 10 seconds
  delay(10000);
}
