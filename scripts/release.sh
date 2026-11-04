#!/usr/bin/env bash
#
# release.sh — cut a firmware release.
#
# Checks the Open AIQ version registry, runs `make verify`, sets the crate
# version (reported by the device as FIRMWARE_VERSION), builds images for both
# OLED controllers, then commits, tags, pushes, and publishes a GitHub release.
#
# Intended to be run via `make release [version=major|minor|patch|x.y.z]`.
# Requires an authenticated `gh` CLI, the ESP toolchain, and espflash.
#
set -euo pipefail

RELEASE_BRANCH="${RELEASE_BRANCH:-main}"
DIST="${DIST:-dist}"
CONTROLLERS="${CONTROLLERS:-sh1106 ssd1306}"
TARGET="xtensa-esp32s3-espidf"
ELF="target/$TARGET/release/device_rust"
PARTITIONS="partitions.csv"

die() {
	echo "error: $*" >&2
	exit 1
}

# --- Preflight ---------------------------------------------------------------

command -v gh >/dev/null 2>&1 || die "gh CLI not found: https://cli.github.com"
gh auth status >/dev/null 2>&1 || die "gh not authenticated. Run: gh auth login"
command -v espflash >/dev/null 2>&1 || die "espflash not found (cargo install espflash)"

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = "$RELEASE_BRANCH" ] ||
	die "releases must be cut from '$RELEASE_BRANCH' (you are on '$branch')"
git diff-index --quiet HEAD -- ||
	die "working tree is dirty; commit or stash changes first"

# Mock builds embed Wi-Fi and device credentials; they must never be released.
if env | grep -q '^MOCK_'; then
	die "MOCK_* variables are set; unset them so no credentials are built into the release"
fi

# --- Determine the next version ----------------------------------------------

git fetch --tags --quiet
current="v$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' Cargo.toml | head -1)"
choice="${1:-${RELEASE_VERSION:-}}"
if [ -z "$choice" ]; then
	read -rp "Current version: $current
Bump type or version [major/minor/patch/x.y.z]: " choice
fi

case "$choice" in
	major | minor | patch)
		version="${current#v}"
		major="${version%%.*}"
		rest="${version#*.}"
		minor="${rest%%.*}"
		patch="${rest#*.}"
		patch="${patch%%-*}"
		case "$choice" in
			major) major=$((major + 1)); minor=0; patch=0 ;;
			minor) minor=$((minor + 1)); patch=0 ;;
			patch) patch=$((patch + 1)) ;;
		esac
		next="v${major}.${minor}.${patch}"
		;;
	*)
		next="v${choice#v}"
		[[ "$next" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
			die "invalid version '$choice' (expected major, minor, patch, or a semantic version)"
		;;
esac
version="${next#v}"

git rev-parse "$next" >/dev/null 2>&1 && die "tag $next already exists"

# Major and minor versions must be registered at docs.air-iq.net/versions.json
# (open-aiq/docs src/data/versions.json) before they can be released.
bash scripts/check-version-registry.sh firmware "$next"

read -rp "Release firmware $next from $branch? [y/N]: " confirm
case "$confirm" in
	y | Y) ;;
	*) die "aborted" ;;
esac

# --- Verify and set the version ----------------------------------------------

echo "==> Verifying"
make verify

echo "==> Setting crate version to $version"
python3 - "$version" <<'PY'
import re, sys
path = "Cargo.toml"
text = open(path).read()
# Only the [package] version (the first top-level `version =` line).
text, count = re.subn(r'(?m)^version\s*=\s*"[^"]*"', f'version = "{sys.argv[1]}"', text, count=1)
if count != 1:
    sys.exit("no package version line in Cargo.toml")
open(path, "w").write(text)
PY
grep -q "^version = \"$version\"" Cargo.toml || die "could not set the version in Cargo.toml"

# --- Build images for every OLED controller ----------------------------------

rm -rf "$DIST"
mkdir -p "$DIST"
for controller in $CONTROLLERS; do
	echo "==> Building $next for $controller"
	OLED_CONTROLLER="$controller" cargo build --release
	bootloader="$(ls -t target/$TARGET/release/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin 2>/dev/null | head -1)"
	[ -n "$bootloader" ] || die "bootloader.bin not found in the esp-idf-sys build output"

	# OTA app image: what the device downloads for an over-the-air update.
	espflash save-image --chip esp32s3 --partition-table "$PARTITIONS" \
		"$ELF" "$DIST/airmonitor-$next-$controller-ota.bin"
	# Full flash image (bootloader + partition table + app) for a first USB flash at 0x0.
	espflash save-image --chip esp32s3 --merge --skip-padding \
		--bootloader "$bootloader" --partition-table "$PARTITIONS" \
		"$ELF" "$DIST/airmonitor-$next-$controller-full.bin"
done

(
	cd "$DIST"
	if command -v shasum >/dev/null 2>&1; then
		shasum -a 256 -- *.bin >SHA256SUMS
	else
		sha256sum -- *.bin >SHA256SUMS
	fi
)

# --- Commit, tag, and publish --------------------------------------------------

echo "==> Committing and tagging $next"
git add Cargo.toml Cargo.lock
git commit -m "Release $next"
git tag -a "$next" -m "Release $next"
git push --atomic origin "$branch" "$next"

echo "==> Creating GitHub release"
gh release create "$next" \
	--title "$next" \
	--target "$branch" \
	--generate-notes \
	"$DIST"/*

echo "==> Refreshing the releases page on docs.air-iq.net"
gh workflow run deploy.yml --repo open-aiq/docs >/dev/null 2>&1 ||
	echo "warning: could not start the docs deploy; run it from open-aiq/docs Actions so the release is listed" >&2

echo "==> Done: firmware $next published (images in $DIST/)."
