#include "display.h"

Display::Display(uint8_t i2cAddress, uint8_t cols, uint8_t rows)
    : lcd(i2cAddress, cols, rows), cols(cols), rows(rows)
{
}

void Display::begin()
{
  lcd.init();
  lcd.backlight();
}

void Display::showMessage(const String &msg)
{
  showLines(msg, "");
}

void Display::writeRow(uint8_t row, const String &text)
{
  // Fixed-width write: truncate to the panel width, then pad with spaces so
  // every cell of the row is overwritten. HD44780 panels don't wrap or clear
  // by themselves — printing variable-length strings without padding leaves
  // characters from the previous, longer text on screen.
  String line = text.substring(0, cols);
  while (line.length() < cols)
  {
    line += ' ';
  }
  lcd.setCursor(0, row);
  lcd.print(line);
}

void Display::showLines(const String &line1, const String &line2)
{
  writeRow(0, line1);
  if (rows > 1)
  {
    writeRow(1, line2);
  }
}
