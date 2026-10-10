# The build environment for Collider's release AppImage (used by release.sh).
#
# Ubuntu 24.04, so everything the AppImage bundles needs glibc 2.39 at most: the floor Neutron
# supports (Ubuntu 24.04, Fedora 40). An AppImage built on a newer distro carries that distro's
# WebKit and GLib, which needed glibc 2.44, and Collider would not start on Bazzite, Fedora 44 or
# Ubuntu 24.04 ("version `GLIBC_2.43' not found", GitHub issues Neutron#1, Collider#1).
FROM docker.io/library/ubuntu:24.04

ARG RUST_VERSION=1.99.0

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      build-essential pkg-config curl wget file git ca-certificates binutils \
      libwebkit2gtk-4.1-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev \
      nodejs npm squashfs-tools \
 && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}"
ENV PATH=/root/.cargo/bin:$PATH
