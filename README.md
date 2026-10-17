# rviewer — Chess Puzzle Viewer (Rust)

A Rust/[egui](https://github.com/emilk/egui) port of `viewer.py`, built to
ship as **a standalone binary** for Windows and Linux — no Python, no
installer, no DLLs to install alongside it. Give the student
`dist/ChessPuzzleViewer.exe` (Windows) or `dist/ChessPuzzleViewer-linux`
(Ubuntu/Linux) and they can run it directly.

## Using it

1. Double-click `ChessPuzzleViewer.exe` (Windows), or run
   `./ChessPuzzleViewer-linux` (Linux).
2. Click **"Open Puzzle File…"** and pick a `.csv` (Lichess puzzle export)
   or `.fen` (one position per line, `#` for comments) file.
3. **Filter** by up to 5 themes (boolean AND) via the comboboxes, and/or by
   solution content via the "🔍 Solution contains…" popup (substring match,
   case-insensitive), then click **Filter**.
4. **Navigate** with the big four-direction pad (or the arrow keys): ↑/▲ and
   ↓/▼ move between puzzles (games); ←/◁ and →/▷ step through the current
   puzzle's solution moves one at a time.
5. Copy the FEN with the "Copy to Clipboard" button; click the URL to open
   the puzzle on Lichess.

Note on FENs: Lichess puzzle FENs are the position *before* an automatic
"preparation" move (the move that creates the tactic). This viewer plays
that move immediately and adapts the FEN accordingly (castling rights, en
passant target, and move counters all correctly updated) — so the FEN
shown, copied, and used as the board's starting point is always the
position the solver actually starts from, not the raw CSV FEN. The
preparation move itself is shown, greyed out, above the board.

Piece graphics are compiled into the binary (`src/pieces_data.rs` embeds
`assets/pieces/*.png`), so the exe has no external file dependencies at all.

## Rebuilding

```bash
./build.sh            # builds both Windows and Linux binaries into dist/
./build.sh windows    # Windows .exe only
./build.sh linux      # Linux binary only
```

One-time setup for cross-compiling to Windows from Linux (already done in
this environment):

```bash
rustup target add x86_64-pc-windows-gnu x86_64-unknown-linux-gnu
apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64
```

Under the hood, `build.sh` runs:

```bash
RUSTFLAGS="-C target-feature=+crt-static" \
  cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

`+crt-static` on the Windows build is important: without it the exe depends
on `libgcc_s_seh-1.dll` / `libwinpthread-1.dll`, which are not present on a
stock Windows install and would break the "no setup" requirement. With it,
the only DLLs the exe imports are baseline Windows system components
(kernel32, user32, gdi32, ole32, opengl32, …) present on every Windows
10/11 machine.

On a native Windows machine, plain `cargo build --release` (MSVC target)
also works and needs no special flags, since it links against the
Universal CRT that ships with Windows.

Note: `Cargo.toml` disables eframe's default features and re-enables only
the ones needed (`glow`, `default_fonts`, `persistence`, `wayland`, `x11`).
The `wayland`/`x11` features are required for the native Linux build to
compile at all — without them, winit's Linux windowing backend isn't
built, and the crate fails with `unresolved import` errors in
`winit::platform_impl`.

## Tests

The parsing logic (`parse_fen`, `format_solution_moves`, CSV/FEN loaders)
has unit tests in `src/main.rs`:

```bash
cargo test --release --target x86_64-unknown-linux-gnu
```

## Project layout

```
rviewer/
├── src/
│   ├── main.rs          # app logic, UI, board rendering, FEN/CSV parsing
│   └── pieces_data.rs   # auto-generated: include_bytes! for all 24 piece PNGs
├── assets/pieces/       # source PNGs (copied from ../pieces/)
├── build.sh             # builds Windows + Linux binaries into dist/
├── dist/
│   ├── ChessPuzzleViewer.exe      # the file to hand to a Windows student
│   └── ChessPuzzleViewer-linux    # the file to hand to a Linux student
└── Cargo.toml
```
