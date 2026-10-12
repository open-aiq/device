HOST_TARGET ?= aarch64-apple-darwin
CARGO       ?= cargo

.PHONY: format format-check test lint check-sh1106 check-ssd1306 build-sh1106 build-ssd1306 verify

format:
	$(CARGO) fmt --all

format-check:
	$(CARGO) fmt --all -- --check

test:
	$(CARGO) test --lib --target $(HOST_TARGET)

lint:
	$(CARGO) clippy --lib --target $(HOST_TARGET) -- -D warnings
	OLED_CONTROLLER=sh1106 $(CARGO) clippy -- -D warnings
	OLED_CONTROLLER=ssd1306 $(CARGO) clippy -- -D warnings

check-sh1106:
	OLED_CONTROLLER=sh1106 $(CARGO) check

check-ssd1306:
	OLED_CONTROLLER=ssd1306 $(CARGO) check

build-sh1106:
	OLED_CONTROLLER=sh1106 $(CARGO) build --release

build-ssd1306:
	OLED_CONTROLLER=ssd1306 $(CARGO) build --release

verify: format-check test lint check-sh1106 check-ssd1306
