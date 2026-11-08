# CleanMeBro

A glassy macOS menu bar companion written in Rust (eframe/egui). It combines a
Mac health panel, Claude plan limits, an AI-judged Cleanup and Space Lens.

## Features

| Area | What it does |
|---|---|
| **Menu bar panel** | Tray icon (title = Claude session %) opens a 440 pt glass panel: Mac Health verdict, Claude card, tiles for disk, memory, battery, CPU, network, connected devices, plus "Today's Recommendation". Clicking a tile opens a detail sheet beside the panel. |
| **Claude** | Plan and limits from the Claude CLI (`claude auth status`, `claude -p /usage`, which spends no tokens). Local token activity is counted from `~/.claude/projects` logs (counts only). |
| **Cleanup** | Finds junk by rule (System Junk, Xcode, app and developer caches, AI clutter). Claude judges each **path** (never file contents) as safe / ask / keep. Anything unclear waits for your OK. Items go to the Trash by default. |
| **Space Lens** | Measures a folder tree, then shows it as a sorted list and a bubble map. Selected items go to the Trash after review. |

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

`--view` values: `panel`, `storage`, `memory`, `battery`, `cpu`, `network`,
`devices`, `claude-detail`, `cleanup`, `results`, `lens`, `claude`.

## Project layout

```
.
├── Cargo.toml / Cargo.lock
├── src/
│   ├── main.rs          entry point, CLI flags, --scan-report
│   ├── app.rs           app shell: tray icon, panel ⇄ full window switching
│   ├── native.rs        macOS: NSVisualEffectView glass, menu-bar app, thermal state
│   ├── glass.rs         drawing kit: glass cards, gradients, glows, vector icons
│   ├── mechanics.rs     numbers/thresholds from docs/MECHANICS.md
│   ├── monitor.rs       live health readings (background thread)
│   ├── claude.rs        Claude plan, limits, local token counts
│   ├── cleanup.rs       junk rules, AI judge, removal
│   ├── spacelens.rs     folder-tree measuring
│   ├── ui_panel.rs      drop-down panel + detail sheets
│   ├── ui_cleanup.rs    Cleanup screens
│   ├── ui_lens.rs       Space Lens screens
│   └── ui_claude.rs     Claude page
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
