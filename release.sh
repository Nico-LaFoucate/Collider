#!/usr/bin/env bash
# release.sh — build Collider's release AppImage and print the command that publishes it.
#
#   ./release.sh        -> dist/Collider_<ver>_amd64.AppImage + dist/SHA256SUMS
#
# The build runs in an Ubuntu 24.04 container (release.Containerfile, rootless podman), so the
# AppImage needs glibc 2.39 at most; the build fails if anything inside needs newer. Built on the
# host instead (a rolling distro), it bundled a WebKit that needed glibc 2.44 and did not start on
# Bazzite, Fedora 44 or Ubuntu 24.04. COLLIDER_RELEASE_NATIVE=1 builds on the host anyway, for a
# local test; the glibc gate still applies.
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
DIST="${COLLIDER_DIST:-$REPO/dist}"
HOST_HOME="${COLLIDER_HOST_HOME:-$HOME}"     # the home path that must not leak into the AppImage
GLIBC_FLOOR=2.39
IMAGE=collider-release:ubuntu24.04
die() { echo "release.sh: $*" >&2; exit 1; }

git -C "$REPO" diff --quiet HEAD || die "uncommitted changes; the release is built from HEAD"

if [ -z "${COLLIDER_IN_CONTAINER:-}" ] && [ -z "${COLLIDER_RELEASE_NATIVE:-}" ]; then
    command -v podman >/dev/null || die "podman is needed (the release is built on Ubuntu 24.04)"
    podman build -q -t "$IMAGE" -f "$REPO/release.Containerfile" "$REPO" >/dev/null
    mkdir -p "$DIST"
    # ipv4first: Ubuntu's Node 18 tries IPv6 first with no fallback, and IPv6 is refused from
    # inside the container (npm ci failed with ECONNREFUSED 2606:4700::...).
    exec podman run --rm -e COLLIDER_IN_CONTAINER=1 -e COLLIDER_DIST=/out \
        -e COLLIDER_HOST_HOME="$HOME" -e APPIMAGE_EXTRACT_AND_RUN=1 \
        -e NODE_OPTIONS=--dns-result-order=ipv4first \
        -v "$REPO:/src:ro" -v "$DIST:/out" "$IMAGE" bash /src/release.sh
fi
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
BUILT="$(ls "$WORK"/src/src-tauri/target/release/bundle/appimage/*.AppImage | head -1)"
[ -f "$BUILT" ] || die "no AppImage was produced"

# Leave the Wayland client libraries to the system. linuxdeploy bundles Ubuntu 24.04's
# libwayland-client/-egl/-cursor, and the system's Mesa EGL then runs against that older copy:
# on Fedora 44 WebKit aborted with "Could not create default EGL display: EGL_BAD_PARAMETER" and
# the window stayed black (in a Fedora 44 container; Fedora's own WebKit rendered there, and so
# did this AppImage with the three removed). libwayland is backward compatible and every desktop
# has it (GTK depends on it on X11 too). Tauri passes no options through to linuxdeploy, so the
# AppImage is repacked: the original runtime, then the same squashfs settings (zstd, 128 KiB).
OFFSET="$("$BUILT" --appimage-offset)"
( cd "$WORK" && "$BUILT" --appimage-extract >/dev/null )
rm -f "$WORK"/squashfs-root/usr/lib/libwayland-client.so* \
      "$WORK"/squashfs-root/usr/lib/libwayland-egl.so* \
      "$WORK"/squashfs-root/usr/lib/libwayland-cursor.so*
IMG="$WORK/Collider.AppImage"
head -c "$OFFSET" "$BUILT" > "$WORK/runtime"
mksquashfs "$WORK/squashfs-root" "$WORK/payload.squashfs" -comp zstd -b 131072 -root-owned \
    -noappend -quiet >/dev/null
cat "$WORK/runtime" "$WORK/payload.squashfs" > "$IMG"
chmod 755 "$IMG"

# The gates below check the repacked AppImage, extracted again.
rm -rf "$WORK/squashfs-root"
( cd "$WORK" && "$IMG" --appimage-extract >/dev/null )
if ls "$WORK"/squashfs-root/usr/lib/libwayland-{client,egl,cursor}.so* >/dev/null 2>&1; then
    die "the AppImage still bundles a Wayland client library"
fi

# Gate: nothing inside may name the home directory.
# Inside the container that is the host's home (passed in); the container's own /root is not
# anyone's, and "/root/" also occurs in linuxdeploy's prebuilt AppRun (/opt/rh/devtoolset-9/root/).
for h in "$HOST_HOME"; do
    if grep -rlF "$h/" "$WORK/squashfs-root" 2>/dev/null | head -3 | grep -q .; then
        die "the AppImage contains $h/: $(grep -rlF "$h/" "$WORK/squashfs-root" | head -3)"
    fi
    if find "$WORK/squashfs-root" -type l -lname "$h/*" | grep -q .; then
        die "the AppImage has links into $h/"
    fi
done

# Gate: everything inside runs on the glibc floor, like Mud Hut's release.
too_new="$(find "$WORK/squashfs-root" -type f -print0 | while IFS= read -r -d '' f; do
    head -c4 "$f" 2>/dev/null | grep -q $'\x7fELF' || continue
    v="$(objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)"
    [ -n "$v" ] && [ "$(printf '%s\n%s\n' "$v" "$GLIBC_FLOOR" | sort -V | tail -1)" != "$GLIBC_FLOOR" ] \
        && echo "  glibc $v  ${f#"$WORK/squashfs-root/"}"
done)"
[ -z "$too_new" ] || die "files in the AppImage need a glibc newer than $GLIBC_FLOOR:
$too_new"

mkdir -p "$DIST"
NAME="Collider_${VER}_amd64.AppImage"
cp "$IMG" "$DIST/$NAME"
( cd "$DIST" && sha256sum "$NAME" > SHA256SUMS )
echo
echo "Built $NAME, needs glibc $GLIBC_FLOOR at most."
echo "Publish:"
echo "  git tag v$VER && git push origin v$VER"
echo "  gh release create v$VER \"dist/$NAME\" dist/SHA256SUMS --title \"Collider $VER\" --notes \"...\""
