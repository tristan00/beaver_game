# Trace: pattern summaries and conventions to findings

Report: `city-builder-interfaces.md`. Every sentence of every pattern summary and every convention is listed below, in order, including each convention's bold opening claim and its "Confidence:" sentence. "Finding" gives the finding ID and its line in the report; each finding line carries its own source link. Confidence sentences are checked against the rule at report line 8 (*medium* = three or more games' findings, at least one Single or Strong; *low* = one or two games, or only Weak findings).

## Pattern summaries

| Section | Sentence (report wording) | Supporting finding (ID, report line) | What the finding states |
|---|---|---|---|
| §1 (l.24) | Timberborn, RimWorld and Anno 1800 keep stockpile or finance figures on the main screen. | 1-TB l.18; 1-RW l.19; 1-AN l.20 | Inventory on main screen; resource list top-left; balance and income in top bar |
| §1 | Only Anno 1800 (Ludo) puts them in the same bar as the time; RimWorld puts the resource list top-left and the time controls bottom-right, and Timberborn's positions are not stated. | 1-AN l.20; 1-RW l.19; 1-TB l.18 | Balance far left and time centre of top bar; list top-left, time bottom-right; "Screen positions not stated" |
| §1 | Timberborn colours satisfied needs green and colours forecast seasons; Cities: Skylines colours its demand bars by zone type. | 1-TB l.18; 1-CS l.21 | Satisfied needs green, Temperate blue/Drought yellow/Badtide red; green residential, blue commercial, orange industrial |
| §2 (l.36) | Timberborn, RimWorld, Banished and Anno 1800 have categorised build menus, in different places (RimWorld bottom-left, Banished lower right, Anno at the left of the bottom bar). | 2-TB l.28; 2-RW l.29; 2-BA l.30; 2-AN l.31 | Tool groups; Architect tabs bottom-left; category icons lower right; hammer far left of bottom bar, category tabs |
| §2 | Anno opens its menu with B and Dwarf Fortress menus open by letter key. | 2-AN l.31; 2-DF l.33 | "hotkey B"; "modal menus by letter key" |
| §2 | Only Timberborn is documented with cycling of groups and tools, using Ctrl+scroll and Shift+scroll. | 2-TB l.28 | "Ctrl+scroll cycles groups, Shift+scroll cycles tools" (no other §2 finding mentions cycling) |
| §3 (l.46) | Timberborn's preview is green when valid and red when blocked (single source). | 3-TB l.40 | "preview green when valid, red when blocked", single source |
| §3 | Dwarf Fortress blueprints and RimWorld plans mark work that workers ignore; Factorio offers a separate ghost placement (Shift+LMB) that has no range limit. | 3-DF l.43; 3-RW l.44; 3-FA l.41 | "dwarves ignore them until converted"; "plans colonists do not act on"; "Shift+LMB builds ghosts; ghosts have no range limit" |
| §6 (l.71) | Three models. | 6-RW–6-BA l.65–69 | Grouping of the five rows below |
| §6 | (a) per-worker settings: RimWorld work grid, DF labors. | 6-RW l.65; 6-DF l.66 | Colonist-by-work-type grid; "Labors enabled per dwarf" |
| §6 | (b) priorities on jobs or buildings: Timberborn workplaces and construction sites, DF designations, ONI priority levels. | 6-TB l.68; 6-DF l.66; 6-ONI l.67 | Per-workplace and construction-site priority; per-designation priority 1–7; priority levels on tasks |
| §6 | (c) profession assignment: Banished, weak. | 6-BA l.69 | "assign villagers to professions", Weak |
| §6 | Documented tie-breaks: RimWorld takes the left column; ONI uses a hidden fixed priority, or the closest task when "Enable Proximity" is on. | 6-RW l.65; 6-ONI l.67 | "ties go to the left column"; hidden fixed priority, closest task with Enable Proximity |
| §6 | Timberborn's is not established and DF's is not stated. | 6-TB l.68; 6-DF l.66 | "Tie-break order ... **not established**"; DF row names no tie-break |

§4, §5, §7 and §8 have no pattern summary; their content is used only in the conventions.

## Conventions

| # | Sentence (report wording) | Supporting finding (ID, report line) | Check |
|---|---|---|---|
| 1 | **Keep stockpile or finance figures on the main screen.** | 1-TB l.18; 1-RW l.19; 1-AN l.20 | Three games state it |
| 1 | Timberborn (Inventory), RimWorld (resource list top-left), Anno 1800 (balance and income in the top bar). | 1-TB l.18; 1-RW l.19; 1-AN l.20 | Each example matches its row |
| 1 | Their position differs between games, and only Anno 1800 (Ludo) shows them in the same bar as the time. | 1-AN l.20; 1-RW l.19; 1-TB l.18 | RimWorld splits list and time across corners; Timberborn positions not stated |
| 1 | Colour has been used to mark need state (Timberborn) and to tell demand types apart (Cities: Skylines demand bars). | 1-TB l.18; 1-CS l.21 | TB: green satisfied needs, coloured seasons; CS: colour = zone type |
| 1 | Confidence: medium for figures on the main screen (three games); low for the use of colour (two games, each using it differently). | 1-TB, 1-RW, 1-AN (TB and RW Single); 1-TB, 1-CS | Rule l.8: 3 games incl. Single → medium; 2 games → low |
| 2 | **Offer a categorised build menu.** | 2-TB l.28; 2-RW l.29; 2-AN l.31; 2-BA l.30 | Four games state it |
| 2 | Timberborn (tool groups), RimWorld (Architect tabs), Anno 1800 (hammer, category tabs), Banished (category icons). | 2-TB l.28; 2-RW l.29; 2-AN l.31; 2-BA l.30 | Each example matches its bullet |
| 2 | Anno opens it with B and Dwarf Fortress menus open by letter key; Cities: Skylines hotkeys are not sourced. | 2-AN l.31; 2-DF l.33; 2-CS l.32 | "hotkey B"; "by letter key"; "hotkeys are **not covered**" |
| 2 | Cycling groups and tools with Ctrl/Shift+scroll is documented only for Timberborn. | 2-TB l.28 | Only §2 finding mentioning cycling |
| 2 | Confidence: medium for categories (four games, RimWorld Strong); low for hotkeys (two games) and for scroll cycling (one game). | 2-RW Strong l.29; 2-AN, 2-DF; 2-TB | Rule l.8 |
| 3 | **Show a placement preview that is green when valid and red when blocked.** | 3-TB l.40 | Stated |
| 3 | Timberborn. | 3-TB l.40 | — |
| 3 | Confidence: low (one game, one source). | 3-TB l.40 "Single source" | Rule l.8 |
| 4 | **Let the player place plans separate from real orders.** | 3-DF l.43; 3-RW l.44; 3-FA l.41 | Blueprint mode, Plan tool, ghost placement are each separate from building |
| 4 | Dwarf Fortress blueprints are ignored by dwarves until converted with k. | 3-DF l.43 | Stated |
| 4 | RimWorld plans are not acted on by colonists. | 3-RW l.44 | Stated |
| 4 | Factorio places ghosts with Shift+LMB, with no range limit. | 3-FA l.41 | Stated |
| 4 | Confidence: medium (three games). | 3-DF Single, 3-RW, 3-FA Strong | Rule l.8 |
| 5 | **Right-click cancels an in-progress road placement.** | 3-CS l.42 | "click start, click end; right-click cancels" (Roads page) |
| 5 | Cities: Skylines. | 3-CS l.42 | — |
| 5 | **A Duplicate action places multiple copies.** | 3-TB l.40 | "'Duplicate' (B) places multiple copies" |
| 5 | Timberborn (B). | 3-TB l.40 | — |
| 5 | Confidence: low (one game each). | 3-CS; 3-TB | Rule l.8 |
| 6 | **When drawing roads, snap to existing roads, angles, the zoning grid and guidelines.** | 3-CS l.42 | "roads snap to roads, angles, the zoning grid and guidelines" |
| 6 | Cities: Skylines. | 3-CS l.42 | — |
| 6 | Confidence: low (one game). | 3-CS | Rule l.8 |
| 7 | **Selecting a building opens a panel with its settings, including its worker priority.** | 4-TB l.50 | Object panel with "workplace priority (5 levels)" |
| 7 | Timberborn (Object Detail, workplace priority in 5 levels). | 4-TB l.50 | — |
| 7 | **Selecting several objects of one type shows their count.** | 4-RW l.51 | "shows a count when several same-type objects are selected" |
| 7 | RimWorld (inspect pane). | 4-RW l.51 | — |
| 7 | Confidence: low (one game each). | 4-TB; 4-RW | Rule l.8 |
| 7 | Panel position is not a convention: RimWorld uses bottom-left and the Anno 1800 sources disagree. | 4-RW l.51; 4-AN l.52 | Bottom-left; Ludo bottom-left vs Gamepressure right |
| 8 | **Area orders are drag-selected.** | 5-DF l.56; 5-CS l.59 | DF rectangle drag; CS marquee |
| 8 | Dwarf Fortress (click, drag, click to make a rectangle), Cities: Skylines (marquee zoning tool). | 5-DF l.56; 5-CS l.59 | — |
| 8 | **Each area order has a matching removal tool.** | 5-DF l.56; 5-RW l.58; 5-TB l.57 | "x removes designations"; "each has a clear tool"; "mark/unmark areas" |
| 8 | Dwarf Fortress (x removes designations), RimWorld (a clear tool for each area), Timberborn (mark/unmark areas). | 5-DF l.56; 5-RW l.58; 5-TB l.57 | — |
| 8 | Confidence: low for drag-selection (two games); medium for removal tools (three games). | 5-DF, 5-CS; 5-DF, 5-RW, 5-TB | Rule l.8 (RimWorld and Timberborn drag behaviour is a Gap, so they are not counted for drag) |
| 9 | **Distinguish exclusive zones from overlapping area layers.** | 5-RW l.58 | "Zones are contiguous and cannot overlap; areas are overlapping layers" |
| 9 | RimWorld: zones are contiguous and cannot overlap; areas are overlapping layers. | 5-RW l.58 | — |
| 9 | Confidence: low (one game, though its finding is Strong on taxonomy). | 5-RW "Strong on taxonomy" | Rule l.8 |
| 10 | **Priority is a small set of levels.** | 6-RW l.65; 6-DF l.66; 6-ONI l.67; 6-TB l.68 | 1–4; 1–7; very high–very low; 5 levels |
| 10 | RimWorld (1 to 4), Dwarf Fortress (1 to 7), ONI ("very high" to "very low", sub-priority 1 to 9), Timberborn (5 levels). | same | — |
| 10 | **An explicit "never" level and a stated tie-break exist in two games.** | 6-RW l.65; 6-ONI l.67 | Both rows state both; DF and Timberborn rows do not |
| 10 | "Never": RimWorld (blank), ONI ("disabled"). | 6-RW l.65; 6-ONI l.67 | "blank = never"; "'disabled' are never done" |
| 10 | Tie-break: RimWorld (left column), ONI (hidden fixed priority, or closest task with "Enable Proximity"). | 6-RW l.65; 6-ONI l.67 | Stated |
| 10 | Confidence: medium for small level sets (four games); low for "never" and tie-break (two games each). | 6-RW, 6-DF, 6-ONI, 6-TB; 6-RW, 6-ONI | Rule l.8 |
| 11 | **Show alerts as a list on the main screen.** | 8-TB l.85; 8-FA l.86; 8-RW l.87; 8-AN l.88 | Alerts list; alerts beside quickbar; alerts top-right; notifications left side |
| 11 | Timberborn (Alerts list), Factorio (beside the quickbar), RimWorld (top-right), Anno 1800 (left side, recent notifications). | same | — |
| 11 | Counts per alert, cycling through targets and a Journal log are documented only for Timberborn; timed alerts and per-type toggles only for Factorio. | 8-TB l.85; 8-FA l.86 | Stated; no other §8 finding mentions counts, cycling, a log, timing or toggles |
| 11 | Confidence: medium for an on-screen alert list (four games); low for counts, cycling, log, timing and toggles (one game each). | 8-TB, 8-FA, 8-RW Single; 8-AN Weak | Rule l.8 |
| 11 | Whether selecting an alert centres the camera is not established. | 8-TB l.85 | Finding says "cycles through targets", not camera movement |
| 12 | **Provide a camera action that moves to a selected object.** | 7-TB l.79; 7-AN l.77; 7-RW l.80 | Focus centres; padlock locks camera to selection; click colonist to jump |
| 12 | Timberborn (Focus), Anno 1800 (padlock locks the camera to a selection), RimWorld (click a colonist in the top bar to jump to them). | same | — |
| 12 | **Provide an overview map.** | 7-AN l.77; 7-FA l.78 | Minimap; Tab/M map toggle |
| 12 | Anno 1800 (minimap), Factorio (Tab/M map toggle). | same | — |
| 12 | Confidence: medium for moving to a selection (three games); low for the overview map (two games). | 7-TB, 7-RW Single; 7-AN Weak; 7-AN, 7-FA | Rule l.8 |
| 12 | Pan and rotate defaults are not sourced. | §7 l.81 | "Pan and rotate defaults ... were not sourced" |

## Changes from the previous version (sentences with no supporting finding)

| Previous sentence | Problem | Action |
|---|---|---|
| Conv. 1 "Show stockpile or finance figures **together with time**", citing RimWorld | 1-RW puts the resource list top-left and the time bottom-right; only 1-AN puts them together | Narrowed to "on the main screen"; "together with time" kept only for Anno |
| Conv. 1 / §1 pattern "use colour for need or demand state", citing Cities: Skylines | 1-CS colours the bars by zone type, not by demand state | Narrowed: CS colour tells demand types apart |
| Conv. 1 "Confidence: medium" for the whole convention | No confidence rule; colour part has two games | Split: medium (figures), low (colour), per rule l.8 |
| Conv. 2 Cities: Skylines as a categorised build menu | 2-CS only says zoning tools are listed in the toolbar | Removed from the examples; kept only as "hotkeys not sourced" |
| Conv. 2 / §2 pattern "keyboard tool cycling" | 2-TB documents Ctrl/Shift+**scroll** | Narrowed to "Ctrl/Shift+scroll" |
| Conv. 3 / §3 pattern: Factorio ghosts "mark work without it being acted on at once" | 3-FA does not say ghosts are left unbuilt | Narrowed to "separate ghost placement with no range limit"; split into convention 4 |
| Conv. 5 "snap to the grid and to existing structures" | 3-CS states road snapping only | Narrowed to the four snap targets for roads |
| Conv. 6 "Confidence: medium" | Each claim has one game | Lowered to low |
| Conv. 8 "and label which a tool makes" | No finding states labelling | Deleted |
| Conv. 8 "clearly specified" | Not a finding | Replaced with the finding's own "Strong on taxonomy" |
| Conv. 9 "worth providing" for "never"/tie-break | Evaluative; only two games state it | Narrowed to "exist in two games"; confidence split |
| Conv. 9 ONI "proximity option" as a tie-break | Old 6-ONI row did not say what the option does | Page re-read; 6-ONI now quotes the tie-break (hidden fixed priority, or closest with Enable Proximity) |
| Conv. 10 "Alerts are a counted list with a log", medium, four games | Counts and log are in 8-TB only | Split: on-screen list (four games, medium); counts/cycling/log (Timberborn only, low) |
| Conv. 11 Anno minimap/lock | Old §7 cited Anno with no link | §7 now has finding 7-AN with the Ludo link (page re-read; wording confirmed) |
| §4 Anno panel position | Old 4-AN line had no link | Both links added (page re-read) |
| §6 Banished "detail page not read" | Mention of an unopened page | Deleted |
| Limits: "PC Gamer's Anno 1800 article (truncated)" | Mention of a page not used for any finding | Deleted |

## Games named in conventions and their findings

Every game named in a convention has a finding with a link that describes the behaviour the convention gives it.

| Game | Conventions | Findings (with link) |
|---|---|---|
| Timberborn | 1, 2, 3, 5, 7, 8, 10, 11, 12 | 1-TB, 2-TB, 3-TB, 4-TB, 5-TB, 6-TB, 7-TB, 8-TB |
| RimWorld | 1, 2, 4, 7, 8, 9, 10, 11, 12 | 1-RW, 2-RW, 3-RW, 4-RW, 5-RW, 6-RW, 7-RW, 8-RW |
| Anno 1800 | 1, 2, 7, 11, 12 | 1-AN, 2-AN, 4-AN, 7-AN, 8-AN |
| Dwarf Fortress | 2, 4, 8, 10 | 2-DF, 3-DF, 5-DF, 6-DF |
| Cities: Skylines | 1, 2, 5, 6, 8 | 1-CS, 2-CS, 3-CS, 5-CS |
| Factorio | 4, 11, 12 | 3-FA, 7-FA, 8-FA |
| Banished | 2 | 2-BA |
| Oxygen Not Included | 10 | 6-ONI |

Eight games remain covered (the criterion needs at least six).
