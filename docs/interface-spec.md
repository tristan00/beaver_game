# Beaver interface spec

**Source:** only `research/interface-ledger.md` (conventions C1–C12, quoted verbatim there). Nothing comes from the earlier interface or its specs.
**Reading order:** 1 (whole screen) → 2–6 (one block each) → 7 (what is left out).

**Trace rule:** each element row names a ledger convention whose quote states that behaviour. The frame elements the request implies (resource bar, build menu, selection panel, notifications, camera pan and zoom) may instead say *required by the request*. Confidence is the ledger's: low unless stated.
**Placement:** where a block sits on screen is a layout choice. The ledger says positions differ between games (C1) and that panel position is not a convention (C7). Placement is shown in the wireframes only; it carries no Trace.
Wireframe text is placeholder labels, not copy.

---

## 1. Full screen

```
┌────────────────────────────────────────────────────────────┐
│ [1] RESOURCE BAR                                           │
├──────────────────────────────────────────────┬─────────────┤
│                                              │ [4] ALERTS  │
│                                              │             │
│                 WORLD VIEW                   ├─────────────┤
│        (camera pans and zooms here)          │ [5] CAMERA  │
│                                              │  overview   │
│ ┌──────────────────────┐                     │  map        │
│ │ [3] SELECTION PANEL  │                     │             │
│ ├──────────────────────┤                     │             │
│ │ [2] BUILD MENU       │                     │             │
│ └──────────────────────┘                     │             │
└──────────────────────────────────────────────┴─────────────┘
```

| # | Block | Trace |
|---|---|---|
| 1 | Resource bar | C1; required by the request |
| 2 | Build menu | C2; required by the request |
| 3 | Selection panel | C7; required by the request |
| 4 | Notifications | C11; required by the request |
| 5 | Camera controls | C12; required by the request (pan, zoom) |

---

## 2. Resource bar

```
┌────────────────────────────────────────────────────────────┐
│ [Good A] ##   [Good B] ##   [Good C] ##   [Good D] ##      │
└────────────────────────────────────────────────────────────┘
```

| Element | Behaviour | Trace |
|---|---|---|
| Stockpile figures | Current amount of each good, always visible on the main screen | C1: "Keep stockpile or finance figures on the main screen." (medium) |
| Resource bar | The strip holding those figures | required by the request |

---

## 3. Build menu

```
 Tabs:  [ Cat 1 ] [ Cat 2 ] [ Cat 3 ] [ Cat 4 ]      key: opens menu
 Tools: [ item ] [ item ] [ item ] [ item ]

 Placement preview (over the world view):
   valid   ▓▓▓  green
   blocked ▓▓▓  red
```

| Element | Behaviour | Trace |
|---|---|---|
| Category tabs | Buildings are grouped into categories; one tab per category | C2: "Offer a categorised build menu." (medium) |
| Open-by-key | One key opens the menu | C2: Anno opens it with B; Dwarf Fortress menus open by letter key (low). The key itself is not set here |
| Placement preview | Ghost of the building follows the cursor: green when valid, red when blocked | C3: "Show a placement preview that is green when valid and red when blocked." (low, one game) |
| Build menu | The dock holding tabs and tools | required by the request |

---

## 4. Selection panel

```
┌──────────────────────────────┐
│ Name of selected building    │
│ Settings …                   │
│ Worker priority: ○ ○ ○ ○ ○   │
└──────────────────────────────┘

 Several of one type selected:
┌──────────────────────────────┐
│ Type name  ×N                │
└──────────────────────────────┘
```

| Element | Behaviour | Trace |
|---|---|---|
| Panel opens on selection | Selecting a building opens a panel with its settings | C7: "Selecting a building opens a panel with its settings, including its worker priority." (low) |
| Worker priority | Five levels, set per workplace | C7: Timberborn, workplace priority in 5 levels; C10: "Priority is a small set of levels." (medium) |
| Multi-select count | Selecting several objects of one type shows their count | C7: "Selecting several objects of one type shows their count." (low) |
| Selection panel | The panel itself | required by the request |

---

## 5. Notifications

```
┌─ ALERTS ──────────┐
│ ⚠ Alert type  ×2  │
│ ⚠ Alert type  ×1  │
│ ⚠ Alert type  ×1  │
└───────────────────┘
```

| Element | Behaviour | Trace |
|---|---|---|
| Alerts on the main screen | Alerts are visible during play | C11: "Show alerts on the main screen." (medium) |
| List layout | Alerts form a list | C11: only Timberborn's finding says the alerts form a list (low) |
| Count per alert | Each row shows how many items it covers | C11: counts per alert documented only for Timberborn (low) |
| Notifications block | The block itself | required by the request |

What selecting an alert does to the camera is not established (C11), so no click behaviour is specified.

---

## 6. Camera controls

```
 World view:   pan  ←↑↓→        zoom  ⊕ ⊖
 Selected object:  [ Focus ]
 Overview map:     ┌────────┐
                   │ ·  ▭   │
                   └────────┘
```

| Element | Behaviour | Trace |
|---|---|---|
| Pan | Move the view across the world | required by the request. Defaults and bindings are not sourced (C12) and are not set here |
| Zoom | Move the view closer or further | required by the request. No convention mentions zoom |
| Move to selection | An action moves the camera to the selected object | C12: "Provide a camera action that moves to a selected object." (medium) |
| Overview map | A map of the whole area | C12: "Provide an overview map." (low) |

---

## 7. Gaps (left out)

The ledger marks these as gaps. They are not designed here.

| Gap | Ledger reason |
|---|---|
| Settling-spot choice | No convention mentions choosing a start location |
| Dam flood preview | C3 covers a valid/blocked placement preview only; nothing on water, floods or dams |
| Drag gather | C8 says area orders are drag-selected but never names gathering |
| Job assignment | C7 and C10 give worker priority and level sets, not assigning workers to tasks |
| Pan and zoom specifics | Defaults and bindings are not sourced (C12); no convention mentions zoom. Only the existence of both is kept, from the request |
