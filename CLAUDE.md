# rviewer — Chess Puzzle Viewer (Rust)

A Rust/[egui](https://github.com/emilk/egui) port of `../viewer.py`, built
to ship as a **standalone binary** for Windows and Linux — no Python
install, no interpreter, no DLLs to install alongside it.

This file documents the `rviewer/` subproject specifically. See
`../CLAUDE.md` for the overall Chess Puzzle Analysis Toolkit.

---

## Project Structure

```
rviewer/
├── src/
│   ├── main.rs          # app logic, UI, board rendering, FEN/CSV parsing, unit tests
│   ├── pieces_data.rs   # auto-generated: include_bytes! for all 24 piece PNGs
│   └── themes.rs        # puzzle theme keys, sourced from lila's puzzleTheme.xml
├── assets/pieces/       # source PNGs (copied from ../pieces/)
├── build.sh             # builds Windows + Linux release binaries into dist/
├── rviewe.sh            # runs the Windows .exe under Wine, for testing on Linux
├── dist/
│   ├── ChessPuzzleViewer.exe      # the file to hand to a Windows student
│   └── ChessPuzzleViewer-linux    # the file to hand to a Linux student
├── Cargo.toml
├── Cargo.lock
└── README.md
```

---

## Using the Viewer

1. Double-click `ChessPuzzleViewer.exe` (Windows), or run
   `./ChessPuzzleViewer-linux` (Linux).
2. Click **"Open Puzzle File…"** and pick a `.csv` (Lichess puzzle export)
   or `.fen` (one position per line, `#` for comments) file.
3. **Filter** by up to 5 themes (boolean AND) via the comboboxes, and/or by
   solution content via the "🔍 Solution contains…" popup (substring match,
   case-insensitive), then click **Filter**.
4. **Navigate** with the four-direction pad (or arrow keys): ↑/▲ and ↓/▼
   move between puzzles (games); ←/◁ and →/▷ step through the current
   puzzle's solution moves one at a time.
5. Copy the FEN with the "Copy to Clipboard" button; click the URL to open
   the puzzle on Lichess.

**FEN handling note**: Lichess puzzle FENs are the position *before* an
automatic "preparation" move (the move that creates the tactic). The
viewer plays that move immediately and adapts the FEN accordingly
(castling rights, en passant target, and move counters all correctly
updated) — so the FEN shown, copied, and used as the board's starting
point is always the position the solver actually starts from, not the raw
CSV FEN. The preparation move itself is shown, greyed out, above the
board.

Piece graphics are compiled into the binary (`src/pieces_data.rs` embeds
`assets/pieces/*.png`), so the binary has no external file dependencies.

---

## Building

```bash
./build.sh            # builds both Windows and Linux binaries into dist/
./build.sh windows    # Windows .exe only
./build.sh linux      # Linux binary only

./clean.sh            # remove target/ and the built binaries in dist/
./clean.sh target     # cargo clean only
./clean.sh dist       # remove only the binaries in dist/ (PDFs etc. are kept)
```

### One-time setup (already done in this environment)

```bash
rustup target add x86_64-pc-windows-gnu x86_64-unknown-linux-gnu
apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
```

### Under the hood

```bash
RUSTFLAGS="-C target-feature=+crt-static" \
  cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

`+crt-static` on the Windows build matters: without it the exe depends on
`libgcc_s_seh-1.dll` / `libwinpthread-1.dll`, which aren't present on a
stock Windows install and would break the "no setup" requirement. With it,
the only DLLs the exe imports are baseline Windows system components
(kernel32, user32, gdi32, ole32, opengl32, …) present on every Windows
10/11 machine.

On a native Windows machine, plain `cargo build --release` (MSVC target)
also works and needs no special flags, since it links against the
Universal CRT that ships with Windows.

### Cargo.toml feature note

`eframe` has `default-features = false` and re-enables only what's needed:
`glow`, `default_fonts`, `persistence`, `wayland`, `x11`. The `wayland`/
`x11` features are **required** for the native Linux build to compile at
all — without them, winit's Linux windowing backend isn't built, and the
crate fails with `unresolved import` errors in `winit::platform_impl`
(this bit the project before `wayland`/`x11` were added — if the Linux
build ever mysteriously stops compiling again after touching `Cargo.toml`,
check these features first).

### Testing under Wine

`rviewe.sh` launches `dist/ChessPuzzleViewer.exe` under a dedicated 64-bit
Wine prefix (`~/.wine-rviewer` by default), for smoke-testing the Windows
build on Linux without a Windows machine.

---

## Tests

Parsing logic (`parse_fen`, `format_solution_moves`, CSV/FEN loaders, UCI
move application) has unit tests in `src/main.rs`. They run natively on
Linux:

```bash
cargo test --release --target x86_64-unknown-linux-gnu
```

---

## Development Notes

- Ported from `../viewer.py` (the Python/tkinter viewer) to get a single
  dependency-free binary that students can run without installing Python.
- Theme list in `src/themes.rs` is sourced from lila's own
  `puzzleTheme.xml` translation strings, not hand-derived from
  `../motives.txt`.
- `dist/` also holds ad-hoc handout material (e.g. PDFs) alongside the
  built binaries — only the binaries are build outputs.
