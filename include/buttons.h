#pragma once

// ---------------------------------------------------------------------------
// Front-panel buttons (right / left / settings / boot)
//
// Each button is wired to a GPIO interrupt and debounced. setupButtons() must
// run once in setup(); loop() polls nextButtonEvent() and acts on any presses
// registered since the last call (one event per call, oldest first).
// ---------------------------------------------------------------------------

enum ButtonEvent
{
  BTN_NONE,
  BTN_RIGHT,
  BTN_LEFT,
  BTN_SETTINGS,
  BTN_BOOT,
};

// Configure the button pins and attach their interrupts.
void setupButtons();

// Return one pending button press (clearing it), or BTN_NONE.
ButtonEvent nextButtonEvent();
