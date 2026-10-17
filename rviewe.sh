#!/usr/bin/env bash
# Launches the Windows build of the Chess Puzzle Viewer under Wine, for
# testing on Linux without a Windows machine.
#
# The default Wine prefix on this system (~/.wine) is 32-bit only and
# cannot run our 64-bit exe, so this script keeps its own dedicated
# 64-bit prefix and initializes it on first run.
#
# Usage: ./rviewe.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXE="$SCRIPT_DIR/dist/ChessPuzzleViewer.exe"

if [[ ! -f "$EXE" ]]; then
    echo "Error: $EXE not found. Build it first, e.g.:" >&2
    echo "  RUSTFLAGS=\"-C target-feature=+crt-static\" cargo build --release --target x86_64-pc-windows-gnu" >&2
    echo "  cp target/x86_64-pc-windows-gnu/release/rviewer.exe dist/ChessPuzzleViewer.exe" >&2
    exit 1
fi

# Locate a 64-bit wine loader; the 'wine' on PATH may be a 32-bit-only build.
WINE64_BIN=""
for candidate in wine64 /usr/lib/wine/wine64 /usr/lib/x86_64-linux-gnu/wine/wine64; do
    if command -v "$candidate" >/dev/null 2>&1; then
        WINE64_BIN="$(command -v "$candidate")"
        break
    elif [[ -x "$candidate" ]]; then
        WINE64_BIN="$candidate"
        break
    fi
done
if [[ -z "$WINE64_BIN" ]]; then
    echo "Error: no 64-bit wine loader (wine64) found. Install the 'wine64' package." >&2
    exit 1
fi

export WINEARCH=win64
export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-rviewer}"

if [[ ! -d "$WINEPREFIX" ]]; then
    echo "First run: creating 64-bit Wine prefix at $WINEPREFIX ..."
    WINEDEBUG=-all "$WINE64_BIN" wineboot --init >/dev/null 2>&1
fi

exec env WINEDEBUG=-all "$WINE64_BIN" "$EXE" "$@"
