#include "ble.h"

#include <WiFi.h>
#include <BLEDevice.h>
#include <BLEServer.h>
#include <BLEUtils.h>
#include <BLE2902.h>

#include "config.h"
#include "wifi_manager.h"

// The name the phone sees when scanning for Bluetooth devices.
#define BLE_DEVICE_NAME "AirMonitor-Setup"

// Fixed IDs for the Nordic UART Service. These are an industry standard, so
// generic BLE apps already know how to talk to them.
#define NUS_SERVICE_UUID "6E400001-B5A3-F393-E0A9-E50E24DCCA9E" // the service
#define NUS_RX_UUID      "6E400002-B5A3-F393-E0A9-E50E24DCCA9E" // phone -> ESP32
#define NUS_TX_UUID      "6E400003-B5A3-F393-E0A9-E50E24DCCA9E" // ESP32 -> phone

// Pointer to the TX channel so we can push messages out from anywhere.
static BLECharacteristic *bleTxCharacteristic = nullptr;
// True while a phone is connected. (volatile: changed inside BLE callbacks.)
static volatile bool bleClientConnected = false;

// Send one line of text back to the connected phone.
void bleSend(const String &msg)
{
  if (bleClientConnected && bleTxCharacteristic != nullptr)
  {
    // Put the text into the TX channel, then "notify" so the phone receives it.
    // The (uint8_t*, size_t) form works on both old and new ESP32 BLE versions.
    bleTxCharacteristic->setValue((uint8_t *)msg.c_str(), msg.length());
    bleTxCharacteristic->notify();
    log_i("BLE sent: %s", msg.c_str());
  }
  else
  {
    // Nothing is connected, so there is nobody to send to.
    log_w("BLE send skipped (no client connected)");
  }
}

// Decide what to do with one command the phone sent us.
// For now it's just a simple test protocol; WiFi commands come later.
static void processBluetoothCommand(const String &cmd)
{
  log_i("BLE received: %s", cmd.c_str());

  if (cmd.equalsIgnoreCase("PING"))
  {
    bleSend("PONG"); // simple "are you alive?" check
  }
  else if (cmd.equalsIgnoreCase("STATUS"))
  {
    String status = "WiFi: ";
    status += (WiFi.status() == WL_CONNECTED) ? "connected" : "disconnected";
    status += " IP: ";
    status += WiFi.localIP().toString();
    bleSend(status); // report current WiFi state

    // --- WiFi-credential provisioning -----------------------------------------
    // The phone sends these one at a time to fill in the settings, then "SAVE".
    // substring() keeps everything after the prefix, so passwords containing
    // ':' are handled correctly.
  }
  else if (cmd.startsWith("SSID:"))
  {
    config.ssid = cmd.substring(5); // text after "SSID:"
    bleSend("OK: SSID set to " + config.ssid);
  }
  else if (cmd.startsWith("PASS:"))
  {
    config.password = cmd.substring(5);
    bleSend("OK: password set"); // never echo the password back
  }
  else if (cmd.startsWith("LAT:"))
  {
    config.lat = cmd.substring(4).toDouble(); // "31.5204" -> 31.5204
    bleSend("OK: LAT set to " + String(config.lat, 6));
  }
  else if (cmd.startsWith("LON:"))
  {
    config.lon = cmd.substring(4).toDouble();
    bleSend("OK: LON set to " + String(config.lon, 6));
  }
  else if (cmd.startsWith("DEVID:"))
  {
    config.deviceId = cmd.substring(6); // backend device identity ("dev_...")
    bleSend("OK: DEVID set to " + config.deviceId);
  }
  else if (cmd.startsWith("DEVKEY:"))
  {
    config.deviceKey = cmd.substring(7);
    bleSend("OK: device key set"); // never echo the secret back
  }
  else if (cmd.equalsIgnoreCase("SAVE"))
  {
    saveConfig(); // write all four values to flash
    bleSend("SAVED. Reconnecting WiFi...");
    if (connectWiFi())
    { // try the new credentials right away
      bleSend("WiFi connected: " + WiFi.localIP().toString());
    }
    else
    {
      bleSend("WiFi connect FAILED (check SSID/password)");
    }
  }
  else if (cmd.equalsIgnoreCase("LOAD"))
  {
    // Report what's currently set (secrets hidden for safety).
    String out = "SSID=" + config.ssid;
    out += " PASS=" + String(config.password.length() ? "(set)" : "(empty)");
    out += " LAT=" + String(config.lat, 6);
    out += " LON=" + String(config.lon, 6);
    out += " DEVID=" + String(config.deviceId.length() ? config.deviceId.c_str() : "(empty)");
    out += " DEVKEY=" + String(config.deviceKey.length() ? "(set)" : "(empty)");
    bleSend(out);
  }
  else if (cmd.equalsIgnoreCase("CLEAR"))
  {
    clearConfig(); // wipe stored settings
    bleSend("CLEARED all stored config");
  }
  else
  {
    bleSend("echo: " + cmd); // anything else: just echo it back
  }
}

// These callbacks run automatically when a phone connects or disconnects.
class ServerCallbacks : public BLEServerCallbacks
{
  void onConnect(BLEServer *server) override
  {
    bleClientConnected = true;
    log_i("BLE: client CONNECTED");
  }
  void onDisconnect(BLEServer *server) override
  {
    bleClientConnected = false;
    log_i("BLE: client DISCONNECTED, advertising again");
    server->startAdvertising(); // become visible again to reconnect
  }
};

// This callback runs automatically every time the phone writes to RX.
class RxCallbacks : public BLECharacteristicCallbacks
{
  void onWrite(BLECharacteristic *characteristic) override
  {
    // Read the text the phone sent. .c_str() works on both old and new
    // ESP32 BLE versions (one returns std::string, the other Arduino String).
    String value = characteristic->getValue().c_str();
    value.trim(); // drop stray newlines / spaces
    if (value.length() > 0)
    {
      processBluetoothCommand(value);
    }
  }
};

// Build and start the whole BLE service. Logs each step so the serial monitor
// shows exactly how far it got if something goes wrong.
void setupBluetooth()
{
  log_i("BLE: setup start");

  // 1) Start the BLE radio and give the device its name.
  BLEDevice::init(BLE_DEVICE_NAME);
  log_i("BLE: radio init done, MAC = %s", BLEDevice::getAddress().toString().c_str());

  // 2) Create the GATT server (the thing that holds our data).
  BLEServer *server = BLEDevice::createServer();
  server->setCallbacks(new ServerCallbacks());
  log_i("BLE: server created");

  // 3) Create the Nordic UART Service and its two channels.
  BLEService *service = server->createService(NUS_SERVICE_UUID);

  // TX channel: ESP32 -> phone. "NOTIFY" lets us push messages out.
  bleTxCharacteristic = service->createCharacteristic(NUS_TX_UUID, BLECharacteristic::PROPERTY_NOTIFY);
  bleTxCharacteristic->addDescriptor(new BLE2902()); // lets the phone enable notifications

  // RX channel: phone -> ESP32. "WRITE" lets the phone send us text.
  BLECharacteristic *rxCharacteristic = service->createCharacteristic(NUS_RX_UUID, BLECharacteristic::PROPERTY_WRITE);
  rxCharacteristic->setCallbacks(new RxCallbacks());

  service->start();
  log_i("BLE: service started");

  // 4) Start advertising (broadcasting) so phones can find us.
  //
  // Important: a BLE advertisement is only 31 bytes. The 128-bit service UUID
  // (18 bytes) plus the device name would overflow that and the name gets
  // dropped — so the device shows up unnamed or not at all. Fix: put the NAME
  // in the main packet, and the big UUID in the separate "scan response" packet.
  BLEAdvertising *advertising = BLEDevice::getAdvertising();

  BLEAdvertisementData advData;
  advData.setFlags(0x06);           // standard "discoverable, BLE-only" flags
  advData.setName(BLE_DEVICE_NAME); // name goes in the main packet
  advertising->setAdvertisementData(advData);

  BLEAdvertisementData scanResponse;
  scanResponse.setCompleteServices(BLEUUID(NUS_SERVICE_UUID)); // UUID goes here instead
  advertising->setScanResponseData(scanResponse);

  BLEDevice::startAdvertising();
  log_i("BLE: advertising as '" BLE_DEVICE_NAME "' — ready to connect");
}
