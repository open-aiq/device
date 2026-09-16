PIO      ?= pio
ESPTOOL  := $(PIO) pkg exec --package tool-esptoolpy -- esptool.py
PIO_ENV  := esp32doit-devkit-v1
BUILDDIR := .pio/build/$(PIO_ENV)

VERSION    ?=
NOTES_FILE ?= RELEASE_NOTES.md
PRERELEASE ?= true

RELEASE_BRANCH ?= main
DIST           ?= dist

.PHONY: help build upload monitor erase merge clean tls-inspect tls-verify tls-update release

## help: Show available commands
help:
	@echo "Available commands:"
	@echo ""
	@sed -n 's/^## //p' $(MAKEFILE_LIST) | column -t -s ':' | sed 's/^/  /'

## build: Compile the firmware
build:
	$(PIO) run -e $(PIO_ENV)

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
## release: Bump version, build artifacts, tag, and publish a GitHub release
release:
	@RELEASE_BRANCH="$(RELEASE_BRANCH)" DIST="$(DIST)" PIO_ENV="$(PIO_ENV)" BUILDDIR="$(BUILDDIR)" \
		bash scripts/release.sh
