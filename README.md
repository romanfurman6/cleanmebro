# CleanMeBro

A glassy macOS menu bar companion written in Rust (eframe/egui). It combines a
Mac health panel, Claude plan limits, an AI-judged Cleanup and Disk Map.

## Features

| Area | What it does |
|---|---|
| **Menu bar panel** | Tray icon (title = Claude session %) opens a 440 pt glass panel of widgets: Mac Health verdict, Claude card, tiles for disk, memory, battery, CPU, network, connected devices, plus "Today's Recommendation". Clicking a tile opens a detail sheet beside the panel. Gear → **Customize widgets…** turns widgets on/off and reorders them; the choice is saved. |
| **Claude** | Plan and limits from the Claude CLI (`claude auth status`, `claude -p /usage`, which spends no tokens). Local token activity is counted from `~/.claude/projects` logs (counts only). |
| **Cleanup** | Finds junk by rule (System Junk, Xcode, app and developer caches, AI clutter). Claude judges each **path** (never file contents) as safe / ask / keep. Anything unclear waits for your OK. Items go to the Trash by default. |
| **Disk Map** | Measures a folder tree, then shows it as a drill-down treemap. Selected items go to the Trash after review. |

Every rule and threshold, with its source, is in [docs/MECHANICS.md](docs/MECHANICS.md).

## Requirements

- macOS (uses AppKit for the glass material and menu bar behavior)
- Rust stable (edition 2021)
- Optional: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) signed in, for the Claude card and the AI judge

## Build and run

```bash
cargo run --release            # menu bar app
cargo test                     # unit tests
```

### Test and preview modes

```bash
# Read-only Cleanup check: prints what would be offered, removes nothing
cargo run --release -- --scan-report
cargo run --release -- --scan-report --ai     # also ask Claude to judge

# Render a view with sample data to a PNG and exit
cargo run --release -- --screenshot out.png --view panel
```

`--view` values: `panel`, `customize` (Settings › Widgets), `settings-tools`,
`storage`, `memory`, `battery`, `cpu`, `network`, `devices`, `claude-detail`,
`cleanup`, `results`, `lens`, `lens-live`, `claude`.

## Architecture

The app is a small **core** (the shell) plus independent **tools**. Each tool
is one folder under `src/tools/` and implements the `Tool` trait
(`src/core/tool.rs`). A tool can offer any of: panel widgets, detail sheets,
a page in the full window, recommendations, gear-menu entries and the
menu-bar title.

- Tools never import each other (a test enforces it). To link to another tool
  they ask the core by id, e.g. `cx.open_page("cleanup", "claude")`; if that
  tool isn't installed, nothing happens.
- Shared code that several tools need lives in `src/kit/` and knows no tool.
- The panel lays widgets out on a 4-column grid: each widget spans 1, 2 or 4
  columns and reports its own height. The panel grows with its widgets and
  scrolls past the screen height.

### Adding a tool

1. Create `src/tools/<name>/mod.rs` with a struct implementing `Tool`
   (`id`, `name`, `badge` are required; everything else is optional).
2. Add one line to `all()` in `src/tools/mod.rs`.

Its widgets appear in the gallery of **Settings › Widgets**, it gets an
on/off switch in **Settings › Tools**, and if it has a page it gets a sidebar
entry. Nothing in `src/core/` changes.

## Project layout

```
.
├── Cargo.toml / Cargo.lock
├── src/
│   ├── main.rs              entry point, CLI flags
│   ├── core/                the shell; knows tools only through the trait
│   │   ├── tool.rs          Tool trait, widget specs, actions, Cx
│   │   ├── shell.rs         tray icon, panel ⇄ full window, actions, screenshots
│   │   ├── panel.rs         widget grid, recommendation, footer, gear menu, detail sheet
│   │   ├── editor.rs        Settings: drag-and-drop widget editor, tools on/off
│   │   ├── window.rs        full window: sidebar of tool pages and Settings
│   │   ├── layout.rs        4-column widget packing
│   │   └── settings.rs      saved widget order, visibility and tools turned off
│   ├── kit/                 shared by all tools
│   │   ├── glass.rs         drawing kit: glass cards, gradients, glows, vector icons
│   │   ├── controls.rs      shared page controls (round action button)
│   │   ├── native.rs        macOS: glass material, menu-bar app, thermal state, Trash
│   │   ├── fs.rs            folder sizes, protected paths, move to Trash, disk space
│   │   ├── claude_code.rs   Claude Code CLI and project discovery
│   │   └── mechanics.rs     numbers/thresholds from docs/MECHANICS.md
│   └── tools/
│       ├── mod.rs           the tool list (the only place that names tools)
│       ├── system/          Mac health: readings, widgets, detail sheets
│       ├── claude/          Claude limits: card, sheet, page, tray title
│       ├── cleanup/         junk rules, AI judge, removal, Cleanup page, --scan-report
│       └── lens/            Disk Map: folder measuring and page
├── docs/
│   ├── MECHANICS.md     rules, thresholds and where each came from
│   └── mechanics.json   same data, machine-readable
```

Screenshots are not kept in git. Render any view locally with
`--screenshot <file.png> --view <name>`.

## Safety

- Cleanup never reads file contents; only paths, sizes and ages are sent to Claude.
- Hard-kept locations (Documents, Desktop, Projects, Keychains, `.ssh`, any folder
  with `.git`, Claude settings, and so on) can't be overridden by rules or the AI.
- Removal moves to the Trash unless you choose "Delete right away".

## Known gaps

- Some health thresholds are estimated (marked in `docs/MECHANICS.md`).
- Live glass and tray behavior hasn't been visually verified end to end.
