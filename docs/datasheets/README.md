# Datasheets

Manufacturer documents for the parts the firmware talks to. We keep a copy here
so the numbers in the code can always be traced back to their source, even if
the original link moves.

## Plantower PMS5003

| | |
| --- | --- |
| File | [`plantower-pms5003-manual-v2.3.pdf`](plantower-pms5003-manual-v2.3.pdf) |
| Title | PMS5003 series data manual |
| Version | V2.3, dated 2016-06-01 |
| Source | <https://cdn-shop.adafruit.com/product-files/3686/plantower-pms5003-manual_v2-3.pdf> |
| Retrieved | 2026-11-10 |
| SHA-256 | `5d191d7c5b4990e7c350dfcc53e8e7961f4525d84cf8d8e42ac48e7e16153688` |

Plantower doesn't offer a public download, so this copy comes from Adafruit,
a distributor of the sensor. Run `shasum -a 256` on the PDF to check it's the
same file.

### The values the firmware relies on

| What | Value | Page |
| --- | --- | --- |
| Output interval, active mode | 2.3 s when the air is steady; 200–800 ms when it changes fast | 4 |
| Response time | 1 s single, 10 s total | 3 |
| Warm-up after wake | "at least 30 seconds after the sensor wakeup from the sleep mode" | 10 |
| SET pin | high or floating runs the sensor, low puts it to sleep | 4 |
| Current | 100 mA active, 200 µA standby | 3 |
| MTTF | 3 years (test conditions not stated) | 3 |
| Consistency error (PM2.5) | ±10 µg/m³ at 0–100 µg/m³, ±10 % at 100–500 µg/m³ | 3 |
| Sleep cycling | endurance test toggles SET at 0.5 Hz for 72 h and passes | 9 |
| Frame layout | data 1–3 are CF=1, data 4–6 are "under atmospheric environment"; the firmware reads data 4–6 | 13 |
| UART | 9600 baud, 8N1, 32-byte frames starting `0x42 0x4d` | 13 |
| Sleep command (not used; we use the SET pin) | `42 4d e4 00 00 01 73` sleep, `42 4d e4 00 01 01 74` wake | 15 |

Two caveats from the manual worth knowing: Plantower only promises consistency
between its own sensors, not agreement with reference instruments (page 10),
and it rates the sensor for normal indoor air, asking for extra protection
outdoors (page 10).
