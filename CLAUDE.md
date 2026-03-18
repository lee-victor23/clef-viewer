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

Single-binary desktop GUI app built on [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui).

**Module structure — place new code in the correct file:**

| File | Responsibility |
|------|---------------|
| `src/main.rs` | Entry point and `mod` declarations only. No logic, no types. |
| `src/types.rs` | All data types: `Level`, `DateFilter`, `LogRecord`, `LevelStats`, `TemplateSummary`, `Tab`. No I/O, no egui imports. |
| `src/parsing.rs` | CLEF/file I/O and data processing: `parse_clef_line`, `load_file`, `render_template`, `build_template_summary`, timestamp helpers. |
| `src/app.rs` | `App` struct, `Default` impl, and business logic methods: `open_file`, `load`, `apply_filter`, `total_pages`, `page_records`. No direct egui rendering. |
| `src/ui.rs` | All egui rendering: `impl eframe::App for App`, `show_logs_tab`, `show_templates_tab`, `show_detail`, `date_filter_ui`, widget helpers (`badge`, `body`, `mono`, `small_gray`). |

**Rules:**
- New types go in `types.rs`
- New parsing/aggregation logic goes in `parsing.rs`
- New filter or state logic goes in `app.rs`
- New widgets or render functions go in `ui.rs`
- Do not add egui imports to `types.rs`, `parsing.rs`, or `app.rs`

**Data flow:**
1. User opens a `.clef` / `.log` / `.json` file via `rfd` native file dialog
2. `load_file` reads it line-by-line; each line is parsed as a CLEF JSON object by `parse_clef_line`
3. Parsed `LogRecord`s are stored in `App::records`
4. `apply_filter` rebuilds `App::filtered` (indices into `records`) whenever any filter changes; it also recomputes `LevelStats` and `TemplateSummary`
5. `eframe::App::update` is the immediate-mode render loop — it reads `filtered` and renders the current page

**Key types** (all in `src/types.rs`):
- `LogRecord` — one parsed log line; holds UTC + local timestamps, `Level`, rendered message, raw `serde_json::Value`
- `Level` — enum with colour/label helpers; mapped from Serilog `@l` field
- `App` — all UI + filter state; defined in `src/app.rs`
- `DateFilter` — date/time range filter state with enable toggle; rendered by `date_filter_ui` in `src/ui.rs`
- `LevelStats` / `TemplateSummary` — derived aggregates rebuilt on every `apply_filter` call

**CLEF fields used:**
| Field | Meaning |
|-------|---------|
| `@t`  | timestamp (RFC3339 or naive) |
| `@l`  | level string |
| `@mt` | message template (Serilog-style `{Property}` placeholders) |
| `@m`  | pre-rendered message (preferred over `@mt` when present) |
| `@x`  | exception string |

**Dependencies:**
| Crate | Version | Role |
|-------|---------|------|
| `eframe` | 0.29 | OS window + render loop host; entry point via `eframe::run_native`; you implement `eframe::App::update` |
| `egui` | 0.29 | Immediate-mode UI toolkit — no retained widget state; call widget fns (`ui.label`, `ui.button`, etc.) each frame inside `update` |
| `serde_json` | 1 | Parses each CLEF log line into a dynamic `serde_json::Value`; fields extracted by key lookup |
| `rfd` | 0.15 | Native OS file-open dialog (Rusty File Dialog); no custom UI needed for file picking |
| `chrono` | 0.4 | Parses `@t` timestamps (RFC3339 / naive) into `DateTime<Utc>`; converts to local time for display; drives date-range filter |

**UI layout:**
- Top panel: toolbar (Open, tab switcher, search, date range, level toggles)
- Right panel: stats sidebar (error count, level bar chart, level table)
- Central panel: either Logs tab (paginated rows, inline expand-to-detail on click) or Templates tab (aggregated message templates, click to filter logs)
