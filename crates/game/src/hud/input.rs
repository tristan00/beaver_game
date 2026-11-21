//! Keyboard, pointer, selection and camera interaction.
use super::*;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::window::PrimaryWindow;

pub(super) fn pointer_over_ui(q: Query<&Interaction, With<Blocks>>, mut over: ResMut<PointerOverUi>) {
    let v = q.iter().any(|i| *i != Interaction::None);
    if over.0 != v {
        over.0 = v;
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
    if typing.0 || menu.0 {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for (k, l) in [(KeyCode::F1, MapLens::Water), (KeyCode::F2, MapLens::Forest), (KeyCode::F3, MapLens::Elevation)] {
        if keys.just_pressed(k) {
            *lens = if *lens == l { MapLens::World } else { l };
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        hud.colony = if hud.colony.is_some() { None } else { Some(ColonyTab::Population) };
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        hud.colony = if hud.colony == Some(ColonyTab::Jobs) { None } else { Some(ColonyTab::Jobs) };
    }
    if keys.just_pressed(KeyCode::KeyL) {
        hud.colony = if hud.colony == Some(ColonyTab::Goods) { None } else { Some(ColonyTab::Goods) };
    }
    if keys.just_pressed(KeyCode::KeyI) {
        next_idle(&sim.0, &mut hud, &mut sel, &mut flow);
    }
    if !ctrl {
        let tools = [
            (KeyCode::KeyZ, Tool::Dam),
            (KeyCode::KeyX, Tool::Building(BuildingKind::Shelter)),
            (KeyCode::KeyC, Tool::Building(BuildingKind::Warehouse)),
            (KeyCode::KeyG, Tool::Harvest(Harvest::Gather)),
            (KeyCode::KeyT, Tool::Harvest(Harvest::Cut)),
        ];
        for (k, t) in tools {
            if keys.just_pressed(k) {
                let allowed = match t {
                    Tool::Dam => sim.0.can_build(BuildingKind::Dam),
                    Tool::Building(kind) => sim.0.can_build(kind),
                    _ => true,
                };
                if allowed {
                    let next = if hud.tool == t { Tool::None } else { t };
                    hud.set_tool(next);
                    *sel = Selection::None;
                }
            }
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
        } else if hud.colony.is_some() {
            hud.colony = None;
        } else if *sel != Selection::None || !hud.selected.is_empty() {
            *sel = Selection::None;
            hud.selected.clear();
            hud.follow = None;
        }
    }
}

pub(super) fn next_idle(s: &sim::SimState, hud: &mut HudState, sel: &mut Selection, flow: &mut Flow) {
    let idle = sim::hud::idle_beavers(s);
    if idle.is_empty() {
        return;
    }
    let i = hud.idle_cursor % idle.len();
    hud.idle_cursor = i + 1;
    if let Some(b) = s.colony.beaver(idle[i]) {
        flow.focus = Some(cell_world(&s.height, b.pos));
        *sel = Selection::Beaver(b.id);
        hud.selected = vec![b.id];
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn world_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    _scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    hover: Res<HoverCell>,
    over: Res<PointerOverUi>,
    thing: Res<world::HoverThing>,
    point: Res<world::HoverPoint>,
    cursor: Res<CursorUi>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    menu: Res<MenuOpen>,
    mut flow: ResMut<Flow>,
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
    if keys.just_pressed(KeyCode::Enter) && hud.tool == Tool::Dam {
        if let Some(id) = commit_line(&mut sim.0, &mut hud, now) {
            *sel = Selection::Site(id);
            hud.selected.clear();
        }
        return;
    }
    if over.0 {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        if hud.tool == Tool::Dam && hud.corners.len() > usize::from(hud.extending.is_some()) {
            hud.corners.pop();
            hud.line = build_logic::polyline(&hud.corners);
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
            if let Some(t) = thing.0 {
                order(&mut sim.0, &mut hud, &who, t, shift, now);
            }
        }
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
                if let Some(end) = dam_end_at(&sim.0, *sel, cell) {
                    let Selection::Site(id) = *sel else { unreachable!() };
                    hud.set_tool(Tool::Dam);
                    hud.extending = Some(id);
                    hud.corners = vec![end];
                    hud.drag_from = Some(end);
                    return;
                }
                hud.box_from = cursor.0;
            }
            if mouse.just_released(MouseButton::Left) {
                let from = hud.box_from.take();
                if let (Some(a), Some(b)) = (from, cursor.0) {
                    if a.distance(b) > 8.0 {
                        hud.box_done = Some((a, b));
                        return;
                    }
                }
                let t = thing.0.unwrap_or(world::Thing::Cell(cell));
                match t {
                    world::Thing::Beaver(id) if shift => {
                        if let Some(i) = hud.selected.iter().position(|x| *x == id) {
                            hud.selected.remove(i);
                        } else {
                            hud.selected.push(id);
                        }
                        *sel = Selection::Beaver(id);
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
        Tool::Building(_) | Tool::Dam if mouse.just_pressed(MouseButton::Left) && !sim.0.fog.explored(cell) => {
            hud.refuse("Unexplored", now);
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::emit(&mut sim.0, &BuildOrder::Building { kind, pos: p.pos }) {
                    Ok(_) => {
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            let id = b.id;
                            hud.undo.push(UndoItem::Site(id));
                            hud.frame_req = Some(cell_world(&sim.0.height, (b.pos.0 as f32, b.pos.1 as f32)));
                            hud.selected.clear();
                            *sel = Selection::Site(id);
                        }
                        hud.finish_placement();
                        hud.swallow_click();
                    }
                    Err(e) => {
                        let w = place_error(&e);
                        hud.say(Some((Icon::Cross, BAD)), Some((building_icon(kind), INK)), w, now);
                    }
                }
            }
        }
        Tool::Dam => {
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
            let line = build_logic::polyline(&live);
            if line != hud.line {
                hud.line = line;
            }
        }
    }
}

fn order(s: &mut sim::SimState, hud: &mut HudState, who: &[BeaverId], t: world::Thing, one: bool, now: f32) {
    match t {
        world::Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return };
            let to = if one {
                sim::agents::Assignment::Resource(rid)
            } else {
                let rad = 6;
                let max = s.world.size - 1;
                let c = r.cell;
                let area = sim::work::Area { min: (c.0.saturating_sub(rad), c.1.saturating_sub(rad)), max: ((c.0 + rad).min(max), (c.1 + rad).min(max)) };
                sim::agents::Assignment::Area(r.kind, area)
            };
            let kind = r.kind;
            match s.apply(sim::Command::Assign { who: who.to_vec(), to: Some(to) }) {
                Ok(_) => hud.say(Some((resource_icon(kind), GOOD)), None, format!("{}", who.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Cell(c) => {
            match s.apply(sim::Command::Assign { who: who.to_vec(), to: Some(sim::agents::Assignment::Go(c)) }) {
                Ok(_) => hud.say(Some((Icon::Walk, INK)), None, format!("{}", who.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Building(b) => {
            let mut placed = 0;
            for &w in who {
                let Some(slot) = s.work.place(b).and_then(|p| p.slots.iter().position(|x| x.worker.is_none())) else { break };
                if s.apply(sim::Command::FillSlot { building: b, slot, who: w }).is_ok() {
                    placed += 1;
                }
            }
            if placed == 0 {
                hud.refuse("Full", now);
            }
        }
        _ => {}
    }
}

pub(super) fn box_select(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, scale: Res<UiScale>, mut sel: ResMut<Selection>) {
    let Some((a, b)) = hud.box_done.take() else { return };
    let Ok((cam, gt)) = cams.single() else { return };
    let (lo, hi) = (a.min(b), a.max(b));
    let s = &sim.0;
    let picked: Vec<BeaverId> = s
        .colony
        .beavers
        .iter()
        .filter(|bv| bv.stage != sim::colony::Stage::Kit)
        .filter(|bv| world::beaver_world(s, bv.id).and_then(|w| cam.world_to_viewport(gt, w).ok()).map(|v| v / scale.0.max(0.01)).is_some_and(|v| v.x >= lo.x && v.x <= hi.x && v.y >= lo.y && v.y <= hi.y))
        .map(|bv| bv.id)
        .collect();
    if let Some(first) = picked.first() {
        *sel = Selection::Beaver(*first);
    }
    hud.selected = picked;
}

pub(super) fn frame_camera(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, windows: Query<&Window, With<PrimaryWindow>>, orbit: Res<crate::camera::OrbitCamera>, mut flow: ResMut<Flow>) {
    let Some(p) = hud.frame_req.take() else { return };
    let (Ok((cam, gt)), Ok(w)) = (cams.single(), windows.single()) else { return };
    if let Some(t) = world::frame_target(cam, gt, Vec2::new(w.width(), w.height()), &sim.0, orbit.target, p) {
        flow.focus = Some(t);
    }
}

pub(super) fn follow_beaver(hud: Res<HudState>, sim: Res<Sim>, mut flow: ResMut<Flow>) {
    if let Some(w) = hud.follow.and_then(|id| world::beaver_world(&sim.0, id)) {
        flow.focus = Some(w);
    }
}


/// The end of the selected dam under the pointer: grabbing it starts an extension.
fn dam_end_at(s: &sim::SimState, sel: Selection, cell: Cell) -> Option<Cell> {
    let Selection::Site(id) = sel else { return None };
    let d = sim::dams::dam(s, id)?;
    let line: Vec<Cell> = d.line().map(|c| c.cell).collect();
    [line.first(), line.last()].into_iter().flatten().copied().find(|e| *e == cell)
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
    let wanted: &[R] = match h {
        Harvest::Cut => &[R::Tree],
        Harvest::Gather => &[R::Plant, R::MudBank, R::RiverClay],
    };
    let here = |k: R| s.resources.items.iter().any(|r| r.kind == k && area.contains(r.cell) && (r.amount > 0.0 || k == R::RiverClay));
    let at_click = s.resources.at(a).find(|r| wanted.contains(&r.kind)).map(|r| r.kind);
    let Some(kind) = at_click.or_else(|| wanted.iter().copied().find(|k| here(*k))) else {
        hud.say(Some((if h == Harvest::Cut { Icon::Tree } else { Icon::Plant }, BAD)), Some((Icon::Cross, BAD)), "", now);
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
