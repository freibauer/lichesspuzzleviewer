// Chess Puzzle Viewer (Rust / egui port of viewer.py)
//
// Single self-contained executable: piece graphics are embedded at compile
// time (see pieces_data.rs), and puzzle files (.csv / .fen) are loaded at
// runtime through a native file-picker dialog. No external files, no
// installer, no Python runtime required on the target machine.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pieces_data;
mod themes;

use eframe::egui;
use egui::{Color32, ColorImage, Key, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::path::Path;

const SQUARE_SIZE: f32 = 64.0;

struct Puzzle {
    id: String,
    fen: String,
    moves: String,
    rating: String,
    themes: String,
    game_url: String,
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 900.0])
            .with_min_inner_size([560.0, 680.0])
            // Explicitly request OS focus/activation on launch. Without this,
            // Windows' focus-stealing prevention can leave a freshly-spawned
            // window unfocused, so arrow-key input silently goes nowhere
            // until the user clicks into it.
            .with_active(true),
        ..Default::default()
    };
    eframe::run_native(
        "Chess Puzzle Viewer",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// Number of independent theme-filter comboboxes shown in the filter bar.
const NUM_THEME_FILTERS: usize = 5;

struct App {
    /// Every puzzle from the loaded file, in file order. Never reordered or
    /// pruned; the filter only affects `view`.
    all_puzzles: Vec<Puzzle>,
    /// Indices into `all_puzzles` for the puzzles currently shown, after
    /// applying the theme filter (or all indices, if no filter is active).
    view: Vec<usize>,
    /// Position of the current puzzle within `view` (not `all_puzzles`).
    idx: usize,
    /// How many moves of the current puzzle's solution have been played
    /// forward from its starting FEN (0 = starting position). Reset to 0
    /// whenever the current puzzle changes.
    move_idx: usize,
    /// One selected theme per combobox, or `None` for "(Any)". Applied as a
    /// boolean AND: a puzzle must carry every selected theme to pass.
    selected_themes: [Option<&'static str>; NUM_THEME_FILTERS],
    /// Free-text filter: a puzzle only passes if its Moves field contains
    /// this text (case-insensitive substring match). Empty = no filter.
    moves_filter: String,
    textures: HashMap<&'static str, TextureHandle>,
    status: String,
    source_name: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut textures = HashMap::new();
        for (name, bytes) in pieces_data::PIECES {
            match image::load_from_memory(bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let tex = cc.egui_ctx.load_texture(*name, color_image, TextureOptions::LINEAR);
                    textures.insert(*name, tex);
                }
                Err(e) => eprintln!("Failed to decode embedded piece image {name}: {e}"),
            }
        }

        Self {
            all_puzzles: Vec::new(),
            view: Vec::new(),
            idx: 0,
            move_idx: 0,
            selected_themes: [None; NUM_THEME_FILTERS],
            moves_filter: String::new(),
            textures,
            status: String::new(),
            source_name: String::new(),
        }
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open puzzle file")
            .add_filter("Puzzle files (*.csv, *.fen)", &["csv", "fen"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load_file(&path);
        }
    }

    fn load_file(&mut self, path: &Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let result = if ext == "fen" {
            load_fen_file(path)
        } else {
            load_csv_file(path)
        };

        match result {
            Ok(puzzles) if puzzles.is_empty() => {
                self.status = format!("No puzzles found in {}", path.display());
            }
            Ok(puzzles) => {
                self.all_puzzles = puzzles;
                self.selected_themes = [None; NUM_THEME_FILTERS];
                self.moves_filter.clear();
                self.view = (0..self.all_puzzles.len()).collect();
                self.idx = 0;
                self.move_idx = 0;
                self.status.clear();
                self.source_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
            }
            Err(e) => {
                self.status = format!("Error loading {}: {}", path.display(), e);
            }
        }
    }

    /// Recompute `view` from `selected_themes` (boolean AND across every
    /// non-"(Any)" combobox) and jump back to the first matching puzzle.
    fn apply_filter(&mut self) {
        let active: Vec<&str> = self.selected_themes.iter().filter_map(|t| *t).collect();
        let text_filter = self.moves_filter.clone();
        self.view = self
            .all_puzzles
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                matches_all_themes(&p.themes, &active) && matches_moves_filter(&p.moves, &text_filter)
            })
            .map(|(i, _)| i)
            .collect();
        self.idx = 0;
        self.move_idx = 0;
    }

    fn clear_filter(&mut self) {
        self.selected_themes = [None; NUM_THEME_FILTERS];
        self.moves_filter.clear();
        self.apply_filter();
    }

    fn current_puzzle(&self) -> Option<&Puzzle> {
        self.view.get(self.idx).map(|&i| &self.all_puzzles[i])
    }

    /// Move to the next puzzle (Down arrow / "Next" button).
    fn next(&mut self) {
        if self.idx + 1 < self.view.len() {
            self.idx += 1;
            self.move_idx = 0;
        }
    }

    /// Move to the previous puzzle (Up arrow / "Previous" button).
    fn prev(&mut self) {
        if self.idx > 0 {
            self.idx -= 1;
            self.move_idx = 0;
        }
    }

    /// Step one move forward through the current puzzle's solution (Right arrow).
    ///
    /// The puzzle's first listed move is the "preparation" move that's
    /// already baked into the starting position (see `load_file`/rendering),
    /// so it isn't one of the steppable moves here.
    fn next_move(&mut self) {
        if let Some(p) = self.current_puzzle() {
            let solver_moves = p.moves.split_whitespace().count().saturating_sub(1);
            if self.move_idx < solver_moves {
                self.move_idx += 1;
            }
        }
    }

    /// Step one move back through the current puzzle's solution (Left arrow).
    fn prev_move(&mut self) {
        if self.move_idx > 0 {
            self.move_idx -= 1;
        }
    }
}

/// A puzzle matches if its whitespace-separated Themes field contains every
/// theme in `required` (boolean AND). An empty `required` matches everything.
fn matches_all_themes(themes_field: &str, required: &[&str]) -> bool {
    if required.is_empty() {
        return true;
    }
    let tokens: Vec<&str> = themes_field.split_whitespace().collect();
    required.iter().all(|t| tokens.contains(t))
}

/// A puzzle matches if its Moves field contains `filter_text` as a
/// substring (case-insensitive). An empty/whitespace-only filter matches
/// everything.
fn matches_moves_filter(moves_field: &str, filter_text: &str) -> bool {
    let needle = filter_text.trim();
    if needle.is_empty() {
        return true;
    }
    moves_field.to_lowercase().contains(&needle.to_lowercase())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Skip global arrow-key navigation while some widget (e.g. the
        // solution text filter) has keyboard focus, so arrow keys there
        // move the text cursor / combobox selection instead of the puzzle.
        let any_widget_focused = ctx.memory(|m| m.focused().is_some());
        if !any_widget_focused {
            ctx.input(|i| {
                // Up/Down: move between puzzles (games).
                if i.key_pressed(Key::ArrowDown) {
                    self.next();
                }
                if i.key_pressed(Key::ArrowUp) {
                    self.prev();
                }
                // Left/Right: step through the current puzzle's solution moves.
                if i.key_pressed(Key::ArrowRight) {
                    self.next_move();
                }
                if i.key_pressed(Key::ArrowLeft) {
                    self.prev_move();
                }
            });
        }

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("📂 Open Puzzle File…").clicked() {
                    self.open_file_dialog();
                }
                ui.separator();
                let loaded_text = if self.source_name.is_empty() {
                    "No file loaded".to_string()
                } else if self.view.len() == self.all_puzzles.len() {
                    format!("Loaded: {}  ({} puzzles)", self.source_name, self.all_puzzles.len())
                } else {
                    format!(
                        "Loaded: {}  ({} / {} puzzles shown)",
                        self.source_name,
                        self.view.len(),
                        self.all_puzzles.len()
                    )
                };
                ui.label(loaded_text);
            });
            ui.add_space(4.0);
        });

        if !self.all_puzzles.is_empty() {
            egui::Panel::top("filter_bar").show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Filter by theme (all must match):");
                    for i in 0..NUM_THEME_FILTERS {
                        egui::ComboBox::from_id_salt(("theme_filter", i))
                            .selected_text(self.selected_themes[i].unwrap_or("(Any)"))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_themes[i], None, "(Any)");
                                for theme in themes::THEMES {
                                    ui.selectable_value(
                                        &mut self.selected_themes[i],
                                        Some(*theme),
                                        *theme,
                                    );
                                }
                            });
                    }
                    ui.separator();

                    // The text filter lives behind a popup button rather than
                    // an always-present text field, so it never steals focus
                    // from (and conflicts with) the arrow-key navigation.
                    let filter_label = if self.moves_filter.trim().is_empty() {
                        "🔍 Solution contains…".to_string()
                    } else {
                        format!("🔍 Solution contains: \"{}\"", self.moves_filter.trim())
                    };
                    let filter_button = ui.button(filter_label);
                    egui::Popup::from_toggle_button_response(&filter_button).show(|ui| {
                        ui.set_min_width(240.0);
                        ui.label("Only show puzzles whose solution contains:");
                        let text_response = ui.add(
                            egui::TextEdit::singleline(&mut self.moves_filter)
                                .hint_text("e.g. e2e4")
                                .desired_width(200.0),
                        );
                        text_response.request_focus();
                        let enter_pressed =
                            text_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() || enter_pressed {
                                self.apply_filter();
                            }
                            if ui.button("Clear").clicked() {
                                self.moves_filter.clear();
                                self.apply_filter();
                            }
                        });
                    });

                    if ui.button("Filter").clicked() {
                        self.apply_filter();
                    }
                    if ui.button("Clear All").clicked() {
                        self.clear_filter();
                    }
                });
                ui.add_space(4.0);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if self.all_puzzles.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.heading("Chess Puzzle Viewer");
                    ui.add_space(10.0);
                    ui.label("Open a Lichess puzzle .csv or a .fen position file to begin.");
                    ui.add_space(16.0);
                    if ui.button("Open Puzzle File…").clicked() {
                        self.open_file_dialog();
                    }
                    if !self.status.is_empty() {
                        ui.add_space(16.0);
                        ui.colored_label(Color32::from_rgb(0xd0, 0x30, 0x30), &self.status);
                    }
                });
                return;
            }

            if self.view.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.heading("No puzzles match the selected filters");
                    ui.label(format!(
                        "{} puzzles loaded, 0 match all selected themes.",
                        self.all_puzzles.len()
                    ));
                    ui.add_space(12.0);
                    if ui.button("Clear Filters").clicked() {
                        self.clear_filter();
                    }
                });
                return;
            }

            let (raw_fen, moves, id, rating, themes, game_url) = {
                let p = &self.all_puzzles[self.view[self.idx]];
                (
                    p.fen.clone(),
                    p.moves.clone(),
                    p.id.clone(),
                    p.rating.clone(),
                    p.themes.clone(),
                    p.game_url.clone(),
                )
            };

            // Lichess puzzle FENs are the position *before* an automatic
            // "preparation" move (the losing side's last move, which is
            // what creates the tactic) — the solver's job only starts after
            // that move is played. So we execute it up front and treat the
            // resulting position as the puzzle's real starting FEN; only
            // the remaining moves are the interactively steppable solution.
            let moves_list: Vec<&str> = moves.split_whitespace().collect();
            let prep_move = moves_list.first().copied();
            let (prepared_fen, _, _) = fen_after_moves(&raw_fen, &moves_list, 1);
            let solver_moves: &[&str] = if moves_list.len() > 1 { &moves_list[1..] } else { &[] };
            let total_moves = solver_moves.len();
            let move_idx = self.move_idx.min(total_moves);

            // Board orientation stays fixed to the puzzle's starting side to
            // move, even as moves are stepped through with Left/Right.
            let flip = prepared_fen.split_whitespace().nth(1) == Some("b");
            let (display_fen, board, active_color) =
                fen_after_moves(&prepared_fen, solver_moves, move_idx);

            ui.vertical_centered(|ui| {
                ui.label(format!("Themes: {themes}"))
                    .on_hover_text(format!("Puzzle {id} — Rating {rating}"));
                draw_turn_indicator(ui, active_color);
                if let Some(prep) = prep_move {
                    ui.label(
                        egui::RichText::new(format!("Preparation move (auto-played): {prep}"))
                            .italics()
                            .weak(),
                    );
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Solution:");
                let mut display = format_solution_moves(&solver_moves.join(" "));
                ui.add(
                    egui::TextEdit::multiline(&mut display)
                        .desired_rows(2)
                        .interactive(false)
                        .font(egui::TextStyle::Monospace),
                );
            });

            ui.horizontal(|ui| {
                ui.label("FEN:");
                let mut fen_display = display_fen.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut fen_display)
                        .desired_rows(1)
                        .interactive(false)
                        .font(egui::TextStyle::Monospace),
                );
                if ui.button("Copy to Clipboard").clicked() {
                    ctx.copy_text(display_fen.clone());
                }
            });

            ui.add_space(8.0);
            // Board on the left, big navigation pad to its right.
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - SQUARE_SIZE * 8.0 - 140.0).max(0.0) / 2.0);
                draw_board(ui, &self.textures, &board, flip, SQUARE_SIZE);
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(SQUARE_SIZE * 8.0 / 2.0 - 90.0);
                    egui::Grid::new("dpad_grid").spacing([8.0, 8.0]).show(ui, |ui| {
                        ui.label("");
                        if big_nav_button(ui, "▲", "Previous game (↑)", self.idx > 0) {
                            self.prev();
                        }
                        ui.label("");
                        ui.end_row();

                        if big_nav_button(ui, "◁", "Previous move (←)", move_idx > 0) {
                            self.prev_move();
                        }
                        ui.vertical_centered(|ui| {
                            ui.label(format!("Game {} / {}", self.idx + 1, self.view.len()));
                            ui.label(format!("Move {move_idx} / {total_moves}"));
                        });
                        if big_nav_button(ui, "▷", "Next move (→)", move_idx < total_moves) {
                            self.next_move();
                        }
                        ui.end_row();

                        ui.label("");
                        if big_nav_button(ui, "▼", "Next game (↓)", self.idx + 1 < self.view.len()) {
                            self.next();
                        }
                        ui.label("");
                        ui.end_row();
                    });
                });
            });

            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                if !game_url.trim().is_empty() {
                    if ui.link(format!("URL: {game_url}")).clicked() {
                        let _ = webbrowser::open(&game_url);
                    }
                } else {
                    ui.label("URL: (none)");
                }
            });

            if !self.status.is_empty() {
                ui.add_space(6.0);
                ui.colored_label(Color32::from_rgb(0xd0, 0x30, 0x30), &self.status);
            }
        });
    }
}

/// Load puzzles from a Lichess-style puzzle CSV. Header-driven: required
/// column is FEN; all others degrade gracefully if absent. Malformed rows
/// are skipped instead of aborting the whole load.
fn load_csv_file(path: &Path) -> Result<Vec<Puzzle>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;

    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let idx_of = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));

    let fen_idx = idx_of("FEN").ok_or("CSV is missing a required 'FEN' column")?;
    let id_idx = idx_of("PuzzleId");
    let moves_idx = idx_of("Moves");
    let rating_idx = idx_of("Rating");
    let themes_idx = idx_of("Themes");
    let url_idx = idx_of("GameUrl");

    let mut puzzles = Vec::new();
    for rec in rdr.records() {
        let rec = match rec {
            Ok(r) => r,
            Err(_) => continue, // skip malformed row rather than aborting the whole file
        };
        let get = |idx: Option<usize>| -> String {
            idx.and_then(|i| rec.get(i)).unwrap_or("").to_string()
        };
        let fen = rec.get(fen_idx).unwrap_or("").trim().to_string();
        if fen.is_empty() {
            continue; // no usable position on this row
        }
        puzzles.push(Puzzle {
            id: if id_idx.is_some() {
                get(id_idx)
            } else {
                format!("#{}", puzzles.len() + 1)
            },
            fen,
            moves: get(moves_idx),
            rating: if rating_idx.is_some() { get(rating_idx) } else { "N/A".into() },
            themes: get(themes_idx),
            game_url: get(url_idx),
        });
    }
    Ok(puzzles)
}

/// Load positions from a simple .fen file: one FEN per line, '#' comments,
/// blank lines ignored.
fn load_fen_file(path: &Path) -> Result<Vec<Puzzle>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut puzzles = Vec::new();
    for (line_num, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        puzzles.push(Puzzle {
            id: format!("FEN-{}", puzzles.len() + 1),
            fen: line.to_string(),
            moves: String::new(),
            rating: "N/A".into(),
            themes: format!("Line {}", line_num + 1),
            game_url: String::new(),
        });
    }
    Ok(puzzles)
}

/// Parse a FEN string into an 8x8 board (row 0 = rank 8) and active color.
/// Tolerant of malformed input: never panics, missing/extra squares are
/// simply left empty rather than causing an index error.
fn parse_fen(fen: &str) -> ([[char; 8]; 8], char) {
    let mut board = [['.'; 8]; 8];
    let parts: Vec<&str> = fen.split_whitespace().collect();
    let position = parts.first().copied().unwrap_or("8/8/8/8/8/8/8/8");
    let active = parts.get(1).and_then(|s| s.chars().next()).unwrap_or('w');

    for (row, rank_str) in position.split('/').enumerate().take(8) {
        let mut col = 0usize;
        for ch in rank_str.chars() {
            if col >= 8 {
                break;
            }
            if let Some(d) = ch.to_digit(10) {
                col += d as usize;
            } else {
                board[row][col] = ch;
                col += 1;
            }
        }
    }
    (board, active)
}

/// Convert a FEN file/rank pair (e.g. 'e', '4') to (row, col), row 0 = rank 8.
/// Returns `None` for anything outside a-h / 1-8.
fn square_to_rc(file: char, rank: char) -> Option<(usize, usize)> {
    if !('a'..='h').contains(&file) || !('1'..='8').contains(&rank) {
        return None;
    }
    let col = (file as u8 - b'a') as usize;
    let row = 7 - (rank as u8 - b'1') as usize;
    Some((row, col))
}

/// Apply one UCI move (e.g. "e2e4", "e7e8q", castling as "e1g1"/"e1c1") to
/// `board` in place. Handles en passant and castling rook movement.
/// Malformed moves or moves from an empty square are silently ignored
/// rather than panicking, since move lists come from puzzle data we don't
/// fully validate.
fn apply_uci_move(board: &mut [[char; 8]; 8], mv: &str) {
    let chars: Vec<char> = mv.chars().collect();
    if chars.len() < 4 {
        return;
    }
    let (Some((fr, fc)), Some((tr, tc))) = (
        square_to_rc(chars[0], chars[1]),
        square_to_rc(chars[2], chars[3]),
    ) else {
        return;
    };

    let piece = board[fr][fc];
    if piece == '.' {
        return; // nothing to move
    }

    // En passant: a pawn moving diagonally onto an empty square captures
    // the pawn sitting beside its start square, not on the target square.
    if piece.eq_ignore_ascii_case(&'p') && fc != tc && board[tr][tc] == '.' {
        board[fr][tc] = '.';
    }

    // Castling: king moving two files also moves the corresponding rook.
    if piece.eq_ignore_ascii_case(&'k') && fc.abs_diff(tc) == 2 {
        let rook_from = if tc > fc { 7 } else { 0 };
        let rook_to = if tc > fc { tc - 1 } else { tc + 1 };
        let rook = board[fr][rook_from];
        board[fr][rook_from] = '.';
        board[fr][rook_to] = rook;
    }

    board[fr][fc] = '.';
    let moved_piece = match chars.get(4) {
        Some(promo) => {
            let upper = promo.to_ascii_uppercase();
            if piece.is_ascii_uppercase() { upper } else { upper.to_ascii_lowercase() }
        }
        None => piece,
    };
    board[tr][tc] = moved_piece;
}

/// Convert a (row, col) board index back to algebraic notation, e.g. (0, 4) -> "e8".
fn square_name(row: usize, col: usize) -> String {
    let file = (b'a' + col as u8) as char;
    let rank = (b'1' + (7 - row) as u8) as char;
    format!("{file}{rank}")
}

/// If (row, col) is one of the four rook home squares, remove the matching
/// castling-right letter from `castling`. Used both when a rook moves away
/// from its home square and when a rook is captured on it.
fn remove_castling_right_for_square(castling: &mut String, row: usize, col: usize) {
    let letter = match (row, col) {
        (7, 0) => Some('Q'), // a1: white queenside
        (7, 7) => Some('K'), // h1: white kingside
        (0, 0) => Some('q'), // a8: black queenside
        (0, 7) => Some('k'), // h8: black kingside
        _ => None,
    };
    if let Some(c) = letter {
        castling.retain(|ch| ch != c);
    }
}

/// A full FEN position: board plus the other four FEN fields, kept correct
/// across move application. Unlike the board-only helpers above, this is
/// used whenever the FEN *string* itself needs to stay accurate — e.g. when
/// baking a puzzle's preparation move into its starting FEN.
struct FenState {
    board: [[char; 8]; 8],
    active: char,
    castling: String,
    en_passant: String,
    halfmove: u32,
    fullmove: u32,
}

impl FenState {
    fn parse(fen: &str) -> Self {
        let (board, active) = parse_fen(fen);
        let parts: Vec<&str> = fen.split_whitespace().collect();
        Self {
            board,
            active,
            castling: parts.get(2).copied().unwrap_or("-").to_string(),
            en_passant: parts.get(3).copied().unwrap_or("-").to_string(),
            halfmove: parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0),
            fullmove: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(1),
        }
    }

    fn to_fen(&self) -> String {
        let mut ranks = Vec::with_capacity(8);
        for row in 0..8 {
            let mut rank = String::new();
            let mut empty = 0u32;
            for col in 0..8 {
                let c = self.board[row][col];
                if c == '.' {
                    empty += 1;
                } else {
                    if empty > 0 {
                        rank.push_str(&empty.to_string());
                        empty = 0;
                    }
                    rank.push(c);
                }
            }
            if empty > 0 {
                rank.push_str(&empty.to_string());
            }
            ranks.push(rank);
        }
        let castling = if self.castling.is_empty() { "-" } else { &self.castling };
        format!(
            "{} {} {} {} {} {}",
            ranks.join("/"),
            self.active,
            castling,
            self.en_passant,
            self.halfmove,
            self.fullmove
        )
    }

    /// Apply one UCI move, keeping every FEN field correct: board, active
    /// color, castling rights, en passant target, and the half/full-move
    /// counters. Malformed or empty-square moves are ignored, same as
    /// [`apply_uci_move`].
    fn apply_move(&mut self, mv: &str) {
        let chars: Vec<char> = mv.chars().collect();
        if chars.len() < 4 {
            return;
        }
        let (Some((fr, fc)), Some((tr, tc))) = (
            square_to_rc(chars[0], chars[1]),
            square_to_rc(chars[2], chars[3]),
        ) else {
            return;
        };
        let piece = self.board[fr][fc];
        if piece == '.' {
            return;
        }
        let is_pawn = piece.eq_ignore_ascii_case(&'p');
        let is_king = piece.eq_ignore_ascii_case(&'k');
        let is_capture = self.board[tr][tc] != '.';

        // Halfmove clock: resets on a pawn move or any capture.
        self.halfmove = if is_pawn || is_capture { 0 } else { self.halfmove + 1 };

        apply_uci_move(&mut self.board, mv);

        // Castling rights: king move loses both of that color's rights; a
        // rook leaving (or being captured on) its home square loses just
        // that one right.
        if is_king {
            let lost: &[char] = if piece.is_ascii_uppercase() { &['K', 'Q'] } else { &['k', 'q'] };
            self.castling.retain(|c| !lost.contains(&c));
        }
        remove_castling_right_for_square(&mut self.castling, fr, fc);
        if is_capture {
            remove_castling_right_for_square(&mut self.castling, tr, tc);
        }

        // En passant target: only set right after a pawn's two-square push.
        self.en_passant = if is_pawn && fr.abs_diff(tr) == 2 {
            square_name((fr + tr) / 2, fc)
        } else {
            "-".to_string()
        };

        if self.active == 'b' {
            self.fullmove += 1;
        }
        self.active = if self.active == 'w' { 'b' } else { 'w' };
    }
}

/// Replay the first `count` moves of `moves` (UCI, whitespace-separated)
/// onto the position in `fen`, returning the resulting FEN, board, and the
/// color to move at that point. `count` is clamped to the number of moves
/// available, so it's always safe to pass e.g. `usize::MAX`.
fn fen_after_moves(fen: &str, moves: &[&str], count: usize) -> (String, [[char; 8]; 8], char) {
    let mut state = FenState::parse(fen);
    let n = count.min(moves.len());
    for mv in &moves[..n] {
        state.apply_move(mv);
    }
    (state.to_fen(), state.board, state.active)
}

fn format_solution_moves(moves: &str) -> String {
    let moves = moves.trim();
    if moves.is_empty() {
        return "(Position only - no moves)".to_string();
    }
    let mut formatted = Vec::new();
    for (i, mv) in moves.split_whitespace().enumerate() {
        if i % 2 == 0 {
            formatted.push(format!("{}. {}", i / 2 + 1, mv));
        } else {
            formatted.push(mv.to_string());
        }
    }
    formatted.join(" ")
}

/// A big solid rectangle standing in for the "White to move" / "Black to
/// move" text: white fill for White, black fill for Black, with a thin
/// outline so the white rectangle stays visible against a light background.
fn draw_turn_indicator(ui: &mut egui::Ui, active_color: char) {
    let size = Vec2::new(100.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let fill = if active_color == 'w' { Color32::WHITE } else { Color32::BLACK };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(2.0, Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text(if active_color == 'w' { "White to move" } else { "Black to move" });
}

/// One big arrow button for the four-direction navigator, with a tooltip
/// describing the matching keyboard shortcut. Returns whether it was clicked.
fn big_nav_button(ui: &mut egui::Ui, arrow: &str, tooltip: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(arrow).size(28.0))
            .min_size(Vec2::new(60.0, 48.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}

fn draw_board(
    ui: &mut egui::Ui,
    textures: &HashMap<&'static str, TextureHandle>,
    board: &[[char; 8]; 8],
    flip: bool,
    square_size: f32,
) {
    let board_px = square_size * 8.0;
    let (rect, _response) = ui.allocate_exact_size(Vec2::splat(board_px), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let light_square = Color32::from_rgb(0xf0, 0xd9, 0xb5);
    let dark_square = Color32::from_rgb(0xb5, 0x88, 0x63);

    for row in 0..8 {
        for col in 0..8 {
            let (drow, dcol) = if flip { (7 - row, 7 - col) } else { (row, col) };
            let x = rect.left() + dcol as f32 * square_size;
            let y = rect.top() + drow as f32 * square_size;
            let sq_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(square_size));

            let is_light = (row + col) % 2 == 0;
            painter.rect_filled(sq_rect, 0.0, if is_light { light_square } else { dark_square });

            let piece = board[row][col];
            if piece != '.' {
                let color_prefix = if piece.is_ascii_uppercase() { "w" } else { "b" };
                let piece_type = piece.to_ascii_uppercase();
                let bg = if is_light { "light" } else { "dark" };
                let key = format!("{color_prefix}{piece_type}_{bg}");
                if let Some(tex) = textures.get(key.as_str()) {
                    painter.image(
                        tex.id(),
                        sq_rect,
                        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    painter.text(
                        sq_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        piece.to_string(),
                        egui::FontId::proportional(square_size * 0.6),
                        Color32::BLACK,
                    );
                }
            }
        }
    }

    let coord_light = Color32::from_rgb(0xf0, 0xd9, 0xb5);
    let coord_dark = Color32::from_rgb(0xb5, 0x88, 0x63);

    for i in 0..8u8 {
        let (file_letter, rank_number) = if flip {
            ((b'h' - i) as char, (i + 1).to_string())
        } else {
            ((b'a' + i) as char, (8 - i).to_string())
        };

        let is_light_bottom = i % 2 == 1;
        let text_color = if is_light_bottom { coord_dark } else { coord_light };
        painter.text(
            Pos2::new(
                rect.left() + i as f32 * square_size + square_size - 5.0,
                rect.top() + 8.0 * square_size - 5.0,
            ),
            egui::Align2::RIGHT_BOTTOM,
            file_letter.to_string(),
            egui::FontId::proportional(13.0),
            text_color,
        );

        let is_light_left = i % 2 == 0;
        let text_color = if is_light_left { coord_dark } else { coord_light };
        painter.text(
            Pos2::new(rect.left() + 5.0, rect.top() + i as f32 * square_size + 5.0),
            egui::Align2::LEFT_TOP,
            rank_number,
            egui::FontId::proportional(13.0),
            text_color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("rviewer_test_{}_{}", std::process::id(), name));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parse_fen_starting_position() {
        let (board, active) =
            parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert_eq!(active, 'w');
        assert_eq!(board[0], ['r', 'n', 'b', 'q', 'k', 'b', 'n', 'r']);
        assert_eq!(board[1], ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p']);
        assert_eq!(board[6], ['P', 'P', 'P', 'P', 'P', 'P', 'P', 'P']);
        assert_eq!(board[7], ['R', 'N', 'B', 'Q', 'K', 'B', 'N', 'R']);
    }

    #[test]
    fn parse_fen_black_to_move() {
        let (_board, active) = parse_fen("8/8/8/4k3/8/8/4K3/8 b - - 0 1");
        assert_eq!(active, 'b');
    }

    #[test]
    fn parse_fen_malformed_does_not_panic() {
        // Missing ranks, overlong rank, garbage tokens, empty string.
        let (_b1, _a1) = parse_fen("");
        let (_b2, _a2) = parse_fen("not-a-fen-at-all");
        let (_b3, _a3) = parse_fen("pppppppppppppppp/8 w - - 0 1"); // overlong rank
        let (b4, _a4) = parse_fen("8/8/8 w"); // too few ranks
        assert_eq!(b4[5], ['.'; 8]); // unspecified ranks stay empty
    }

    #[test]
    fn apply_uci_move_simple_advance() {
        let (mut board, _) = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        apply_uci_move(&mut board, "e2e4");
        assert_eq!(board[6][4], '.'); // e2 empty
        assert_eq!(board[4][4], 'P'); // e4 has the pawn
    }

    #[test]
    fn apply_uci_move_capture_overwrites_target() {
        let (mut board, _) = parse_fen("8/8/8/4p3/3P4/8/8/8 w - - 0 1");
        apply_uci_move(&mut board, "d4e5");
        assert_eq!(board[3][3], '.');
        assert_eq!(board[3][4], 'P');
    }

    #[test]
    fn apply_uci_move_en_passant_removes_captured_pawn() {
        // White pawn on e5, black just played d7-d5; white captures en
        // passant with e5xd6, removing the black pawn from d5 (not d6).
        let (mut board, _) = parse_fen("8/8/8/3pP3/8/8/8/8 w - d6 0 1");
        apply_uci_move(&mut board, "e5d6");
        assert_eq!(board[2][3], 'P'); // white pawn landed on d6
        assert_eq!(board[3][4], '.'); // e5 (origin) now empty
        assert_eq!(board[3][3], '.'); // d5 (captured black pawn) now empty
    }

    #[test]
    fn apply_uci_move_castling_moves_rook_too() {
        let (mut board, _) =
            parse_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        apply_uci_move(&mut board, "e1g1"); // white kingside castle
        assert_eq!(board[7][4], '.'); // e1 empty
        assert_eq!(board[7][6], 'K'); // king on g1
        assert_eq!(board[7][5], 'R'); // rook moved to f1
        assert_eq!(board[7][7], '.'); // h1 empty
    }

    #[test]
    fn apply_uci_move_promotion_uses_move_case() {
        let (mut board, _) = parse_fen("8/4P3/8/8/8/8/8/8 w - - 0 1");
        apply_uci_move(&mut board, "e7e8q");
        assert_eq!(board[0][4], 'Q'); // promotes to white queen (uppercase)
    }

    #[test]
    fn apply_uci_move_malformed_does_not_panic() {
        let (mut board, _) = parse_fen("8/8/8/8/8/8/8/8 w - - 0 1");
        apply_uci_move(&mut board, "");
        apply_uci_move(&mut board, "z9z9");
        apply_uci_move(&mut board, "e2");
        apply_uci_move(&mut board, "e2e4"); // empty square, no piece to move
    }

    #[test]
    fn fen_after_moves_alternates_active_color_and_clamps_count() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let moves = ["e2e4", "e7e5", "g1f3"];
        let (_, _, active0) = fen_after_moves(fen, &moves, 0);
        assert_eq!(active0, 'w');
        let (_, _, active1) = fen_after_moves(fen, &moves, 1);
        assert_eq!(active1, 'b');
        let (_, board3, active3) = fen_after_moves(fen, &moves, 3);
        assert_eq!(active3, 'b');
        assert_eq!(board3[4][4], 'P'); // e4
        assert_eq!(board3[3][4], 'p'); // e5
        assert_eq!(board3[5][5], 'N'); // Nf3
        // count beyond available moves is clamped, not a panic.
        let (fen_over, board_over, active_over) = fen_after_moves(fen, &moves, 999);
        assert_eq!(board_over, board3);
        assert_eq!(active_over, active3);
        assert_eq!(fen_over, fen_after_moves(fen, &moves, 3).0);
    }

    #[test]
    fn fen_state_round_trips_starting_position() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert_eq!(FenState::parse(fen).to_fen(), fen);
    }

    #[test]
    fn fen_state_pawn_double_push_sets_en_passant_target() {
        let mut state = FenState::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        state.apply_move("e2e4");
        assert_eq!(state.en_passant, "e3");
        // The very next move (not a double pawn push) clears it again.
        state.apply_move("g8f6");
        assert_eq!(state.en_passant, "-");
    }

    #[test]
    fn fen_state_king_move_loses_both_castling_rights() {
        let mut state = FenState::parse("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        state.apply_move("e1e2");
        assert_eq!(state.castling, "kq"); // white's rights gone, black's remain
    }

    #[test]
    fn fen_state_rook_move_loses_only_that_side_right() {
        let mut state = FenState::parse("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        state.apply_move("h1g1"); // white kingside rook moves
        assert_eq!(state.castling, "Qkq");
    }

    #[test]
    fn fen_state_capturing_rook_removes_opponents_right() {
        // White rook on g8 captures the black rook sitting on its home
        // square h8; only black's kingside right ('k') should be lost.
        let mut state = FenState::parse("6Rr/8/8/8/8/8/8/8 w Qkq - 0 1");
        state.apply_move("g8h8");
        assert_eq!(state.castling, "Qq");
    }

    #[test]
    fn fen_state_halfmove_clock_resets_on_pawn_move_or_capture() {
        let mut state = FenState::parse("8/8/8/4p3/3P4/8/8/8 w - - 7 10");
        state.apply_move("d4e5"); // pawn capture
        assert_eq!(state.halfmove, 0);
        let mut quiet = FenState::parse("8/8/8/8/8/4N3/8/8 w - - 7 10");
        quiet.apply_move("e3d5"); // knight move, no capture
        assert_eq!(quiet.halfmove, 8);
    }

    #[test]
    fn fen_state_fullmove_increments_after_black_moves() {
        let mut state = FenState::parse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        state.apply_move("e2e4");
        assert_eq!(state.fullmove, 1); // still white's move number until black replies
        state.apply_move("e7e5");
        assert_eq!(state.fullmove, 2);
    }

    #[test]
    fn preparation_move_is_baked_into_prepared_fen() {
        // Starting position, White to move; "e2e4" is the puzzle's
        // preparation move (auto-played), leaving Black to move.
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let moves = ["e2e4", "e7e5", "g1f3"];
        let (prepared_fen, board, active) = fen_after_moves(fen, &moves, 1);
        assert_eq!(active, 'b');
        assert_eq!(board[4][4], 'P'); // e4 has been played
        // The remaining solver moves replay correctly on top of that FEN.
        let solver_moves = &moves[1..];
        let (_, _, active_after_solver_move) = fen_after_moves(&prepared_fen, solver_moves, 1);
        assert_eq!(active_after_solver_move, 'w');
    }

    #[test]
    fn matches_all_themes_boolean_and() {
        let themes = "middlegame short advantage crushing fork";
        assert!(matches_all_themes(themes, &["fork", "short"]));
        assert!(matches_all_themes(themes, &[]));
        assert!(!matches_all_themes(themes, &["fork", "mateIn1"]));
        assert!(!matches_all_themes(themes, &["endgame"]));
    }

    #[test]
    fn matches_moves_filter_substring_case_insensitive() {
        let moves = "e2e4 e7e5 g1f3 b8c6";
        assert!(matches_moves_filter(moves, "e2e4"));
        assert!(matches_moves_filter(moves, "E2E4")); // case-insensitive
        assert!(matches_moves_filter(moves, "")); // empty filter matches everything
        assert!(matches_moves_filter(moves, "   ")); // whitespace-only too
        assert!(!matches_moves_filter(moves, "d2d4"));
    }

    #[test]
    fn format_solution_moves_pairs_correctly() {
        assert_eq!(format_solution_moves(""), "(Position only - no moves)");
        assert_eq!(format_solution_moves("e2e4 e7e5 g1f3"), "1. e2e4 e7e5 2. g1f3");
    }

    #[test]
    fn load_fen_file_skips_comments_and_blanks() {
        let path = temp_file(
            "positions.fen",
            "# a comment\n\n8/8/8/4k3/8/8/4K3/8 w - - 0 1\n  \nrnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\n",
        );
        let puzzles = load_fen_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(puzzles.len(), 2);
        assert_eq!(puzzles[0].id, "FEN-1");
        assert_eq!(puzzles[1].fen, "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    }

    #[test]
    fn load_csv_file_parses_lichess_format() {
        let path = temp_file(
            "puzzles.csv",
            "PuzzleId,FEN,Moves,Rating,Themes,GameUrl\n\
             abc12,8/8/8/4k3/8/8/4K3/8 w - - 0 1,e2e4 e7e5,1500,mate mateIn1,https://lichess.org/abc12\n",
        );
        let puzzles = load_csv_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(puzzles.len(), 1);
        assert_eq!(puzzles[0].id, "abc12");
        assert_eq!(puzzles[0].rating, "1500");
        assert_eq!(puzzles[0].game_url, "https://lichess.org/abc12");
    }

    #[test]
    fn load_csv_file_rejects_missing_fen_column() {
        let path = temp_file("nofen.csv", "PuzzleId,Moves,Rating\nabc,e2e4,1500\n");
        let result = load_csv_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
    }

    #[test]
    fn load_csv_file_skips_rows_with_empty_fen() {
        let path = temp_file(
            "emptyfen.csv",
            "PuzzleId,FEN,Moves,Rating\nabc,,e2e4,1500\ndef,8/8/8/4k3/8/8/4K3/8 w - - 0 1,e2e4,1500\n",
        );
        let puzzles = load_csv_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(puzzles.len(), 1);
        assert_eq!(puzzles[0].id, "def");
    }
}
