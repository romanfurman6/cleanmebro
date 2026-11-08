# CleanMeBro — mechanics sheet

What the rebuild does and where each rule or number came from. Sources are the
reference app bundle, read statically.
"Estimated" means the value was looked for in the code and not found (the Swift
view models of the menu helper did not decompile into readable thresholds).

## Menu bar panel

Layout from the person's screenshots and the helper's strings (`Monitor_*` keys in
`Localizable.strings`, referenced from the binary, e.g. "Test Speed" at 0x1003258d0
used by `sub_1001a44ec`, "Pressure" at 0x100325900 used by `sub_1001a5118`).

| Tile | Shows | Detail sheet (click) |
|---|---|---|
| Mac Health header | worst of disk / memory / thermal / battery level ("Good", "Fair", "Needs care") — glow assets `HeathStatus_glow_{good,fair,critical}` (`sub_1000294b0`) | — |
| Claude card (replaces Protection) | plan, session % and week % with reset time | plan limits, local tokens (5h/today/7d + 7-day bars), "what uses your limits" |
| Macintosh HD | Available: free space, "Free Up" → Cleanup | ring by Applications/Documents/Downloads/Desktop/Other, Health (SMART), Used %, Trash, Disk Map |
| Memory | Pressure % | ring Active/Wired/Compressed/Available, Pressure, Swap File |
| Battery | %, Not charging / Charging / Fully charged / time left | charge ring, cycles (of design count), Health %, Temperature |
| CPU | Load %, thermal word | User/System/Available ring + sparkline, Temperature (thermal state), Uptime |
| Network | name, ↑/↓ per sec, "Test Speed" | Upload/Download, Test Your Connection with "Good for:", connection info |
| Connected Devices | up to 2 Bluetooth devices with battery | full device list |
| Today's Recommendation | first match of: Claude session ≥ 80 %, memory pressure, uptime > 7 days, Claude junk > 50 MB, low disk, default "quick clean" | — |

Thresholds and rules — **decompiled** (menu helper frameworks):

| Rule | Value | Source |
|---|---|---|
| Low-disk alert threshold | preset nearest to **capacity ÷ 10**, presets **2, 5, 10, 20, 30, 50, 100 GB**; **1 TB** when capacity is unknown | `SharedPreferences` `sub_5764` (key `DiskSpaceThreshold`, fallback `0xE8D4A51000`), `sub_1c60` (`umulh …0xCCCD; lsr #3` = ÷10, then nearest by `fabd`), table at `0x1c1e0` built by `sub_1148` |
| Disk level in the panel | free < threshold → Fair; free < threshold ÷ 2 → Needs care | first half decompiled (OutOfDiskSpace factor uses `DiskSpaceThreshold`); the ÷2 step is estimated |
| Trash alert threshold | **TrashSizeLimits[2] = 2 GB**; list 0.5, 1, 2, 3, 5, 10, 15, 20, 30, 50, 100 GB | `SharedPreferences` `sub_5594` (key `TrashSizeThreshold`, `ldr [x8,#0x30]` = index 2), table at `0x1c280` built by `sub_2000`; matches the original screenshot "Used: 46,1 MB of 2 GB" |
| Bluetooth low battery | device reported only when battery **≤ 19 %** (`cmp w25, #0x13` → "above threshold"); no-battery (0xff) and Apple mobile devices skipped | `HealthIssuesController` `sub_a308` (BluetoothLowBatteryIssueProducer) |
| Restart reminder | uptime > **604800 s (7 days)** | `AssistantDomain` `sub_1ba28` (UptimeHealthFactor stores `0x4122750000000000` = 604800.0 at +0x48 `noRecentRestartInterval`) |
| Uptime days | trunc(uptime seconds ÷ **86400**) | `AssistantDomain` `sub_1bbc0` (`fdiv` by `0x40F5180000000000`, `frintz`) |
| Recommendation priorities | drive health **100**, battery health **80**, out of disk space **20**, Trash overfill **5** (highest shown first) | `AssistantDomain` `sub_1ecc4` / `sub_1bdcc` / `sub_19bec` / `sub_1ae60` (argument to `sub_20544`) |
| Recommendation comes back after | 604800 s (7 days) | `AssistantDomain` `sub_1a98c` (`fcmp elapsed, 604800.0; cset gt`) — read, not used yet (no dismiss button) |
| Heavy Chrome usage | 0.2 (20 %) | `SharedPreferences` `sub_68ec` — not used (no Chrome monitor here) |
| Large external drive | from 128 GB | `SharedPreferences` `sub_9924` — not used |
| Memory "Pressure: N %" | (wired + compressed) ÷ physical memory | `SystemResourcesMonitor` `sub_90b8` (`host_statistics64` flavor 4; `fdiv (wire+compressor), total`) |
| Memory used / available | used = (internal − purgeable) + wired + compressed; available = physical − used; "Active" shown = internal − purgeable | `SystemResourcesMonitor` `sub_90b8` (vm_statistics64 offsets 0x8c, 0x58, 0x0c, 0x80) |
| Memory pressure level | `kern.memorystatus_vm_pressure_level`: 1 → normal, 2 → warning, anything else → critical (byte table `0x02010003`) | `SystemResourcesMonitor` `sub_9504` |
| Battery health level | macOS's own text: "Good" → good, "Fair" → fair, "Poor" → poor; "Check Battery" / "Permanent Battery Failure" or `PermanentFailureStatus` ≠ 0 → service | `BatteryMonitor` `sub_25d4`, inputs read in `sub_2a54` (`BatteryHealth`, `BatteryHealthCondition`) and `sub_1bc8` (`PermanentFailureStatus`) |
| Battery health % | `AppleRawMaxCapacity` ÷ `DesignCapacity` on Apple Silicon (`MaxCapacity` otherwise); unavailable when either is 0 | `BatteryMonitor` `sub_1bc8` |
| Battery temperature | `Temperature` ÷ 100 (°C) | `BatteryMonitor` `sub_1bc8` (`fdiv` by 100.0f `0x42c80000`) |
| Battery time left | `AvgTimeToEmpty` / `AvgTimeToFull` (minutes), 65535 = unknown; power-source "Time to Empty" / "Time to Full Charge" as fallback | `BatteryMonitor` `sub_1bc8`, `sub_2a54` |
| Plugged in / charging | power source "Power Source State" = "AC Power"; "Is Charging" | `BatteryMonitor` `sub_2a54` |
| Detail list row heights | 68, 76, 125, 184 pt by row kind | menu helper `sub_1000bcd4c` (constants 0x4051…, 0x4053…, 0x405F4…, 0x4067…) — layout only |
| Battery health warning | < 80 % | string `Monitor_Battery_Chart_Cycles_Title_Tooltip` ("retain up to 80%") |
| Temperature words | macOS thermal state Nominal/Fair → Cool/Warm, Serious → Hot, Critical | estimated: the original reads CPU temperature through private IOHID sensors (`SystemResourcesMonitor` `-[CMIOHIDProcessorTemperature getCPUTemperature:]`), which this rebuild doesn't use |
| Battery temperature "not optimal" | ≥ 35 °C | estimated |
| Refresh | every 2 s (battery 10 s, devices/Wi-Fi 60 s, folder sizes 10 min) | estimated |
| Speed test | 25 MB download, 8 MB upload ("downloads a small test file") | string `Monitor_Detailed_Network_TestSpeed_Legend_Description`; sizes estimated |
| Good for (Mbps) | Messaging 0.5, Audio calls 1, Music 2, Video calls 4, Video streaming 10, Online gaming 25 | labels from `Monitor_Detailed_Network_TestSpeed_Legend_*`; speeds estimated |

Today's Recommendation picks the highest-priority match: drive health (100),
battery health (80), Claude session ≥ 80 % (60, ours), low disk (20), memory
pressure warning or worse (15, ours), restart after 7 days (10, ours), Trash > 2 GB (5), Claude junk
> 50 MB (4, ours), else the default "quick clean".

## Claude info (new)

* Plan and sign-in: `claude auth status --json` (`subscriptionType`, `loggedIn`).
* Limits: `claude -p /usage --output-format json --no-session-persistence` — a local
  slash command that spends no tokens. Lines `Current …: N% used · resets …` become
  bars; `Last 24h · …` blocks become "What uses your limits". Refreshed every 5 min
  and on demand. The session % is also shown as the menu bar title.
* Local activity: `usage` fields of assistant messages in `~/.claude/projects/*/*.jsonl`
  from the last 7 days (counts only; message ids de-duplicated).

## Cleanup

Groups follow the original's scan tasks (`SystemJunkScanning`, `AIToolScanning`,
`TrashJunkScanning` frameworks).

**AI Clutter — Claude** (`AIToolScanning` catalog, functions `sub_7118` Claude Code and
`sub_7308` Claude Desktop):

| Path | Kind | Rule |
|---|---|---|
| `~/.claude/debug` | Logs | safe |
| `~/.claude/cache`, `telemetry`, `stats-cache.json`, `mcp-needs-auth-cache.json`, `plugins/install-counts-cache.json` | Caches | safe |
| `~/.claude/shell-snapshots/*` older than 2 days | Temp Files | safe (estimated age) |
| `~/.local/share/claude/versions/*` except the target of `~/.local/bin/claude` | Stale Versions | safe (`ActiveVersionResolver`, `StaleVersionGuard`) |
| `~/Library/Logs/Claude` | Logs | safe |
| `~/Library/Caches/claude-cli-nodejs`, `com.anthropic.claudefordesktop(.ShipIt)` | Caches/Logs | safe |
| Claude Desktop Chromium caches (Cache, Code Cache, GPUCache, Dawn*/Graphite* caches, Service Worker/CacheStorage, Shared Dictionary) | Caches | safe, ask while Claude Desktop is open |
| `…/Claude/vm_bundles`, `…/Claude/claude-code-vm` | Downloads | ask (not in the original catalog) |
| `~/.claude/projects/<p>` | Projects | **kept** when its folder exists and was used within 30 days; ask otherwise |
| `~/.claude/file-history/*`, `~/.claude/uploads/*` older than 30 days | Old sessions | ask |

A project folder name is its path with every non-alphanumeric character turned
into `-`; the app walks the disk to find the real folder before deciding.

**Other AI tools** (same catalog): Codex `.codex/cache/*`, Gemini/Antigravity and
Chrome DevTools MCP browser caches, Cursor agent `*.log`, OpenCode logs — safe.
Local models (Ollama, LM Studio, Hugging Face) — always ask (the original warns
"Removing local models may affect your projects").

**System Junk**: every entry of `~/Library/Caches` (CMUserCachesScanTask) — safe,
`com.apple.*` ask, iCloud/sign-in caches kept; logs in `~/Library/Logs` and sandboxed
`Data/Library/Logs`; sandboxed caches `~/Library/Containers/*/Data/Library/Caches`.

**App rules** (hard-coded paths in `SystemJunkScanning`): Adobe Media Cache Files,
Spotify `PersistentCache/Storage`, Telegram `tdata/user_data` (ask), WeChat (ask),
Mail Downloads, Electron/Chromium app caches under `~/Library/Application Support/*`
(CMElectronAppsCacheScanTask); plus Steam app/depot cache and iOS backups (ask).

**Xcode Junk** (CMXcode* tasks): DerivedData, DocumentationCache, ModuleCache,
Previews, iOS Device Logs, CoreSimulator caches — safe; DeviceSupport — older versions
safe, newest ask; Archives — ask.

**Developer caches**: Gradle (original), npm, pip, Go, Bun, Yarn — safe; Cargo
registry, CocoaPods specs, pnpm store — ask.

**Never touched** (hard keep, checked again before removal): Documents, Desktop,
Pictures, Movies, Music, Projects, Developer, Dropbox, iCloud Drive, Keychains,
Messages, Mail, `.ssh`, `.gnupg`, Claude settings/skills/plugins/history/sessions,
any folder holding `.git`.

**AI judge**: items ≥ 1 MB that aren't hard-kept are sent to
`claude -p … --model sonnet --tools "" --system-prompt <judge>` in batches of 120 —
path (home shown as `~`), size, age, group, the rule's verdict and whether the app is
open. Never file contents. Merge: Claude may always be more careful; it may relax a
rule's "ask" to "safe" unless the app is open; hard keeps can't be overridden.
Safe items start selected; "ask" items wait for Allow/Keep.

**Removal**: moved to the Trash by default (can be put back); "Delete right away"
deletes; emptying the Trash is permanent and confirmed.

## Disk Map

Walks a chosen place (Home by default, or the whole disk) on one volume, sizes by
allocated blocks, keeps files ≥ 8 MB individually and folds the rest into "Other
items". Shown as a drill-down squarified treemap of the current folder (one
colour per top-level folder; click a folder to zoom in, tick a block's corner to
select it). System folders can't be selected; removal goes to the Trash after a
review list.
