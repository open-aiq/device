PIO      ?= pio
ESPTOOL  := $(PIO) pkg exec --package tool-esptoolpy -- esptool.py
PIO_ENV  := esp32doit-devkit-v1
BUILDDIR := .pio/build/$(PIO_ENV)

MOCK_PROVISIONING ?= 0
WIPE_CONFIG       ?= 0
BUILD_FLAGS       = -DCORE_DEBUG_LEVEL=4 -DMOCK_APP_PROVISIONING=$(MOCK_PROVISIONING) -DWIPE_CONFIG_ON_BOOT=$(WIPE_CONFIG)

VERSION    ?=
NOTES_FILE ?= RELEASE_NOTES.md
PRERELEASE ?= true

RELEASE_BRANCH ?= main
DIST           ?= dist

.PHONY: help build build-mock-provision build-reset upload monitor erase merge clean libs-outdated tls-inspect tls-verify tls-update release

## help: Show available commands
help:
	@echo "Available commands:"
	@echo ""
	@sed -n 's/^## //p' $(MAKEFILE_LIST) | column -t -s ':' | sed 's/^/  /'

## build: Compile the firmware
build:
	PLATFORMIO_BUILD_FLAGS="$(BUILD_FLAGS)" $(PIO) run -e $(PIO_ENV)

## build-mock-provision: Build with provisioning values from secrets.h
build-mock-provision: MOCK_PROVISIONING=1
build-mock-provision: build

## build-reset: Build firmware that clears saved provisioning on every boot
build-reset: WIPE_CONFIG=1
build-reset: build

## upload: Build and upload firmware to an attached device
upload:
	$(PIO) run -e $(PIO_ENV) -t upload

## monitor: Open the serial monitor
monitor:
	$(PIO) device monitor

## erase: Erase the attached device's flash
erase:
	$(PIO) run -e $(PIO_ENV) -t erase

## libs-outdated: Check installed libraries for newer versions
libs-outdated:
	$(PIO) pkg outdated -e $(PIO_ENV)

## tls-inspect: Inspect the backend's live TLS certificate chain
tls-inspect:
	@bash scripts/tls_certificate.sh inspect "$(BACKEND_HOST)" "$(TLS_PORT)"

## tls-verify: Verify the backend against the embedded root CA
tls-verify:
	@bash scripts/tls_certificate.sh verify "$(BACKEND_HOST)" "$(TLS_PORT)" "$(TLS_CA_FILE)"

## tls-update: Replace the root CA after validating TLS_CA_URL and TLS_CA_SHA256
tls-update:
	@test -n "$(TLS_CA_URL)" || { echo "ERROR: TLS_CA_URL is required."; exit 1; }
	@test -n "$(TLS_CA_SHA256)" || { echo "ERROR: TLS_CA_SHA256 is required."; exit 1; }
	@bash scripts/tls_certificate.sh update "$(TLS_CA_FILE)" "$(TLS_CA_URL)" "$(TLS_CA_SHA256)"

## merge: Build and create a single flashable firmware image
merge: build
	$(ESPTOOL) --chip esp32 merge_bin -o $(BUILDDIR)/merged-firmware.bin \
		--flash_mode dio --flash_freq 40m --flash_size 4MB \
		0x1000  $(BUILDDIR)/bootloader.bin \
		0x8000  $(BUILDDIR)/partitions.bin \
		0x10000 $(BUILDDIR)/firmware.bin
## clean: Remove build artifacts
clean:
	$(PIO) run -e $(PIO_ENV) -t clean

##.
## release: Build, tag, and publish — make release [version=major|minor|patch|1.2.3|1.2.3-rc.1]
release:
	@RELEASE_BRANCH="$(RELEASE_BRANCH)" RELEASE_VERSION="$(version)" DIST="$(DIST)" PIO_ENV="$(PIO_ENV)" BUILDDIR="$(BUILDDIR)" \
		bash scripts/release.sh
