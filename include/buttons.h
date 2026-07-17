#pragma once

// ---------------------------------------------------------------------------
// Front-panel buttons (right / left)
//
// Each button is wired to a GPIO interrupt and debounced. setupButtons() must
// run once in setup(); loop() polls nextButtonEvent() and acts on any presses
// registered since the last call (one event per call).
//
// The Settings (GPIO36) and Boot (GPIO39) buttons are DISABLED: an ESP32
// silicon erratum makes those RTC-domain pins glitch on every WiFi modem-sleep
// wake, firing phantom interrupts constantly. Keeping WiFi power-save was
// preferred over those buttons; rewire them to non-RTC pins to bring them back.
// ---------------------------------------------------------------------------

enum ButtonEvent
{
  BTN_NONE,
  BTN_RIGHT,
  BTN_LEFT,
};

// Configure the button pins and attach their interrupts.
void setupButtons();

// Return one pending button press (clearing it), or BTN_NONE.
ButtonEvent nextButtonEvent();
