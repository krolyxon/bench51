#!/bin/sh
# Installs bench51 (the bench51-gui desktop app and the bench51 terminal app)
# from the GitHub releases, on Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/krolyxon/bench51/master/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/krolyxon/bench51/master/install.sh | sh -s -- --dir /usr/local/bin
#
# Run with --help for all options. On Windows, use install.ps1.

set -eu

REPO=krolyxon/bench51
VERSION=${BENCH51_VERSION:-latest}
DIR=${BENCH51_INSTALL_DIR:-$HOME/.local/bin}
ARCHIVE=
DESKTOP=1
UNINSTALL=0

DATA=${XDG_DATA_HOME:-$HOME/.local/share}
APPS=$DATA/applications
ICONS=$DATA/icons/hicolor/scalable/apps

usage() {
	cat <<EOF
Install bench51 from https://github.com/$REPO/releases

Usage: install.sh [options]

  --version <tag>   release to install, e.g. v0.2.0 (default: latest)
  --dir <path>      where to put the binaries (default: ~/.local/bin)
  --archive <file>  install from a downloaded bench51-<target>.tar.gz
  --no-desktop      on Linux, skip the application menu entry and icon
  --uninstall       remove bench51 from --dir and the menu
  -h, --help        show this help

The environment variables BENCH51_VERSION and BENCH51_INSTALL_DIR set the
defaults for --version and --dir.
EOF
}

say() { printf 'bench51: %s\n' "$*"; }
die() {
	printf 'bench51: error: %s\n' "$*" >&2
	exit 1
}

while [ $# -gt 0 ]; do
	case $1 in
	--version) [ $# -ge 2 ] || die "--version needs a value"; VERSION=$2; shift 2 ;;
	--version=*) VERSION=${1#*=}; shift ;;
	--dir) [ $# -ge 2 ] || die "--dir needs a value"; DIR=$2; shift 2 ;;
	--dir=*) DIR=${1#*=}; shift ;;
	--archive) [ $# -ge 2 ] || die "--archive needs a value"; ARCHIVE=$2; shift 2 ;;
	--archive=*) ARCHIVE=${1#*=}; shift ;;
	--no-desktop) DESKTOP=0; shift ;;
	--uninstall) UNINSTALL=1; shift ;;
	-h | --help) usage; exit 0 ;;
	*) die "unknown option: $1 (see --help)" ;;
	esac
done

refresh_menu() {
	if command -v update-desktop-database >/dev/null 2>&1; then
		update-desktop-database -q "$APPS" 2>/dev/null || true
	fi
}

if [ "$UNINSTALL" = 1 ]; then
	rm -f "$DIR/bench51" "$DIR/bench51-gui" "$APPS/bench51-gui.desktop" "$ICONS/bench51-gui.svg"
	refresh_menu
	say "removed bench51 from $DIR"
	exit 0
fi

# ---- Which build fits this machine ----

os=$(uname -s)
arch=$(uname -m)
case $os in
Linux) os=unknown-linux-gnu ;;
Darwin) os=apple-darwin ;;
*) die "no prebuilt binaries for $os; on Windows use install.ps1" ;;
esac
case $arch in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
*) die "no prebuilt binaries for $arch; build from source with: cargo install --git https://github.com/$REPO bench51-gui" ;;
esac
TARGET=$arch-$os
NAME=bench51-$TARGET.tar.gz

download() {
	if command -v curl >/dev/null 2>&1; then
		curl --proto '=https' --tlsv1.2 -fsSL -o "$2" "$1"
	elif command -v wget >/dev/null 2>&1; then
		wget -q --https-only -O "$2" "$1"
	else
		die "neither curl nor wget is installed"
	fi
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d ' ' -f 1
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
	else
		die "neither sha256sum nor shasum is installed to verify the download"
	fi
}

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t bench51)
trap 'rm -rf "$TMP"' EXIT
trap 'exit 130' INT TERM

# ---- Download and verify ----

if [ -z "$ARCHIVE" ]; then
	if [ "$VERSION" = latest ]; then
		BASE=https://github.com/$REPO/releases/latest/download
	else
		case $VERSION in v*) ;; *) VERSION=v$VERSION ;; esac
		BASE=https://github.com/$REPO/releases/download/$VERSION
	fi
	say "downloading $NAME ($VERSION)"
	download "$BASE/$NAME" "$TMP/$NAME" || die "could not download $BASE/$NAME"
	download "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" || die "could not download the checksums"
	expected=$(grep " [* ]*$NAME\$" "$TMP/SHA256SUMS" | cut -d ' ' -f 1)
	[ -n "$expected" ] || die "$NAME is not listed in SHA256SUMS"
	[ "$(sha256 "$TMP/$NAME")" = "$expected" ] || die "checksum mismatch for $NAME"
	ARCHIVE=$TMP/$NAME
fi
[ -f "$ARCHIVE" ] || die "no such archive: $ARCHIVE"

mkdir "$TMP/x"
tar -xzf "$ARCHIVE" -C "$TMP/x"
SRC=$(find "$TMP/x" -type f -name bench51-gui | head -n 1)
[ -n "$SRC" ] || die "the archive has no bench51-gui"
SRC=$(dirname "$SRC")
[ -f "$SRC/bench51" ] || die "the archive has no bench51"

# ---- Install ----

mkdir -p "$DIR"
for bin in bench51 bench51-gui; do
	cp "$SRC/$bin" "$DIR/$bin.tmp"
	chmod 755 "$DIR/$bin.tmp"
	mv -f "$DIR/$bin.tmp" "$DIR/$bin"
done
say "installed bench51 and bench51-gui to $DIR"

if [ "$DESKTOP" = 1 ] && [ "$(uname -s)" = Linux ] && [ -f "$SRC/share/applications/bench51-gui.desktop" ]; then
	mkdir -p "$APPS" "$ICONS"
	# Launchers don't always see ~/.local/bin on PATH: point at the binary.
	exec_path=$(printf '%s' "$DIR/bench51-gui" | sed 's/[\\&|]/\\&/g')
	sed "s|^Exec=bench51-gui|Exec=\"$exec_path\"|" \
		"$SRC/share/applications/bench51-gui.desktop" >"$APPS/bench51-gui.desktop"
	cp "$SRC/share/icons/hicolor/scalable/apps/bench51-gui.svg" "$ICONS/bench51-gui.svg"
	refresh_menu
	say "added bench51 to the application menu"
fi

"$DIR/bench51" --version
case ":$PATH:" in
*":$DIR:"*) say "run bench51-gui for the desktop app, or bench51 in a terminal" ;;
*) say "$DIR is not on your PATH; add it, e.g. in ~/.profile: export PATH=\"$DIR:\$PATH\"" ;;
esac
