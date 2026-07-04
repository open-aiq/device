#include "buttons.h"

#include <Arduino.h>

#include "pins.h"

static volatile bool rightPressed = false;
static volatile bool leftPressed = false;
static volatile bool settingsPressed = false;
static volatile bool bootPressed = false;

static volatile uint32_t lastRightInterrupt = 0;
static volatile uint32_t lastLeftInterrupt = 0;
static volatile uint32_t lastSettingsInterrupt = 0;
static volatile uint32_t lastBootInterrupt = 0;

static const uint32_t DEBOUNCE_MS = 150;

static void IRAM_ATTR rightISR()
{
  uint32_t now = millis();

  if (now - lastRightInterrupt > DEBOUNCE_MS)
  {
    rightPressed = true;
    lastRightInterrupt = now;
  }
}

static void IRAM_ATTR leftISR()
{
  uint32_t now = millis();

  if (now - lastLeftInterrupt > DEBOUNCE_MS)
  {
    leftPressed = true;
    lastLeftInterrupt = now;
  }
}

static void IRAM_ATTR settingsISR()
{
  uint32_t now = millis();

  if (now - lastSettingsInterrupt > DEBOUNCE_MS)
  {
    settingsPressed = true;
    lastSettingsInterrupt = now;
  }
}

static void IRAM_ATTR bootISR()
{
  uint32_t now = millis();

  if (now - lastBootInterrupt > DEBOUNCE_MS)
  {
    bootPressed = true;
    lastBootInterrupt = now;
  }
}

void setupButtons()
{
  pinMode(BTN_RIGHT_PIN, INPUT);
  pinMode(BTN_LEFT_PIN, INPUT);
  pinMode(BTN_SETTINGS_PIN, INPUT);
  pinMode(BTN_BOOT_PIN, INPUT);

  attachInterrupt(
      digitalPinToInterrupt(BTN_RIGHT_PIN),
      rightISR,
      FALLING);

  attachInterrupt(
      digitalPinToInterrupt(BTN_LEFT_PIN),
      leftISR,
      FALLING);

  attachInterrupt(
      digitalPinToInterrupt(BTN_SETTINGS_PIN),
      settingsISR,
      FALLING);

  attachInterrupt(
      digitalPinToInterrupt(BTN_BOOT_PIN),
      bootISR,
      FALLING);
}

ButtonEvent nextButtonEvent()
{
  if (rightPressed)
  {
    rightPressed = false;
    log_i("RIGHT");
    return BTN_RIGHT;
  }

  if (leftPressed)
  {
    leftPressed = false;
    log_i("LEFT");
    return BTN_LEFT;
  }

  if (settingsPressed)
  {
    settingsPressed = false;
    log_i("SETTINGS");
    return BTN_SETTINGS;
  }

  if (bootPressed)
  {
    bootPressed = false;
    log_i("BOOT");
    return BTN_BOOT;
  }

  return BTN_NONE;
}
