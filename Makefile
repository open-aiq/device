HOST_TARGET ?= aarch64-apple-darwin
CARGO       ?= cargo
ESPFLASH    ?= espflash

# Firmware build configuration.
OLED_CONTROLLER ?= sh1106
DEVICE_TARGET   ?= xtensa-esp32s3-espidf
FIRMWARE        ?= target/$(DEVICE_TARGET)/release/device_rust

# OTA partition table and the ESP-IDF bootloader built with rollback support.
# espflash otherwise substitutes its own single-app table and bootloader.
PARTITIONS  ?= partitions.csv
# Newest build wins: stale esp-idf-sys output dirs may hold a bootloader without rollback.
BOOTLOADER  ?= $(shell ls -t target/$(DEVICE_TARGET)/release/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin 2>/dev/null | head -1)
IMAGE       ?= target/$(DEVICE_TARGET)/release/device_rust.bin
FLASH_ARGS   = --partition-table $(PARTITIONS) $(if $(BOOTLOADER),--bootloader $(BOOTLOADER))

# Serial port for flashing and monitoring; espflash auto-detects it when unset.
PORT ?=
PORT_ARG := $(if $(PORT),--port $(PORT))

.PHONY: help format\:fix format\:check test lint\:check lint\:fix check\:sh1106 check\:ssd1306 verify build build\:sh1106 build\:ssd1306 flash flash\:sh1106 flash\:ssd1306 image board\:info monitor release

## help: Show available commands
help:
	@echo "Available commands:"
	@echo ""
	@sed -n 's/^## //p' $(MAKEFILE_LIST) | awk '{ i = index($$0, ": "); if (i) printf "  %-20s %s\n", substr($$0, 1, i - 1), substr($$0, i + 2); else print "" }'

## format:fix: Format all Rust sources
format\:fix:
	$(CARGO) fmt --all

## format:check: Check formatting without rewriting files
format\:check:
	$(CARGO) fmt --all -- --check

## test: Run host library tests — make test [HOST_TARGET=x86_64-unknown-linux-gnu]
test:
	$(CARGO) test --lib --target $(HOST_TARGET)

## lint:check: Run Clippy on the host library and both OLED firmware builds
lint\:check:
	$(CARGO) clippy --lib --target $(HOST_TARGET) -- -D warnings
	OLED_CONTROLLER=sh1106 $(CARGO) clippy -- -D warnings
	OLED_CONTROLLER=ssd1306 $(CARGO) clippy -- -D warnings

## lint:fix: Apply Clippy's automatic fixes to the host library and firmware
lint\:fix:
	$(CARGO) clippy --fix --allow-dirty --allow-staged --lib --target $(HOST_TARGET)
	OLED_CONTROLLER=$(OLED_CONTROLLER) $(CARGO) clippy --fix --allow-dirty --allow-staged

## check:sh1106: Type-check the firmware for the SH1106 OLED
check\:sh1106:
	OLED_CONTROLLER=sh1106 $(CARGO) check

## check:ssd1306: Type-check the firmware for the SSD1306 OLED
check\:ssd1306:
	OLED_CONTROLLER=ssd1306 $(CARGO) check

## verify: Run format:check, test, lint:check, and both OLED checks
verify:
	@$(MAKE) --no-print-directory format:check
	@$(MAKE) --no-print-directory test
	@$(MAKE) --no-print-directory lint:check
	@$(MAKE) --no-print-directory check:sh1106
	@$(MAKE) --no-print-directory check:ssd1306

## build: Build release firmware — make build [OLED_CONTROLLER=sh1106|ssd1306]
build:
	OLED_CONTROLLER=$(OLED_CONTROLLER) $(CARGO) build --release

## build:sh1106: Build release firmware for the SH1106 OLED
build\:sh1106:
	@$(MAKE) build OLED_CONTROLLER=sh1106

## build:ssd1306: Build release firmware for the SSD1306 OLED
build\:ssd1306:
	@$(MAKE) build OLED_CONTROLLER=ssd1306

## flash: Build, flash, and monitor the device — make flash [OLED_CONTROLLER=sh1106|ssd1306] [PORT=/dev/cu.usbmodem*]
flash: build
	$(ESPFLASH) flash --monitor $(PORT_ARG) $(FLASH_ARGS) $(FIRMWARE)

## flash:sh1106: Build and flash firmware for the SH1106 OLED — make flash:sh1106 [PORT=...]
flash\:sh1106:
	@$(MAKE) flash OLED_CONTROLLER=sh1106

## flash:ssd1306: Build and flash firmware for the SSD1306 OLED — make flash:ssd1306 [PORT=...]
flash\:ssd1306:
	@$(MAKE) flash OLED_CONTROLLER=ssd1306

## image: Build the OTA app image (.bin) and print its size and SHA-256 for a release
image: build
	$(ESPFLASH) save-image --chip esp32s3 --partition-table $(PARTITIONS) $(FIRMWARE) $(IMAGE)
	@echo "size:   $$(wc -c < $(IMAGE) | tr -d ' ')"
	@echo "sha256: $$(shasum -a 256 $(IMAGE) | cut -d ' ' -f 1)"

## board:info: Print chip and flash size of the attached board (read-only)
board\:info:
	$(ESPFLASH) board-info $(PORT_ARG)

## monitor: Open the serial monitor without flashing — make monitor [PORT=...]
monitor:
	$(ESPFLASH) monitor $(PORT_ARG)

## release: Verify, build, tag, and publish firmware — make release [version=major|minor|patch|x.y.z]
release:
	@RELEASE_VERSION="$(version)" bash scripts/release.sh
