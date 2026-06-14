PIO      := $(HOME)/.platformio/penv/bin/pio
PY       := $(HOME)/.platformio/penv/bin/python
ESPTOOL  := $(PY) $(HOME)/.platformio/packages/tool-esptoolpy/esptool.py
ENV      := esp32doit-devkit-v1
BUILDDIR := .pio/build/$(ENV)

VERSION    ?=
NOTES_FILE ?= RELEASE_NOTES.md
PRERELEASE ?= true

# Build the `gh release` notes flag: inline NOTES wins, else read from a file.
ifdef NOTES
  NOTES_ARG := --notes "$(NOTES)"
else
  NOTES_ARG := --notes-file "$(NOTES_FILE)"
endif

# Add --prerelease unless PRERELEASE=false.
ifeq ($(PRERELEASE),true)
  PRERELEASE_ARG := --prerelease
else
  PRERELEASE_ARG :=
endif

.PHONY: build merge tag release clean check-release-inputs

build:
	$(PIO) run -e $(ENV)

merge: build
	$(ESPTOOL) --chip esp32 merge_bin -o $(BUILDDIR)/merged-firmware.bin \
		--flash_mode dio --flash_freq 40m --flash_size 4MB \
		0x1000  $(BUILDDIR)/bootloader.bin \
		0x8000  $(BUILDDIR)/partitions.bin \
		0x10000 $(BUILDDIR)/firmware.bin

# Validate the per-release inputs up front, so we never push a git tag and then
# fail on a missing version or notes file half-way through a release.
check-release-inputs:
	@test -n "$(VERSION)" || { \
		echo "ERROR: VERSION is required, e.g. make release VERSION=v0.1.0"; exit 1; }
ifndef NOTES
	@test -f "$(NOTES_FILE)" || { \
		echo "ERROR: notes file '$(NOTES_FILE)' not found."; \
		echo "       Create it, pass NOTES_FILE=path, or NOTES=\"...\"."; exit 1; }
endif

tag: check-release-inputs
	git tag -a $(VERSION) -m "Release $(VERSION)"
	git push origin $(VERSION)

release: check-release-inputs merge tag
	gh release create $(VERSION) \
		$(BUILDDIR)/firmware.bin \
		$(BUILDDIR)/bootloader.bin \
		$(BUILDDIR)/partitions.bin \
		$(BUILDDIR)/merged-firmware.bin \
		$(PRERELEASE_ARG) \
		--title "$(VERSION)" \
		$(NOTES_ARG)

# Remove build artifacts.
clean:
	$(PIO) run -e $(ENV) -t clean
