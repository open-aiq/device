#!/usr/bin/env bash
#
# release.sh — cut a new firmware release.
#
set -euo pipefail

RELEASE_BRANCH="${RELEASE_BRANCH:-main}"
DIST="${DIST:-dist}"
PIO_ENV="${PIO_ENV:-esp32doit-devkit-v1}"
BUILDDIR="${BUILDDIR:-.pio/build/$PIO_ENV}"

die() {
	echo "error: $*" >&2
	exit 1
}

command -v gh >/dev/null 2>&1 || die "gh CLI not found: https://cli.github.com"
gh auth status >/dev/null 2>&1 || die "gh not authenticated. Run: gh auth login"

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = "$RELEASE_BRANCH" ] ||
	die "releases must be cut from '$RELEASE_BRANCH' (you are on '$branch')"

git diff-index --quiet HEAD -- ||
	die "working tree is dirty; commit or stash changes first"

git fetch --tags --quiet
current="$(git tag --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -n 1)"

current="${current:-v0.0.0}"
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

git rev-parse "$next" >/dev/null 2>&1 && die "tag $next already exists"

read -rp "Release $next from $branch? [y/N]: " confirm
case "$confirm" in
	y|Y) ;;
	*)   die "aborted" ;;
esac

echo "==> Building firmware for $next"
make merge

[ "$DIST" != "/" ] && [ -n "$DIST" ] || die "unsafe DIST value '$DIST'"
rm -rf "$DIST"
mkdir -p "$DIST"

for artifact in firmware.bin bootloader.bin partitions.bin merged-firmware.bin; do
	[ -f "$BUILDDIR/$artifact" ] || die "missing build artifact: $BUILDDIR/$artifact"
	cp "$BUILDDIR/$artifact" "$DIST/${artifact%.bin}-${next}.bin"
done

(
	cd "$DIST"
	if command -v shasum >/dev/null 2>&1; then
		shasum -a 256 * >SHA256SUMS
	else
		sha256sum * >SHA256SUMS
	fi
)

echo "==> Tagging and pushing $next"
git tag -a "$next" -m "Release $next"
git push origin "$next"

echo "==> Creating GitHub release"
gh release create "$next" \
	--title "$next" \
	--target "$branch" \
	--generate-notes \
	"$DIST"/*

echo "==> Done: $next published (artifacts in $DIST/)."
