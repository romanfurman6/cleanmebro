# UI rules

Approved by Roman on 2026-11-12. The tokens live in `src/kit/glass.rs` (`sp::*`, `ty::*`, `R_*`); use them, not raw numbers.

| # | Rule | Token |
|---|------|-------|
| R1 | Content starts 24 pt from the window edge, header included. | `sp::PAGE_EDGE` |
| R2 | Cards: padding 16, gap 16, radius 12. Title, 12, content. A section title sits 12 above its card. | `sp::L`, `sp::M`, `R_CARD` |
| R3 | Two row heights: settings rows 40 (label left, control right, hairline between), list rows 44 (icon, name, detail). | `sp::SETTINGS_ROW_H`, `sp::ROW_H` |
| R4 | Rows with a bar: 36 tall, name and value on one line, a 4 pt bar 8 pt under them. | `sp::BAR_ROW_H` |
| R5 | Controls are 28 inside cards and 32 in page headers. | `sp::CONTROL_H`, `sp::HEADER_CONTROL_H` |
| R6 | One empty state: centred, 56 icon chip, lead title, body up to 400 wide in dim, one secondary button. Inside a card: one caption-size line in the middle. | `sp::XXL` around it |
| R7 | Chips say a state (Normal, Charging, In menu bar), never an action: 11/600 on a 16% tint. | `chip()` |
| R8 | Numbers in lists and tables are right-aligned. | |
| R9 | A title with a detail line under it ("Ollama" over "com.ollama.ollama") keeps 6 between the lines; such rows are 52 tall. Columns in a row sit 16 apart. | `sp::LINE_GAP`, `two_lines_y()`, `sp::ROW2_H`, `sp::COL_GAP` |
| R10 | Text the app writes (times, states, counts) is never cut with "…": it wraps or the layout changes. Only names and paths people bring (apps, files, projects) may be cut. Find cut text with `CMDMEOW_FIT_LOG=1`. | `text_wrap()` |

**Spacing scale:** 4, 8, 12, 16, 24, 32 (`sp::XS` to `sp::XXL`). Nothing in between.

**Type ramp:** hero 28/600, number 22/600, lead 15/600, title 13/600, body 13/400, label 12/400, caption 11/400 (`ty::*`). Bold 11 and 12 are allowed for chips and small values. Only exceptions: the speed dial (52) and the app mark (40).
