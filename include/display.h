#pragma once

#include <Arduino.h>
#include <LiquidCrystal_I2C.h>

// ---------------------------------------------------------------------------
// Front-panel display
//
// Wraps the 16x2 I2C LCD so the rest of the firmware talks to a small,
// hardware-agnostic API instead of the LiquidCrystal_I2C driver directly.
// Swapping to a different panel later means changing only this module, and the
// button-driven screen logic (next/previous screen, settings, home) has a place
// to live here rather than leaking into main.cpp.
// ---------------------------------------------------------------------------

class Display
{
public:
  // `i2cAddress` is the LCD's I2C address; `cols`/`rows` its character size.
  Display(uint8_t i2cAddress, uint8_t cols, uint8_t rows);

  void begin(); // power up the panel and turn on the backlight

  // Clear the screen and print one message starting at the top-left.
  void showMessage(const String &msg);

  // Render both rows. Each line is truncated to the panel width and padded
  // with spaces to overwrite whatever was there before — no clear() between
  // refreshes, so updates are flicker-free and can't leave stale characters
  // behind (the cause of the "congested text" garbage).
  void showLines(const String &line1, const String &line2);

private:
  // Write one padded/truncated line at the given row.
  void writeRow(uint8_t row, const String &text);

  LiquidCrystal_I2C lcd;
  uint8_t cols, rows;
};
