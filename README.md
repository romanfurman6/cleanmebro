<p align="center">
  <img src="docs/images/icon.png" width="128" alt="Cmd+Meow icon">
</p>

<h1 align="center">Cmd+Meow</h1>

<p align="center">
  <a href="https://cmdmeow.furman.cat">Website</a> ·
  <a href="https://github.com/romanfurman6/cmdmeow/releases/latest">Download</a> ·
  <a href="https://github.com/romanfurman6/cmdmeow/actions/workflows/macos.yml">Latest build</a>
</p>

A macOS menu bar app that keeps a Mac in shape: cleaner, uninstaller, startup
items, disk map, fan control, screenshots and screen recording, clipboard
history and keep-awake in one place. Written in Rust; everything runs locally.

<p align="center">
  <img src="docs/images/panel.png" width="380" alt="The menu bar panel">
</p>

## What it does

- **Cleanup** finds caches, logs and other junk; Claude can review each path (never file contents). Items go to the Trash.
- **Disk Map** shows what takes space as a treemap.
- **Uninstaller** removes apps with their leftovers; **Startup** turns off what starts with your Mac.
- **Capture** takes screenshots with an editor and records the screen with audio and camera.
- **Displays** sets brightness and speaker volume of external monitors (DDC/CI, or dimming the picture when a monitor can't be reached), from the panel or the keyboard's own keys.
- **Clipboard**, **Fan Control**, **Keep Awake** and **Claude** limits round it out. Any tool can be turned off.
- Links for scripts, Raycast or Shortcuts (`open cmdmeow://…`; the Dev build answers `cmdmeow-dev://`):
  - `awake/on`, `awake/off`, `awake/toggle`
  - `capture/area`, `window`, `screen`, `delayed`, `text`, `color`, `ruler`, `record`, `history`
  - `dictation/toggle`, `start`, `stop`, `cancel`
  - `open/<tool>` with an optional page, e.g. `open/disk-map`, `open/capture/settings`, `open/settings`
- Keep Awake's switch can also get its own shortcut on its page.

![Disk Map](docs/images/lens.png)

## Requirements

- macOS (uses AppKit for the glass material and menu bar behavior)
- Rust stable (edition 2021)
- Optional: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) signed in, for the Claude card and the AI judge

## Build and run

```bash
git clone https://github.com/romanfurman6/cmdmeow
cd cmdmeow
cargo run --release            # menu bar app
cargo test                     # unit tests
```

### Build the app

```bash
scripts/bundle-mac.sh              # target/bundle/CmdMeow.app + .zip
UNIVERSAL=1 scripts/bundle-mac.sh  # Apple silicon + Intel
```

CI (`.github/workflows/macos.yml`) runs the tests and builds a universal
`CmdMeow.app` on every push and PR; download it from the run's
**Artifacts**.

### Releases

Versions follow semver; `Cargo.toml` is the source of truth. The app's
Info.plist gets it as `CFBundleShortVersionString`, the commit count as
`CFBundleVersion`, and Settings › General shows both ("Cmd+Meow 0.2.0 (57)").

```sh
scripts/release.sh patch   # or minor, major, or an exact 0.2.0
```

On a clean, up-to-date `main` this bumps the version, commits, tags `vX.Y.Z`
and pushes. The tag runs `.github/workflows/release.yml`: tests, universal
build, then a GitHub Release with generated notes and `CmdMeow.dmg`,
`CmdMeow.zip`, `SHA256SUMS.txt`. The asset names never change, so
`https://github.com/romanfurman6/cmdmeow/releases/latest/download/CmdMeow.zip`
always points to the newest version (the website's Download button uses it).

### Signing and notarization

Release builds are signed with **Developer ID Application** and notarized when
these repository secrets are set (Settings › Secrets and variables › Actions);
without them the release is ad-hoc signed and users must right-click › Open.

| Secret | Value |
| --- | --- |
| `MACOS_CERT_P12` | `base64 -i DeveloperID.p12` (certificate + private key) |
| `MACOS_CERT_PASSWORD` | password of that .p12 |
| `NOTARY_KEY_P8` | contents of the App Store Connect API key `AuthKey_XXXX.p8` |
| `NOTARY_KEY_ID` | that key's Key ID |
| `NOTARY_ISSUER_ID` | Issuer ID from App Store Connect › Users and Access › Integrations |

The signed app runs with the hardened runtime and `assets/CmdMeow.entitlements`
(microphone, camera, Apple Events).

The app is ad-hoc signed, not notarized. On first launch, right-click it and
choose **Open**, or run `xattr -dr com.apple.quarantine CmdMeow.app`.

Coming from CleanMeBro (the old name; bundle id is now
`com.romanfurman6.cmdmeow`): the new bundle id makes it a new app to macOS. Grant
Screen Recording, Accessibility and Full Disk Access again, turn **Open at
login** back on, and delete the old app. Settings, clipboard history and the
sharing key move over on first launch. The fan helper keeps its old id, so an
installed helper keeps working.

The icon artwork is `assets/icon/AppIcon-art.png`; regenerate `assets/AppIcon.icns`
with `python3 scripts/make-icon.py` (needs `pip install pillow`). The menu bar
icon is drawn from vector paths in `src/kit/brand.rs`.

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
`cleanup`, `results`, `lens`, `lens-live`, `claude`, `clipboard`,
`clipboard-settings`, `clipboard-quick`, `clipboard-list`.

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
cmdmeow/
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
│   │   ├── brand.rs         the Cmd+Meow mark as vector paths (menu bar icon)
│   │   ├── controls.rs      shared page controls (round action button)
│   │   ├── native.rs        macOS: glass material, menu-bar app, thermal state, Trash
│   │   ├── fs.rs            folder sizes, protected paths, move to Trash, disk space
│   │   ├── claude_code.rs   Claude Code CLI and project discovery
│   │   └── mechanics.rs     numbers/thresholds from docs/MECHANICS.md
│   └── tools/
│       ├── mod.rs           the tool list (the only place that names tools)
│       ├── system/          Mac Health: readings, widgets, detail sheets
│       ├── cleanup/         junk rules, AI judge, removal, --scan-report
│       ├── lens/            Disk Map: folder measuring and treemap
│       ├── uninstaller/     apps, leftovers, drop-to-uninstall widget
│       ├── startup/         login items, launch agents and daemons
│       ├── capture/         screenshots, editor, screen recording, sharing
│       ├── clipboard/       clipboard history and quick paste
│       ├── fan/             fan speeds, temperatures, helper
│       ├── keep_awake/      keep the Mac awake, also while Claude works
│       └── claude/          Claude limits: card, sheet, page, tray title
├── vendor/eframe/        eframe 0.37 with one patch (see Cargo.toml)
├── docs/
│   ├── images/          README screenshots
│   ├── MECHANICS.md     rules, thresholds and where each came from
│   └── mechanics.json   same data, machine-readable
├── assets/
│   ├── AppIcon.icns     app icon, all sizes (generated)
│   └── icon/AppIcon-art.png icon artwork
├── scripts/
│   ├── bundle-mac.sh    build CmdMeow.app + zip
│   └── make-icon.py     artwork → AppIcon.icns
└── .github/workflows/macos.yml   CI: test, build, upload the app
```

The images in `docs/images/` are rendered from sample data with
`--screenshot <file.png> --view <name>`.

## Safety

- Cleanup never reads file contents; only paths, sizes and ages are sent to Claude.
- Hard-kept locations (Documents, Desktop, Projects, Keychains, `.ssh`, any folder
  with `.git`, Claude settings, and so on) can't be overridden by rules or the AI.
- Removal moves to the Trash unless you choose "Delete right away".

## Known gaps

- Some health thresholds are estimated (marked in `docs/MECHANICS.md`).
- Live glass and tray behavior hasn't been visually verified end to end.

## License

MIT, see [LICENSE](LICENSE).
