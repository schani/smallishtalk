#!/usr/bin/env bash
# Run the classic Smalltalk UI (UI.md) from a fresh checkout.
#
#   scripts/run-ui.sh              # headless: render a Class Browser -> ui-screenshot.png
#   scripts/run-ui.sh --window     # live, click-navigable window (needs the `ui`
#                                  #   cargo feature's deps + a display server)
#
# Builds the VM binary, cross-compiles a UI image (kernel + compiler + UI layers
# + a demo driver) with GNU Smalltalk, and runs it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE=headless
[ "${1:-}" = "--window" ] && MODE=window

command -v gst >/dev/null 2>&1 || {
	echo "error: GNU Smalltalk (gst) is required to build the image." >&2
	echo "       Install it (e.g. 'brew install gnu-smalltalk' or your package manager)." >&2
	exit 1
}

# The compiler, filed in on the gst command line to run the image builder, and
# again into the image itself (by build_ui_image.st) for live compilation.
COMPILER=(
	st/compiler/Compat.st st/compiler/Treaty.st st/compiler/Platform.st
	st/compiler/AST.st st/compiler/Lexer.st st/compiler/Parser.st
	st/compiler/ChunkReader.st st/compiler/CodeGen.st st/compiler/Encoder.st
	st/compiler/ImageWriter.st st/compiler/Compiler.st
)

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
IMG="$TMP/ui.im"

if [ "$MODE" = window ]; then
	DRIVER=st/tools/ui_browser_window.st
	echo "== building the VM (with the ui window feature) =="
	cargo build --release --features ui
else
	DRIVER=st/tools/ui_browser_demo.st
	echo "== building the VM =="
	cargo build --release
fi

echo "== cross-compiling the UI image =="
gst -Q "${COMPILER[@]}" st/tools/build_ui_image.st -a "$DRIVER" "$IMG"

echo "== running =="
if [ "$MODE" = window ]; then
	exec ./target/release/smallishtalk --ui "$IMG"
else
	./target/release/smallishtalk "$IMG"
	echo
	echo "Wrote $ROOT/ui-screenshot.png — open it to see the live Class Browser."
	echo "For an interactive window: scripts/run-ui.sh --window  (or: make ui-window)"
fi
