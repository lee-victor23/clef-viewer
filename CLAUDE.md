# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run
cargo run

# Check without building
cargo check

# Run tests
cargo test

# Run a single test
cargo test <test_name>
```

The release binary ends up at `target/release/clef-viewer.exe`.

## Architecture

Single-binary desktop GUI app built on [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui). All code lives in `src/main.rs`.

**Data flow:**
1. User opens a `.clef` / `.log` / `.json` file via `rfd` native file dialog
2. `load_file` reads it line-by-line; each line is parsed as a CLEF JSON object by `parse_clef_line`
3. Parsed `LogRecord`s are stored in `App::records`
4. `apply_filter` rebuilds `App::filtered` (indices into `records`) whenever any filter changes; it also recomputes `LevelStats` and `TemplateSummary`
5. `eframe::App::update` is the immediate-mode render loop — it reads `filtered` and renders the current page

**Key types:**
- `LogRecord` — one parsed log line; holds UTC + local timestamps, `Level`, rendered message, raw `serde_json::Value`
- `Level` — enum with colour/label helpers; mapped from Serilog `@l` field
- `App` — all UI + filter state; no separate model layer
- `DateTimePicker` — custom spin-box widget for date range filtering
- `LevelStats` / `TemplateSummary` — derived aggregates rebuilt on every `apply_filter` call

**CLEF fields used:**
| Field | Meaning |
|-------|---------|
| `@t`  | timestamp (RFC3339 or naive) |
| `@l`  | level string |
| `@mt` | message template (Serilog-style `{Property}` placeholders) |
| `@m`  | pre-rendered message (preferred over `@mt` when present) |
| `@x`  | exception string |

**UI layout:**
- Top panel: toolbar (Open, tab switcher, search, date range, level toggles)
- Right panel: stats sidebar (error count, level bar chart, level table)
- Central panel: either Logs tab (paginated rows, inline expand-to-detail on click) or Templates tab (aggregated message templates, click to filter logs)
