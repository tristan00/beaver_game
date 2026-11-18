# Trace: pattern summaries and conventions to findings

Report: `city-builder-interfaces.md`. "Finding" = section and game line in that report (each carries its source link). Status: **kept** = supported as written; **narrowed** = reworded to what the finding states; **deleted** = removed because no finding states it.

## Pattern summaries

| Sentence (current wording, abridged) | Supporting finding | Status |
|---|---|---|
| Timberborn and Anno 1800 (Ludo) show resource/finance figures with time | §1 Timberborn row (inventory, time); §1 Anno row (balance, time) | narrowed (was "persistent strip with stockpile totals plus time/speed", which RimWorld contradicts) |
| RimWorld resource list top-left, time controls bottom-right | §1 RimWorld row | kept |
| Timberborn green satisfied needs; Cities: Skylines coloured demand bars | §1 Timberborn row; §1 Cities: Skylines row | kept |
| Timberborn, RimWorld, Banished, Anno have categorised build menus, with positions | §2 each game bullet | narrowed (was "at a screen edge") |
| Anno opens with B; DF menus open by letter key | §2 Anno bullet; §2 DF bullet | narrowed (was "opened by hotkey" for all) |
| Only Timberborn documented with keyboard cycling | §2 Timberborn bullet | narrowed |
| Timberborn preview green valid / red blocked (single source) | §3 Timberborn bullet | narrowed (was generic "binary colour") |
| Factorio ghosts, DF blueprint, RimWorld Plan mark work without immediate action | §3 Factorio, DF, RimWorld bullets | kept |
| Three models: per-worker settings; priorities on jobs/buildings; profession assignment | §6 table: RimWorld, DF; Timberborn, ONI, DF; Banished | narrowed ("matrix" and "per-building" for ONI dropped) |
| Tie-breaks: RimWorld left column, ONI proximity; Timberborn not established; DF not stated | §6 RimWorld, ONI, Timberborn, DF rows | narrowed |
| "...placement order" as a tie-break rule | none | deleted |
| "Ties are broken by a documented rule" for all games | none (DF, Timberborn lack one) | deleted |

## Conventions

| # | Sentence | Supporting finding | Status |
|---|---|---|---|
| 1 | Show stockpile/finance with time; colour for need/demand state | §1 Anno, Timberborn, RimWorld, Cities: Skylines rows | narrowed (dropped "top strip", "unmet needs") |
| 2 | Categorised build menu | §2 Timberborn, RimWorld, Anno, Banished, Cities: Skylines bullets | narrowed (dropped "screen edge", "one hotkey") |
| 2 | Keyboard cycling only Timberborn; CS hotkeys unsourced | §2 Timberborn, Cities: Skylines bullets | kept |
| 3 | Green valid / red blocked preview | §3 Timberborn bullet | narrowed (Timberborn only) |
| 3 | Mark work without committing | §3 Factorio, DF, RimWorld bullets | kept |
| 4 | Right-click cancels placement | §3 Cities: Skylines bullet | kept |
| 4 | Duplicate places multiple copies | §3 Timberborn bullet | kept |
| 4 | Factorio Shift ghosts as repeat placement | §3 Factorio bullet says ghosts only | deleted |
| 5 | Snap to grid and structures | §3 Cities: Skylines bullet | kept |
| 6 | Panel with settings incl. worker priority (Timberborn) | §4 Timberborn bullet | kept |
| 6 | Multi-select shows counts (RimWorld) | §4 RimWorld bullet | kept |
| 6 | Anno info panel as example of settings panel | §4 Anno bullet gives location only | deleted from examples; kept as location disagreement |
| 7 | DF rectangle drag, x removes | §5 DF bullet | kept |
| 7 | CS marquee/fill/brush | §5 Cities: Skylines bullet | kept |
| 7 | CS "de-zone" tool | none | deleted |
| 7 | RimWorld clear tool per area, shrink zone | §5 RimWorld bullet (drag behaviour is a Gap) | narrowed (not claimed as drag) |
| 7 | Timberborn mark/unmark areas | §5 Timberborn bullet | kept |
| 7 | Confidence "medium-high" | only two games document drag | narrowed to low-medium |
| 8 | Exclusive zones vs overlapping areas | §5 RimWorld bullet | kept |
| 9 | Ranges RimWorld 1–4, DF 1–7, ONI, Timberborn 5 levels | §6 rows | kept |
| 9 | "Never": RimWorld blank, ONI disabled | §6 RimWorld, ONI rows | kept |
| 9 | Tie-break RimWorld, ONI | §6 RimWorld, ONI rows | kept |
| 9 | "Never"/tie-break in DF and Timberborn | §6 DF, Timberborn rows: not stated / not established | deleted; confidence high → medium |
| 10 | Timberborn counts, cycling, Journal | §8 Timberborn bullet | kept |
| 10 | Factorio timed alerts, toggles; RimWorld alerts top-right; Anno notification list | §8 Factorio, RimWorld, Anno bullets | kept |
| 10 | Selecting alert centres camera; RimWorld colonist-bar jump as alert | §8 says "cycles through targets"; colonist jump is in §7 | deleted |
| 11 | Focus selected: Timberborn; minimap/lock: Anno; map toggle: Factorio | §7 | kept |
| 11 | "when the world is large" | none | deleted |

Games named in conventions and their finding sections: Timberborn (§1-8), RimWorld (§1-8), Dwarf Fortress (§2, 3, 5, 6), Banished (§1, 2, 6), Anno 1800 (§1, 2, 4, 7, 8), Cities: Skylines (§1, 2, 3, 5), Oxygen Not Included (§6), Factorio (§3, 7, 8). All eight remain.
