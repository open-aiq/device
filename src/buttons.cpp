#include "buttons.h"

#include <Arduino.h>

#include "pins.h"

// NOTE: the Settings (GPIO36) and Boot (GPIO39) buttons are intentionally not
// wired up — see the erratum note in buttons.h.

static volatile bool rightPressed = false;
static volatile bool leftPressed = false;

static volatile uint32_t lastRightInterrupt = 0;
static volatile uint32_t lastLeftInterrupt = 0;

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

void setupButtons()
{
  pinMode(BTN_RIGHT_PIN, INPUT);
  pinMode(BTN_LEFT_PIN, INPUT);

  attachInterrupt(
      digitalPinToInterrupt(BTN_RIGHT_PIN),
      rightISR,
      FALLING);

  attachInterrupt(
      digitalPinToInterrupt(BTN_LEFT_PIN),
      leftISR,
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

  return BTN_NONE;
}
