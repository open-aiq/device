#pragma once

// ---------------------------------------------------------------------------
// Pin map (ESP32)
//
// Two layers:
//   1. Raw GPIO aliases — only the pins we actually use, named D<n>.
//   2. Usage aliases     — what each pin is wired to, pointing at a D<n>.
//
// Always refer to the usage names in code so re-wiring a pin only touches the
// usage layer below.
// ---------------------------------------------------------------------------

// ---- 1. Raw GPIO aliases (only pins in use) ----
#define D14 14
#define D16 16
#define D17 17
#define D21 21
#define D22 22
#define D34 34
#define D35 35
#define D36 36
#define D39 39

// ---- 2. Pins by usage ----

// Buttons (active-low, wired to interrupts)
#define BTN_RIGHT_PIN    D34
#define BTN_LEFT_PIN     D35
#define BTN_SETTINGS_PIN D36
#define BTN_BOOT_PIN     D39

// PMS5003 particulate-matter sensor (UART / Serial2)
#define PMS_RX_PIN D16
#define PMS_TX_PIN D17

// DHT22 temperature/humidity sensor
#define DHT_PIN D14

// I2C bus (LCD)
#define I2C_SDA_PIN D21
#define I2C_SCL_PIN D22
