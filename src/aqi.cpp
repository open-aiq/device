#include "aqi.h"

#include <math.h>

// One row of an AQI lookup table: concentration range [cLow, cHigh] maps
// linearly onto index range [iLow, iHigh].
struct Breakpoint
{
  float cLow, cHigh;
  int iLow, iHigh;
  AqiCategory category;
};

// EPA breakpoints, 2024 revision (PM2.5 24-hour). PM10 unchanged from prior.
// Source: EPA AQS aqi_breakpoints table.
static const Breakpoint PM25_BP[] = {
    {0.0f, 9.0f, 0, 50, AQI_GOOD},
    {9.1f, 35.4f, 51, 100, AQI_MODERATE},
    {35.5f, 55.4f, 101, 150, AQI_UNHEALTHY_SENSITIVE},
    {55.5f, 125.4f, 151, 200, AQI_UNHEALTHY},
    {125.5f, 225.4f, 201, 300, AQI_VERY_UNHEALTHY},
    {225.5f, 325.4f, 301, 500, AQI_HAZARDOUS},
};

static const Breakpoint PM10_BP[] = {
    {0.0f, 54.0f, 0, 50, AQI_GOOD},
    {55.0f, 154.0f, 51, 100, AQI_MODERATE},
    {155.0f, 254.0f, 101, 150, AQI_UNHEALTHY_SENSITIVE},
    {255.0f, 354.0f, 151, 200, AQI_UNHEALTHY},
    {355.0f, 424.0f, 201, 300, AQI_VERY_UNHEALTHY},
    {425.0f, 604.0f, 301, 500, AQI_HAZARDOUS},
};

// Map one concentration to its sub-index via the EPA piecewise-linear formula.
// Concentrations above the table top are "beyond the AQI" and are capped at the
// table's maximum index. `category` receives the band that was used.
static int subIndex(float c, const Breakpoint *table, size_t count, AqiCategory &category)
{
  const Breakpoint &top = table[count - 1];
  if (c > top.cHigh)
  {
    category = top.category;
    return top.iHigh;
  }

  for (size_t i = 0; i < count; i++)
  {
    const Breakpoint &b = table[i];
    if (c <= b.cHigh)
    {
      category = b.category;
      float aqi = (float)(b.iHigh - b.iLow) / (b.cHigh - b.cLow) * (c - b.cLow) + b.iLow;
      return (int)lroundf(aqi);
    }
  }

  category = table[0].category; // c below the table (negative); treat as best
  return table[0].iLow;
}

AqiResult computeUsAqi(uint16_t pm25, uint16_t pm10)
{
  // Inputs are already integer ug/m3, so EPA's truncation step is a no-op.
  AqiCategory cat25, cat10;
  int i25 = subIndex((float)pm25, PM25_BP, sizeof(PM25_BP) / sizeof(PM25_BP[0]), cat25);
  int i10 = subIndex((float)pm10, PM10_BP, sizeof(PM10_BP) / sizeof(PM10_BP[0]), cat10);

  AqiResult r;
  if (i25 >= i10) // tie goes to PM2.5
  {
    r.aqi = i25;
    r.category = cat25;
    r.dominant = AQI_PM25;
  }
  else
  {
    r.aqi = i10;
    r.category = cat10;
    r.dominant = AQI_PM10;
  }
  return r;
}

const char *aqiCategoryName(AqiCategory category)
{
  switch (category)
  {
  case AQI_GOOD:
    return "Good";
  case AQI_MODERATE:
    return "Moderate";
  case AQI_UNHEALTHY_SENSITIVE:
    return "Unhealthy for Sensitive Groups";
  case AQI_UNHEALTHY:
    return "Unhealthy";
  case AQI_VERY_UNHEALTHY:
    return "Very Unhealthy";
  case AQI_HAZARDOUS:
    return "Hazardous";
  }
  return "Unknown";
}
