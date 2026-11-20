//! Keyboard, pointer, selection and camera interaction.
use super::*;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
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
    for (k, l) in [(KeyCode::F1, MapLens::World), (KeyCode::F2, MapLens::Water), (KeyCode::F3, MapLens::Goods), (KeyCode::F4, MapLens::Forest), (KeyCode::F5, MapLens::Elevation)] {
        if keys.just_pressed(k) {
            *lens = l;
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
    if keys.just_pressed(KeyCode::KeyP) {
        hud.profile_collapsed = !hud.profile_collapsed;
    }
    if keys.just_pressed(KeyCode::Slash) {
        hud.search = Some(String::new());
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if hud.area_edit.is_some() {
            hud.area_edit = None;
            hud.area_from = None;
        } else if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
        } else if hud.colony.is_some() {
            hud.colony = None;
        } else if hud.open_cat.is_some() {
            hud.open_cat = None;
        } else if *sel != Selection::None || !hud.selected.is_empty() {
            *sel = Selection::None;
            hud.selected.clear();
            hud.follow = None;
        }
    }
    let n = panels::categories(&sim.0).len();
    for (i, k) in CAT_KEYS.iter().enumerate().take(n) {
        if !ctrl && keys.just_pressed(*k) {
            hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) };
            hud.search = None;
        }
    }
    if matches!(hud.tool, Tool::Dam) {
        for (k, d) in [(KeyCode::BracketRight, 1.0), (KeyCode::BracketLeft, -1.0)] {
            if keys.just_pressed(k) {
                hud.step_height(d);
            }
        }
        if keys.just_pressed(KeyCode::KeyG) {
            extend_dam(&sim.0, &mut hud);
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
    scroll: Res<AccumulatedMouseScroll>,
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
    if matches!(hud.tool, Tool::Dam) && keys.pressed(KeyCode::ControlLeft) && scroll.delta.y != 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        hud.step_height(lines);
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        undo_last(&mut sim.0, &mut hud, now);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && matches!(hud.tool, Tool::Dam | Tool::Canal) && hud.search.is_none() {
        commit_line(&mut sim.0, &mut hud, now);
        return;
    }
    if over.0 {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        if hud.tool != Tool::None || hud.area_edit.is_some() {
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
    let Some(cell) = hover.0 else { return };
    if let Some(b) = hud.area_edit {
        if mouse.just_pressed(MouseButton::Left) {
            hud.area_from = Some(cell);
        } else if mouse.just_released(MouseButton::Left) {
            if let Some(a) = hud.area_from.take() {
                let area = drawn_area(&sim.0, b, a, cell);
                if let Err(e) = sim.0.apply(sim::Command::SetWorkArea { building: b, area }) {
                    hud.refuse(sim_error(&e.to_string()), now);
                }
                hud.area_edit = None;
            }
        }
        return;
    }
    match hud.tool {
        Tool::None => {
            if mouse.just_pressed(MouseButton::Left) {
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
        Tool::Building(_) | Tool::Dam | Tool::Canal if mouse.just_pressed(MouseButton::Left) && !sim.0.fog.explored(cell) => {
            hud.refuse("Unexplored", now);
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::emit(&mut sim.0, &BuildOrder::Building { kind, pos: p.pos }) {
                    Ok(_) => {
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            hud.undo.push(UndoItem::Site(b.id));
                            hud.frame_req = Some(cell_world(&sim.0.height, (b.pos.0 as f32, b.pos.1 as f32)));
                        }
                    }
                    Err(e) => {
                        let w = place_error(&e);
                        hud.say(Some((Icon::Cross, BAD)), Some((building_icon(kind), INK)), w, now);
                    }
                }
            }
        }
        Tool::Dam | Tool::Canal => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.drag_from = Some(cell);
                hud.line = vec![cell];
                hud.dam_crest = None;
            } else if mouse.pressed(MouseButton::Left) {
                if let Some(a) = hud.drag_from {
                    let line = if hud.tool == Tool::Canal { sim::sandbox::canal_line(&[a, cell]) } else { build_logic::line_cells(a, cell) };
                    if line != hud.line {
                        hud.line = line;
                    }
                }
            } else if mouse.just_released(MouseButton::Left) {
                hud.drag_from = None;
                if !hud.line.is_empty() {
                    let n = hud.line.len() as f32;
                    let mid = (hud.line.iter().map(|c| c.0 as f32).sum::<f32>() / n, hud.line.iter().map(|c| c.1 as f32).sum::<f32>() / n);
                    hud.frame_req = Some(cell_world(&sim.0.height, mid));
                }
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

pub(super) fn search_input(mut keys: MessageReader<KeyboardInput>, sim: Res<Sim>, mut hud: ResMut<HudState>, mut typing: ResMut<TextCapture>) {
    let open = hud.search.is_some();
    if typing.0 != open {
        typing.0 = open;
    }
    if !open {
        keys.clear();
        return;
    }
    for ev in keys.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Escape => hud.search = None,
            Key::Backspace => {
                if let Some(q) = hud.search.as_mut() {
                    q.pop();
                }
            }
            Key::Enter => {
                let q = hud.search.clone().unwrap_or_default();
                if let Some(e) = search_matches(&sim.0, &q).into_iter().find(|e| e.unlocked) {
                    let t = match e.kind {
                        BuildingKind::Dam => Tool::Dam,
                        BuildingKind::Canal => Tool::Canal,
                        k => Tool::Building(k),
                    };
                    hud.set_tool(t);
                }
                hud.search = None;
            }
            _ => {
                if let Some(t) = &ev.text {
                    let add: String = t.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
                    if !add.is_empty() {
                        if let Some(q) = hud.search.as_mut() {
                            q.push_str(&add);
                        }
                    }
                }
            }
        }
    }
    typing.0 = hud.search.is_some();
}
