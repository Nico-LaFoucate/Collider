#!/usr/bin/env bash
# release.sh — build Collider's release AppImage and print the command that publishes it.
#
#   ./release.sh        -> dist/Collider_<ver>_amd64.AppImage + dist/SHA256SUMS
#
# Builds a fresh clone of HEAD (commit first) under /var/tmp, so the AppImage carries no path
# from this machine: rustc embeds source paths, which --remap-path-prefix rewrites, and the
# AppImage tooling records its build directory (the .DirIcon link). The build fails if the
# unpacked AppImage contains the home path anywhere. `neutron setup` downloads the AppImage and
# checks it against SHA256SUMS.
set -euo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"
WORK="${COLLIDER_RELEASE_WORK:-/var/tmp/collider-release}"
CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"
die() { echo "release.sh: $*" >&2; exit 1; }

git -C "$REPO" diff --quiet HEAD || die "uncommitted changes; the release is built from HEAD"
VER="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/src-tauri/Cargo.toml" | head -1)"
[ -n "$VER" ] || die "no version in src-tauri/Cargo.toml"

rm -rf "$WORK"; mkdir -p "$WORK"
git clone -q "$REPO" "$WORK/src"
cd "$WORK/src"
npm ci --no-audit --no-fund
# NO_STRIP: linuxdeploy cannot strip RELR binaries (see package.json build:appimage).
NO_STRIP=true \
RUSTFLAGS="--remap-path-prefix=$WORK/src=/build/collider --remap-path-prefix=$CARGO_DIR=/cargo" \
    npx tauri build --bundles appimage
IMG="$(ls "$WORK"/src/src-tauri/target/release/bundle/appimage/*.AppImage | head -1)"
[ -f "$IMG" ] || die "no AppImage was produced"

# Gate: nothing inside may name the home directory.
( cd "$WORK" && "$IMG" --appimage-extract >/dev/null )
if grep -rlF "$HOME/" "$WORK/squashfs-root" 2>/dev/null | head -3 | grep -q .; then
    die "the AppImage contains $HOME/: $(grep -rlF "$HOME/" "$WORK/squashfs-root" | head -3)"
fi
if find "$WORK/squashfs-root" -type l -lname "$HOME/*" | grep -q .; then
    die "the AppImage has links into $HOME/"
fi

mkdir -p "$REPO/dist"
NAME="Collider_${VER}_amd64.AppImage"
cp "$IMG" "$REPO/dist/$NAME"
( cd "$REPO/dist" && sha256sum "$NAME" > SHA256SUMS )
echo
echo "Built $NAME ($("$REPO/dist/$NAME" --version))."
echo "Publish:"
echo "  git tag v$VER && git push origin v$VER"
echo "  gh release create v$VER \"dist/$NAME\" dist/SHA256SUMS --title \"Collider $VER\" --notes \"...\""
