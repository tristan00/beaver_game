//! Keyboard, pointer, selection and camera interaction.
use super::*;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::window::PrimaryWindow;

/// Whether the pointer is on a panel (which keeps every click from the map), or only on a pin
/// over the map (which takes left clicks but lets right-click orders through).
pub(super) fn pointer_over_ui(q: Query<(&Interaction, Has<ui::MapPin>), With<Blocks>>, mut over: ResMut<PointerOverUi>, mut pin: ResMut<PointerOverPin>) {
    let panel = q.iter().any(|(i, p)| *i != Interaction::None && !p);
    let on_pin = q.iter().any(|(i, p)| *i != Interaction::None && p);
    if over.0 != panel {
        over.0 = panel;
    }
    if pin.0 != on_pin {
        pin.0 = on_pin;
    }
}

pub(super) fn track_cursor(windows: Query<&Window, With<PrimaryWindow>>, scale: Res<UiScale>, mut c: ResMut<CursorUi>) {
    let p = windows.single().ok().and_then(|w| w.cursor_position()).map(|p| p / scale.0.max(0.01));
    if c.0 != p {
        c.0 = p;
    }
}

pub(super) fn hover_cell(point: Res<world::HoverPoint>, mut hover: ResMut<HoverCell>) {
    let cell = point.0.map(|p| p.0);
    if hover.0 != cell {
        hover.0 = cell;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    typing: Res<TextCapture>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut lens: ResMut<MapLens>,
    mut flow: ResMut<Flow>,
    menu: Res<MenuOpen>,
) {
    // A colony just settled opens on its welcome card, once.
    if flow.welcome {
        flow.welcome = false;
        hud.welcome = true;
    }
    if typing.0 || menu.0 {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for (k, l) in [(KeyCode::F1, MapLens::Water), (KeyCode::F2, MapLens::Forest), (KeyCode::F3, MapLens::Elevation), (KeyCode::F4, MapLens::Deposits), (KeyCode::F5, MapLens::Paths)] {
        if keys.just_pressed(k) {
            *lens = if *lens == l { MapLens::World } else { l };
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let open = hud.colony.is_some() && hud.page().is_none();
        hud.open_book(if open { None } else { Some(ColonyTab::Jobs) }, None);
    }
    if keys.just_pressed(KeyCode::KeyL) && !ctrl {
        let open = hud.ledger.is_some();
        hud.open_book(None, if open { None } else { Some(LedgerTab::Charts) });
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        hud.journal_open = !hud.journal_open;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        next_idle(&sim.0, &mut hud, &mut sel, &mut flow);
    }
    if !ctrl {
        // Each building's key from its row, then the harvest tools.
        let mut tools: Vec<(KeyCode, Tool)> = BuildingKind::all().filter_map(|k| Some((letter(k.row().hotkey.as_deref()?)?, Tool::of(k)))).collect();
        tools.extend([(KeyCode::KeyG, Tool::Harvest(Harvest::Gather)), (KeyCode::KeyT, Tool::Harvest(Harvest::Cut))]);
        for (k, t) in tools {
            let locked = match t {
                Tool::Dam => sim.0.unlocked(BuildingKind::dam()).is_err(),
                Tool::Canal => sim.0.unlocked(BuildingKind::canal()).is_err(),
                Tool::Building(b) => sim.0.unlocked(b).is_err(),
                Tool::Harvest(_) | Tool::None => false,
            };
            if keys.just_pressed(k) && !locked {
                let next = if hud.tool == t { Tool::None } else { t };
                hud.set_tool(next);
                *sel = Selection::None;
            }
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        // Esc always lets go of the camera: a glide to a notice ends where it is.
        flow.focus = None;
        hud.dismiss_top(&mut sel, &mut lens);
    }
}

pub(super) fn next_idle(s: &sim::SimState, hud: &mut HudState, sel: &mut Selection, flow: &mut Flow) {
    let idle = sim::hud::idle_beavers(s);
    if idle.is_empty() {
        return;
    }
    let i = hud.idle_cursor % idle.len();
    hud.idle_cursor = i + 1;
    hud.select_beaver(s, sel, flow, idle[i]);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn world_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    _scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    hover: Res<HoverCell>,
    over: Res<PointerOverUi>,
    on_pin: Res<PointerOverPin>,
    thing: Res<world::HoverThing>,
    point: Res<world::HoverPoint>,
    cursor: Res<CursorUi>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    menu: Res<MenuOpen>,
    mut flow: ResMut<Flow>,
    snap: Res<world::WorksSnap>,
) {
    if menu.0 {
        return;
    }
    let now = time.elapsed_secs();
    if hud.consume_left_release {
        if mouse.just_released(MouseButton::Left) { hud.consume_left_release = false; return; }
        if mouse.pressed(MouseButton::Left) { return; }
    }
    if !sim.0.is_running() {
        if flow.phase == crate::flow::Phase::Scouting && !over.0 && mouse.just_pressed(MouseButton::Left) {
            if let Some(cell) = hover.0 {
                match crate::flow::can_settle(&sim.0, cell) {
                    Ok(()) => {
                        if let Err(e) = crate::flow::settle(&mut sim.0, &mut flow, cell) {
                            hud.refuse(sim_error(&e), now);
                        }
                    }
                    Err(e) => hud.refuse(settle_error(e), now),
                }
            }
        }
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        undo_last(&mut sim.0, &mut hud, now);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && matches!(hud.tool, Tool::Dam | Tool::Canal) {
        if let Some(id) = commit_line(&mut sim.0, &mut hud, now) {
            *sel = Selection::Site(id);
            hud.selected.clear();
        }
        return;
    }
    // A box dragged out on the map selects even when let go over a panel; a press that never
    // reached the map leaves no box behind.
    if hud.box_from.is_some() && !mouse.pressed(MouseButton::Left) && !mouse.just_released(MouseButton::Left) {
        hud.box_from = None;
    }
    if mouse.just_released(MouseButton::Left) && hud.tool == Tool::None && hud.grab.is_none() {
        if let (Some(a), Some(b)) = (hud.box_from, cursor.0) {
            if a.distance(b) > 8.0 {
                hud.box_from = None;
                hud.box_done = Some((a, b));
                return;
            }
        }
    }
    if over.0 {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) && hud.opening.is_some() {
        hud.opening = None;
        return;
    }
    // Placing a gate or lock: a click on a lit cell of the dam places it (shift: keep placing).
    if let Some((dam, o)) = hud.opening {
        if mouse.just_released(MouseButton::Left) {
            let on_dam = snap.0.or(hover.0).filter(|c| sim::dams::dam(&sim.0, dam).is_some_and(|d| d.line().any(|x| x.cell == *c)));
            match on_dam {
                Some(cell) => match super::works::opening_fits(&sim.0, dam, cell, o) {
                    Ok(()) => {
                        let c = match o {
                            super::Opening::Gate => sim::Command::AddDamGate { dam, cell },
                            super::Opening::Lock => sim::Command::UpgradeLock { building: dam, cell },
                        };
                        match sim.0.apply(c) {
                            Ok(_) => {
                                hud.gate_sel = Some((dam, cell));
                                if !shift {
                                    hud.opening = None;
                                }
                            }
                            Err(e) => hud.refuse(sim_error(&e.to_string()), now),
                        }
                    }
                    Err(e) => hud.refuse(e, now),
                },
                None => hud.opening = None,
            }
        }
        if mouse.pressed(MouseButton::Left) || mouse.just_released(MouseButton::Left) {
            return;
        }
    }
    if mouse.just_pressed(MouseButton::Right) {
        if matches!(hud.tool, Tool::Dam | Tool::Canal) && hud.corners.len() > usize::from(hud.extending.is_some()) {
            hud.corners.pop();
            hud.line = if hud.tool == Tool::Canal { sim::sandbox::canal_line(&hud.corners) } else { build_logic::polyline(&hud.corners) };
            return;
        }
        if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
            return;
        }
        let who: Vec<BeaverId> = if hud.selected.is_empty() {
            match *sel {
                Selection::Beaver(b) => vec![b],
                _ => Vec::new(),
            }
        } else {
            hud.selected.clone()
        };
        if !who.is_empty() {
            if let Some(t) = thing.1 {
                order(&mut sim.0, &mut hud, &who, t, shift, now);
            }
        }
        return;
    }
    // A pin takes its own left click.
    if on_pin.0 {
        return;
    }
    let Some(cell) = hover.0.or_else(|| match thing.0 {
        Some(world::Thing::Building(id)) => sim.0.buildings.building(id).map(|b|b.pos),
        Some(world::Thing::Beaver(id)) => sim.0.agents.get(id).map(|a|a.cell),
        _ => None,
    }) else { return };
    match hud.tool {
        Tool::None => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.grab = dam_end_near(&sim.0, *sel, thing.0, cell);
                hud.box_from = cursor.0;
            }
            // Dragging away from a dam's end extends it from that end, and dragging from a canal
            // extends it from its open end; a plain click selects it.
            if let Some((id, end)) = hud.grab {
                let moved = matches!((hud.box_from, cursor.0), (Some(a), Some(b)) if a.distance(b) > 8.0);
                if mouse.pressed(MouseButton::Left) && moved {
                    hud.grab = None;
                    hud.box_from = None;
                    hud.set_tool(if sim::canals::canal(&sim.0, id).is_some() { Tool::Canal } else { Tool::Dam });
                    hud.extending = Some(id);
                    hud.corners = vec![end];
                    hud.drag_from = Some(end);
                    hud.selected.clear();
                    *sel = Selection::Site(id);
                    return;
                }
            }
            if mouse.just_released(MouseButton::Left) {
                hud.grab = None;
                let from = hud.box_from.take();
                if let (Some(a), Some(b)) = (from, cursor.0) {
                    if a.distance(b) > 8.0 {
                        hud.box_done = Some((a, b));
                        return;
                    }
                }
                let t = thing.0.unwrap_or(world::Thing::Cell(cell));
                // On the dam or canal already selected, a click picks the cell it is ringed
                // on (`WorksSnap`), or failing that the one under it (for a gate or lock, or to
                // raise or deepen just there); shift adds to the pick.
                let on_works = match (t, *sel) {
                    (_, Selection::Site(_)) if snap.0.is_some() => snap.0,
                    (world::Thing::Building(id), Selection::Site(sid)) if id == sid && on_line(&sim.0, id, cell) => Some(cell),
                    _ => None,
                };
                if let Some(cell) = on_works {
                    match hud.dam_cells.iter().position(|x| *x == cell) {
                        Some(i) => {
                            hud.dam_cells.remove(i);
                        }
                        None if shift => hud.dam_cells.push(cell),
                        None => hud.dam_cells = vec![cell],
                    }
                    return;
                }
                if t.selection() != *sel {
                    hud.dam_cells.clear();
                }
                match t {
                    world::Thing::Beaver(id) if shift => {
                        // Shift adds to the beaver already selected, or takes one away.
                        if let (true, Selection::Beaver(x)) = (hud.selected.is_empty(), *sel) {
                            hud.selected.push(x);
                        }
                        if let Some(i) = hud.selected.iter().position(|x| *x == id) {
                            hud.selected.remove(i);
                        } else {
                            hud.selected.push(id);
                        }
                        *sel = if hud.selected.contains(&id) { Selection::Beaver(id) } else { hud.selected.first().map_or(Selection::None, |f| Selection::Beaver(*f)) };
                    }
                    world::Thing::Beaver(id) => {
                        hud.selected = vec![id];
                        *sel = Selection::Beaver(id);
                    }
                    other => {
                        hud.selected.clear();
                        *sel = other.selection();
                    }
                }
                if hud.follow.is_some_and(|f| *sel != Selection::Beaver(f)) {
                    hud.follow = None;
                }
                let _ = point;
            }
        }
        Tool::Harvest(h) => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.drag_from = Some(cell);
            } else if mouse.just_released(MouseButton::Left) {
                if let Some(from) = hud.drag_from.take() {
                    harvest(&mut sim.0, &mut hud, h, from, cell, now);
                }
            }
        }
        Tool::Building(_) | Tool::Dam | Tool::Canal if mouse.just_pressed(MouseButton::Left) && !sim.0.fog.explored(cell) => {
            hud.refuse("Unexplored", now);
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::emit(&mut sim.0, &BuildOrder::Building { kind, pos: p.pos }) {
                    Ok(_) => {
                        // Shift keeps the building in hand for the next spot.
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            let id = b.id;
                            hud.undo.push(UndoItem::Site(id));
                            if !shift {
                                hud.frame_req = Some(cell_world(&sim.0.height, (b.pos.0 as f32, b.pos.1 as f32)));
                                hud.selected.clear();
                                *sel = Selection::Site(id);
                            }
                        }
                        if !shift {
                            hud.finish_placement();
                        }
                        hud.swallow_click();
                    }
                    Err(e) => {
                        let w = place_error(&e);
                        hud.say(Some((Icon::Cross, BAD)), Some((building_icon(kind), INK)), w, now);
                    }
                }
            }
        }
        Tool::Dam | Tool::Canal => {
            // A click adds a corner; a drag draws a straight run to where it ends.
            // Moving without a button held changes nothing, so the confirm tag stays put.
            if mouse.just_pressed(MouseButton::Left) {
                if hud.corners.is_empty() {
                    hud.corners.push(cell);
                }
                hud.drag_from = Some(cell);
            } else if mouse.just_released(MouseButton::Left) && hud.drag_from.take().is_some() && hud.corners.last() != Some(&cell) {
                hud.corners.push(cell);
            }
            let mut live = hud.corners.clone();
            if hud.drag_from.is_some() && live.last() != Some(&cell) {
                live.push(cell);
            }
            // A canal runs cell to cell across faces, never on a corner.
            let line = if hud.tool == Tool::Canal { sim::sandbox::canal_line(&live) } else { build_logic::polyline(&live) };
            if line != hud.line {
                hud.line = line;
            }
        }
    }
}

/// A right-click order for the selected beavers. On a tree, plant or deposit they work the one
/// clicked, then the nearest of its kind once it is used up. With `queue` (Shift held) the order
/// waits behind the ones they already have, and starts once those are done.
fn order(s: &mut sim::SimState, hud: &mut HudState, who: &[BeaverId], t: world::Thing, queue: bool, now: f32) {
    use sim::agents::{Assignment, Order};
    let give = |s: &mut sim::SimState, who: Vec<BeaverId>, to: Assignment| {
        if queue { s.apply(sim::Command::Queue { who, order: Order::Do(to) }) } else { s.apply(sim::Command::Assign { who, to: Some(to) }) }
    };
    match t {
        world::Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return };
            let workers: Vec<BeaverId> = who.iter().copied().filter(|id| s.colony.beaver(*id).is_some_and(|b| b.can_work())).collect();
            if workers.is_empty() {
                hud.refuse("Too young", now);
                return;
            }
            let kind = r.kind;
            match give(s, workers.clone(), Assignment::Resource(rid)) {
                Ok(_) => hud.say(Some((resource_icon(kind), GOOD)), None, format!("{}", workers.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Cell(c) => {
            match give(s, who.to_vec(), Assignment::Go(c)) {
                Ok(_) => hud.say(Some((Icon::Walk, INK)), None, format!("{}", who.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Building(b) => {
            let workers: Vec<BeaverId> = who.iter().copied().filter(|id| s.colony.beaver(*id).is_some_and(|bv| bv.can_work())).collect();
            if workers.is_empty() {
                hud.refuse("Too young", now);
                return;
            }
            if queue {
                match s.apply(sim::Command::Queue { who: workers.clone(), order: Order::Work(b) }) {
                    Ok(_) => hud.say(Some((Icon::Building, GOOD)), None, format!("{}", workers.len()), now),
                    Err(e) => hud.refuse(sim_error(&e.to_string()), now),
                }
                return;
            }
            let mut placed = 0;
            let mut last_err: Option<String> = None;
            for &w in &workers {
                let Some(slot) = s.work.place(b).and_then(|p| p.open_seat(|_| true)).map(|x| x.0) else { break };
                match s.apply(sim::Command::FillSlot { building: b, slot, who: w }) {
                    Ok(_) => placed += 1,
                    Err(e) => last_err = Some(sim_error(&e.to_string())),
                }
            }
            if placed == 0 {
                let why = last_err.unwrap_or_else(|| sim::jobs::workplace_error_reason(s, b));
                hud.refuse(why, now);
            }
        }
        _ => {}
    }
}

/// Every beaver inside the box dragged out on the map, kits too; with Shift held, added to
/// those already selected.
pub(super) fn box_select(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, scale: Res<UiScale>, keys: Res<ButtonInput<KeyCode>>, mut sel: ResMut<Selection>, drawn: Option<Res<crate::entities_render::BeaverDrawn>>) {
    let Some((a, b)) = hud.box_done.take() else { return };
    let Ok((cam, gt)) = cams.single() else { return };
    let (lo, hi) = (a.min(b), a.max(b));
    let s = &sim.0;
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let picked: Vec<BeaverId> = s
        .colony
        .beavers
        .iter()
        .filter(|bv| world::beaver_world(s, drawn.as_deref(), bv.id).and_then(|w| cam.world_to_viewport(gt, w).ok()).map(|v| v / scale.0.max(0.01)).is_some_and(|v| v.x >= lo.x && v.x <= hi.x && v.y >= lo.y && v.y <= hi.y))
        .map(|bv| bv.id)
        .collect();
    if shift {
        if let (true, Selection::Beaver(x)) = (hud.selected.is_empty(), *sel) {
            hud.selected.push(x);
        }
        for id in picked {
            if !hud.selected.contains(&id) {
                hud.selected.push(id);
            }
        }
    } else {
        hud.selected = picked;
    }
    let keep = matches!(*sel, Selection::Beaver(x) if hud.selected.contains(&x));
    if !keep {
        *sel = hud.selected.first().map_or(Selection::None, |f| Selection::Beaver(*f));
    }
    if hud.follow.is_some_and(|f| !hud.selected.contains(&f)) {
        hud.follow = None;
    }
}

pub(super) fn frame_camera(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, windows: Query<&Window, With<PrimaryWindow>>, orbit: Res<crate::camera::OrbitCamera>, mut flow: ResMut<Flow>) {
    let Some(p) = hud.frame_req.take() else { return };
    let (Ok((cam, gt)), Ok(w)) = (cams.single(), windows.single()) else { return };
    if let Some(t) = world::frame_target(cam, gt, Vec2::new(w.width(), w.height()), &sim.0, orbit.target, p) {
        flow.focus = Some(t);
    }
}

pub(super) fn follow_beaver(hud: Res<HudState>, sim: Res<Sim>, mut flow: ResMut<Flow>, drawn: Option<Res<crate::entities_render::BeaverDrawn>>) {
    if let Some(w) = hud.follow.and_then(|id| world::beaver_world(&sim.0, drawn.as_deref(), id)) {
        flow.focus = Some(w);
    }
}

/// The end of the selected dam under the pointer: grabbing it starts an extension.
/// The dam end at or next to `cell`, on the dam under the pointer or else the selected one.
/// Whether `cell` is on the line of dam or canal `id` (a cell the panels let you pick).
fn on_line(s: &sim::SimState, id: BuildingId, cell: Cell) -> bool {
    sim::dams::dam(s, id).is_some_and(|d| d.line().any(|c| c.cell == cell)) || sim::canals::canal(s, id).is_some_and(|k| k.cells.iter().any(|c| c.cell == cell))
}

/// The dam end, or the canal's open end, that a press at `cell` grabs: a dam by either end, a
/// canal anywhere along it (it can only grow from its open end).
fn dam_end_near(s: &sim::SimState, sel: Selection, thing: Option<world::Thing>, cell: Cell) -> Option<(BuildingId, Cell)> {
    let under = match thing {
        Some(world::Thing::Building(id)) => Some(id),
        _ => None,
    };
    let selected = match sel {
        Selection::Site(id) => Some(id),
        _ => None,
    };
    [under, selected].into_iter().flatten().find_map(|id| {
        if let Some(k) = sim::canals::canal(s, id) {
            let near = k.cells.iter().any(|c| c.cell.0.abs_diff(cell.0).max(c.cell.1.abs_diff(cell.1)) <= 1);
            return near.then(|| (id, k.end()));
        }
        let d = sim::dams::dam(s, id)?;
        let line: Vec<Cell> = d.line().map(|c| c.cell).collect();
        [line.first(), line.last()].into_iter().flatten().copied()
            .filter(|e| e.0.abs_diff(cell.0).max(e.1.abs_diff(cell.1)) <= 1)
            .min_by_key(|e| e.0.abs_diff(cell.0) + e.1.abs_diff(cell.1))
            .map(|e| (id, e))
    })
}

/// Send beavers to gather or cut: one spot gathers around it, a drag covers the area.
fn harvest(s: &mut sim::SimState, hud: &mut HudState, h: Harvest, a: Cell, b: Cell, now: f32) {
    use sim::resources::ResourceKind as R;
    let who = panels::order_crew(s, hud);
    if who.is_empty() {
        hud.say(Some((Icon::Beaver, BAD)), Some((Icon::Cross, BAD)), "0", now);
        return;
    }
    let max = s.world.size - 1;
    let area = if a == b {
        let r = 6;
        sim::work::Area { min: (a.0.saturating_sub(r), a.1.saturating_sub(r)), max: ((a.0 + r).min(max), (a.1 + r).min(max)) }
    } else {
        sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) }
    };
    // Cutting takes trees; gathering what grows or lies on land and needs no tools.
    let wanted: Vec<R> = R::all().filter(|k| match h {
        Harvest::Cut => k.is_tree(),
        Harvest::Gather => !k.is_tree() && !k.is_felled() && !k.aquatic() && k.row().needs.is_none(),
    }).collect();
    let here = |k: R| s.resources.items().iter().any(|r| r.kind == k && area.contains(r.cell) && (r.amount > 0.0 || k.flowing()));
    let at_click = s.resources.at(a).find(|r| wanted.contains(&r.kind)).map(|r| r.kind);
    let Some(kind) = at_click.or_else(|| wanted.iter().copied().find(|k| here(*k))) else {
        hud.say(Some((if h == Harvest::Cut { crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()) } else { crate::hud::words::resource_icon(sim::resources::ResourceKind::food_plant(false)) }, BAD)), Some((Icon::Cross, BAD)), "", now);
        return;
    };
    match s.apply(sim::Command::Assign { who: who.clone(), to: Some(sim::agents::Assignment::Area(kind, area)) }) {
        Ok(_) => {
            hud.say(Some((resource_icon(kind), GOOD)), Some((Icon::Beaver, ui::FUR)), format!("{}", who.len()), now);
            hud.set_tool(Tool::None);
        }
        Err(e) => hud.refuse(sim_error(&e.to_string()), now),
    }
}

/// The key for a letter a content row names.
fn letter(s: &str) -> Option<KeyCode> {
    use KeyCode::*;
    const KEYS: [KeyCode; 26] = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
    let c = s.chars().next()?.to_ascii_uppercase();
    c.is_ascii_uppercase().then(|| KEYS[(c as u8 - b'A') as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::agents::{Assignment, Task};

    struct DryLand;
    impl sim::colony::Landscape for DryLand {
        fn is_water(&self, _: f32, _: f32) -> bool {
            false
        }
    }

    /// A colony with a working beaver, a plant right beside it and one a little further off.
    fn colony_with_two_plants() -> (sim::SimState, BeaverId, sim::resources::ResourceId, sim::resources::ResourceId) {
        let mut s = sim::SimState::new(1);
        let c = (s.world.size / 2, s.world.size / 2);
        s.colony.found((c.0 as f32, c.1 as f32), &DryLand).expect("found");
        s.step(sim::STEP_DAYS).expect("step");
        let id = s.colony.beavers.iter().find(|b| b.can_work()).expect("a working beaver").id;
        let at = s.agents.get(id).expect("agent").cell;
        let kind = sim::resources::ResourceKind::food_plant(false);
        let near = s.resources.add(kind, (at.0 + 1, at.1), 10.0);
        let far = s.resources.add(kind, (at.0, at.1 + 6), 10.0);
        (s, id, near, far)
    }

    #[test]
    fn right_clicking_a_plant_sends_the_beaver_to_that_plant() {
        let (mut s, id, near, far) = colony_with_two_plants();
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(far), false, 0.0);
        assert_eq!(s.agents.get(id).expect("agent").assignment, Some(Assignment::Resource(far)));
        for _ in 0..5 {
            s.step(sim::STEP_DAYS).expect("step");
            if let Task::Collect { target, .. } = s.agents.get(id).expect("agent").task {
                assert_eq!(target, far, "went to the plant beside it ({near:?}) instead of the one clicked");
                return;
            }
        }
        panic!("the beaver never set out for the plant: {:?}", s.agents.get(id).expect("agent").task);
    }

    fn walk_until(s: &mut sim::SimState, id: BeaverId, done: impl Fn(&sim::agents::Agent) -> bool) -> bool {
        for _ in 0..400 {
            s.step(sim::STEP_DAYS).expect("step");
            if done(s.agents.get(id).expect("agent")) {
                return true;
            }
        }
        false
    }

    #[test]
    fn shift_right_click_queues_a_walk_after_the_current_one() {
        let (mut s, id, _, _) = colony_with_two_plants();
        let at = s.agents.get(id).expect("agent").cell;
        let (a, b) = ((at.0 + 3, at.1), (at.0 + 3, at.1 + 3));
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Cell(a), false, 0.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Cell(b), true, 0.0);
        let ag = s.agents.get(id).expect("agent");
        assert_eq!(ag.assignment, Some(Assignment::Go(a)), "the first order runs now");
        assert_eq!(ag.queue.len(), 1, "the second waits");
        assert!(walk_until(&mut s, id, |ag| ag.assignment == Some(Assignment::Go(b))), "never set out for the queued spot");
        assert!(walk_until(&mut s, id, |ag| ag.cell == b), "never reached the queued spot");
        assert!(s.agents.get(id).expect("agent").queue.is_empty());
    }

    #[test]
    fn a_queued_plant_is_gathered_once_the_walk_is_done() {
        let (mut s, id, _, far) = colony_with_two_plants();
        let at = s.agents.get(id).expect("agent").cell;
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Cell((at.0 + 3, at.1)), false, 0.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(far), true, 0.0);
        assert!(walk_until(&mut s, id, |ag| ag.assignment == Some(Assignment::Resource(far))), "never took up the queued plant");
        assert!(walk_until(&mut s, id, |ag| matches!(ag.task, Task::Collect { target, .. } if target == far)), "never gathered the queued plant");
    }

    #[test]
    fn shift_right_click_on_a_beaver_without_orders_starts_at_once() {
        let (mut s, id, _, far) = colony_with_two_plants();
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(far), true, 0.0);
        let ag = s.agents.get(id).expect("agent");
        assert_eq!(ag.assignment, Some(Assignment::Resource(far)));
        assert!(ag.queue.is_empty());
    }

    #[test]
    fn a_plain_right_click_drops_the_queue() {
        let (mut s, id, near, far) = colony_with_two_plants();
        let at = s.agents.get(id).expect("agent").cell;
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Cell((at.0 + 3, at.1)), false, 0.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(far), true, 0.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(near), false, 0.0);
        let ag = s.agents.get(id).expect("agent");
        assert_eq!(ag.assignment, Some(Assignment::Resource(near)));
        assert!(ag.queue.is_empty());
    }

    #[test]
    fn queued_orders_survive_a_script_round_trip() {
        for order in [sim::agents::Order::Do(Assignment::Go((3, 4))), sim::agents::Order::Do(Assignment::Resource(sim::resources::ResourceId(7))), sim::agents::Order::Work(sim::buildings::BuildingId(2))] {
            let cmd = sim::Command::Queue { who: vec![BeaverId(1), BeaverId(2)], order };
            assert_eq!(sim::script::parse_command(&sim::script::to_line(&cmd)), Ok(cmd));
        }
    }


    #[test]
    fn a_queued_walk_starts_once_the_plant_is_used_up() {
        // The only plant left, nearly bare, so the order ends with it.
        let (mut s, id, _, _) = colony_with_two_plants();
        s.resources.clear();
        let at = s.agents.get(id).expect("agent").cell;
        let far = s.resources.add(sim::resources::ResourceKind::food_plant(false), (at.0, at.1 + 2), 0.2);
        let then = (at.0 + 3, at.1);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(far), false, 0.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Cell(then), true, 0.0);
        assert!(walk_until(&mut s, id, |ag| ag.assignment == Some(Assignment::Go(then))), "never moved on to the queued walk: {:?}", s.agents.get(id).map(|a| (&a.assignment, &a.task)));
    }


    #[test]
    fn an_ordered_plant_with_little_left_is_still_gathered() {
        // Too little on it for a beaver to set out for by itself, but this one was ordered.
        let (mut s, id, _, _) = colony_with_two_plants();
        s.resources.clear();
        let at = s.agents.get(id).expect("agent").cell;
        let plant = s.resources.add(sim::resources::ResourceKind::food_plant(false), (at.0, at.1 + 2), 1.0);
        order(&mut s, &mut HudState::default(), &[id], world::Thing::Resource(plant), false, 0.0);
        assert!(walk_until(&mut s, id, |ag| matches!(ag.task, Task::Collect { target, .. } if target == plant)), "stood idle beside the plant it was sent to");
    }


    /// A gatherer sent to an area, with no plant of its own to go back to.
    fn area_gatherer_beside(kind: sim::resources::ResourceKind, amount: f32) -> (sim::SimState, BeaverId, sim::resources::ResourceId) {
        let (mut s, id, _, _) = colony_with_two_plants();
        s.resources.clear();
        let at = s.agents.get(id).expect("agent").cell;
        let r = s.resources.add(kind, (at.0, at.1 + 2), amount);
        let area = sim::work::Area { min: (at.0.saturating_sub(6), at.1.saturating_sub(6)), max: (at.0 + 6, at.1 + 6) };
        s.apply(sim::Command::Assign { who: vec![id], to: Some(Assignment::Area(kind, area)) }).expect("assign");
        assert_eq!(s.agents.get(id).expect("agent").source, None);
        (s, id, r)
    }

    /// No minimum keeps a part-eaten plant from being finished: a gatherer that did not start
    /// it, or forgot it (eating, resting), still takes what is left.
    #[test]
    fn a_part_eaten_plant_is_finished_by_any_forager() {
        let (mut s, id, plant) = area_gatherer_beside(sim::resources::ResourceKind::food_plant(false), 0.3);
        assert!(walk_until(&mut s, id, |ag| matches!(ag.task, Task::Collect { target, .. } if target == plant)), "left a part-eaten plant unfinished");
        for _ in 0..400 {
            if s.resources.get(plant).is_none() {
                return;
            }
            s.step(sim::STEP_DAYS).expect("step");
        }
        panic!("the plant was not picked to nothing: {:?}", s.resources.get(plant).map(|r| r.amount));
    }

    /// No minimum leaves the last of a mud bed in the river.
    #[test]
    fn the_last_of_a_river_mud_bed_is_dug() {
        let mud = sim::resources::ResourceKind::all().find(|k| k.flowing()).expect("a flowing deposit");
        let (mut s, id, bed) = area_gatherer_beside(mud, 0.2);
        assert!(walk_until(&mut s, id, |ag| matches!(ag.task, Task::Collect { target, .. } if target == bed)), "left the last of the mud bed");
    }

}
