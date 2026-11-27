use super::*;
use bevy::window::PrimaryWindow;

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

#[allow(clippy::too_many_arguments)]
pub(super) fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut lens: ResMut<MapLens>,
    mut flow: ResMut<Flow>,
    menu: Res<MenuOpen>,
) {
    if menu.0 {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for (k, l) in [(KeyCode::F1, MapLens::Water), (KeyCode::F2, MapLens::Forest), (KeyCode::F3, MapLens::Elevation), (KeyCode::F4, MapLens::Deposits), (KeyCode::F5, MapLens::Paths)] {
        if keys.just_pressed(k) {
            *lens = if *lens == l { MapLens::World } else { l };
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let open = hud.colony().is_some();
        hud.set_overlay(if open { None } else { Some(Overlay::Colony(ColonyTab::Jobs)) });
    }
    if keys.just_pressed(KeyCode::KeyL) && !ctrl {
        let open = hud.ledger().is_some();
        hud.set_overlay(if open { None } else { Some(Overlay::Ledger(LedgerTab::Charts)) });
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        hud.journal_open = !hud.journal_open;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        next_idle(&sim.0, &mut hud, &mut sel, &mut flow);
    }
    if !ctrl {
        let mut tools: Vec<(KeyCode, Tool)> = BuildingKind::all().filter_map(|k| Some((letter(k.row().hotkey.as_deref()?)?, Tool::of(k)))).collect();
        tools.extend([(KeyCode::KeyG, Tool::Harvest(Harvest::Gather)), (KeyCode::KeyT, Tool::Harvest(Harvest::Cut))]);
        for (k, t) in tools {
            let locked = match t {
                Tool::Dam => sim.0.unlocked(BuildingKind::dam()).is_err(),
                Tool::Canal => sim.0.unlocked(BuildingKind::canal()).is_err(),
                Tool::Building(b) => sim.0.unlocked(b).is_err(),
                Tool::Harvest(_) | Tool::Opening(_, _) | Tool::None => false,
            };
            if keys.just_pressed(k) && !locked {
                let next = if hud.tool == t { Tool::None } else { t };
                hud.set_tool(next);
                hud.select(&mut sel, Selection::None);
            }
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        flow.focus = None;
        hud.dismiss_top(&mut sel, &mut lens, &mut flow.welcome);
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
    hover: Res<Hover>,
    over: Res<PointerOverUi>,
    on_pin: Res<PointerOverPin>,
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
    if hud.consume_left_release {
        if mouse.just_released(MouseButton::Left) { hud.consume_left_release = false; return; }
        if mouse.pressed(MouseButton::Left) { return; }
    }
    if !sim.0.is_running() {
        if !over.0 && mouse.just_pressed(MouseButton::Left) {
            if let Some(cell) = hover.cell() {
                match crate::flow::can_settle(&sim.0, cell) {
                    Ok(()) => {
                        if let Err(e) = crate::flow::settle(&mut sim.0, &mut flow, cell) {
                            hud.refuse(sim_error(&e));
                        }
                    }
                    Err(e) => hud.refuse(settle_error(e)),
                }
            }
        }
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        undo_last(&mut sim.0, &mut hud, &mut sel);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && matches!(hud.tool, Tool::Dam | Tool::Canal) {
        if let Some(id) = commit_line(&mut sim.0, &mut hud) {
            hud.select(&mut sel, Selection::Site(id));
        }
        return;
    }
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
    if mouse.just_pressed(MouseButton::Right) {
        if matches!(hud.tool, Tool::Dam | Tool::Canal) && hud.corners.len() > usize::from(hud.extending.is_some()) {
            hud.corners.pop();
            hud.line = if hud.tool == Tool::Canal { sim::sandbox::canal_line(&hud.corners) } else { sim::sandbox::line(&hud.corners) };
            return;
        }
        if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
            return;
        }
        let who = hud.selected_beavers(*sel);
        if !who.is_empty() && hover.order != Selection::None {
            order(&mut sim.0, &mut hud, &who, hover.order, shift);
        }
        return;
    }
    if let Tool::Opening(dam, o) = hud.tool {
        if mouse.just_released(MouseButton::Left) {
            let on_dam = hover.works_cell.or(hover.cell()).filter(|c| sim::dams::dam(&sim.0, dam).is_some_and(|d| d.line().any(|x| x.cell == *c)));
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
                                    hud.set_tool(Tool::None);
                                }
                            }
                            Err(e) => hud.refuse(sim_error(&e.to_string())),
                        }
                    }
                    Err(e) => hud.refuse(e),
                },
                None => hud.set_tool(Tool::None),
            }
        }
        return;
    }
    if on_pin.0 {
        return;
    }
    let Some(cell) = hover.cell().or_else(|| match hover.thing {
        Selection::Site(id) => sim.0.buildings.building(id).map(|b| b.pos),
        Selection::Beaver(id) => sim.0.agents.get(id).map(|a| a.cell),
        _ => None,
    }) else { return };
    match hud.tool {
        Tool::None => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.grab = dam_end_near(&sim.0, *sel, hover.thing, cell);
                hud.box_from = cursor.0;
            }
            if let Some((id, end)) = hud.grab {
                let moved = matches!((hud.box_from, cursor.0), (Some(a), Some(b)) if a.distance(b) > 8.0);
                if mouse.pressed(MouseButton::Left) && moved {
                    hud.grab = None;
                    hud.box_from = None;
                    hud.set_tool(if sim::canals::canal(&sim.0, id).is_some() { Tool::Canal } else { Tool::Dam });
                    hud.extending = Some(id);
                    hud.corners = vec![end];
                    hud.drag_from = Some(end);
                    hud.select(&mut sel, Selection::Site(id));
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
                let t = if hover.thing != Selection::None { hover.thing } else { Selection::Cell(cell) };
                let on_works = match (t, *sel) {
                    (_, Selection::Site(_)) if hover.works_cell.is_some() => hover.works_cell,
                    (Selection::Site(id), Selection::Site(sid)) if id == sid && on_line(&sim.0, id, cell) => Some(cell),
                    _ => None,
                };
                if let Some(cell) = on_works {
                    match hud.dam_cells.iter().position(|x| *x == cell) {
                        Some(i) => { hud.dam_cells.remove(i); }
                        None if shift => hud.dam_cells.push(cell),
                        None => hud.dam_cells = vec![cell],
                    }
                    return;
                }
                match t {
                    Selection::Beaver(id) if shift => {
                        if let Some(i) = hud.selected.iter().position(|x| *x == id) {
                            hud.selected.remove(i);
                        } else {
                            hud.selected.push(id);
                        }
                        *sel = if hud.selected.contains(&id) { Selection::Beaver(id) } else { hud.selected.first().map_or(Selection::None, |f| Selection::Beaver(*f)) };
                        if hud.follow.is_some_and(|f| !hud.selected.contains(&f)) { hud.follow = None; }
                    }
                    other => hud.select(&mut sel, other),
                }
            }
        }
        Tool::Opening(_, _) => {}
        Tool::Harvest(h) => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.drag_from = Some(cell);
            } else if mouse.just_released(MouseButton::Left) {
                if let Some(from) = hud.drag_from.take() {
                    harvest(&mut sim.0, &mut hud, h, from, cell);
                }
            }
        }
        Tool::Building(_) | Tool::Dam | Tool::Canal if mouse.just_pressed(MouseButton::Left) && !sim.0.fog.explored(cell) => {
            hud.refuse("Unexplored");
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::place_building(&mut sim.0, kind, p.pos) {
                    Ok(()) => {
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            let id = b.id;
                            hud.undo.push(id);
                            if !shift {
                                hud.frame_req = Some(cell_world(&sim.0.height, b.centre()));
                                hud.select(&mut sel, Selection::Site(id));
                            }
                        }
                        if !shift {
                            hud.finish_placement();
                        }
                        hud.swallow_click();
                    }
                    Err(w) => hud.say(Some((Icon::Cross, BAD)), Some((building_icon(kind), INK)), w),
                }
            }
        }
        Tool::Dam | Tool::Canal => {
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
            let line = if hud.tool == Tool::Canal { sim::sandbox::canal_line(&live) } else { sim::sandbox::line(&live) };
            if line != hud.line {
                hud.line = line;
            }
        }
    }
}

fn order(s: &mut sim::SimState, hud: &mut HudState, who: &[BeaverId], t: Selection, queue: bool) {
    use sim::agents::Assignment;
    let give = |s: &mut sim::SimState, who: Vec<BeaverId>, to: Assignment| {
        s.apply(sim::Command::Assign { who, to: Some(to), queue })
    };
    match t {
        Selection::Tree(rid) => {
            let Some(r) = s.resources.get(rid) else { return };
            let workers: Vec<BeaverId> = who.iter().copied().filter(|id| s.colony.beaver(*id).is_some_and(|b| b.can_work())).collect();
            if workers.is_empty() {
                hud.refuse("Too young");
                return;
            }
            let kind = r.kind;
            match give(s, workers.clone(), Assignment::Resource(rid, kind)) {
                Ok(_) => hud.say(Some((resource_icon(kind), GOOD)), None, format!("{}", workers.len())),
                Err(e) => hud.refuse(sim_error(&e.to_string())),
            }
        }
        Selection::Cell(c) => {
            match give(s, who.to_vec(), Assignment::Go(c)) {
                Ok(_) => hud.say(Some((Icon::Walk, INK)), None, format!("{}", who.len())),
                Err(e) => hud.refuse(sim_error(&e.to_string())),
            }
        }
        Selection::Site(b) => {
            let workers: Vec<BeaverId> = who.iter().copied().filter(|id| s.colony.beaver(*id).is_some_and(|bv| bv.can_work())).collect();
            if workers.is_empty() {
                hud.refuse("Too young");
                return;
            }
            match give(s, workers.clone(), Assignment::Work(b)) {
                Ok(_) => hud.say(Some((Icon::Building, GOOD)), None, format!("{}", workers.len())),
                Err(e) => hud.refuse(sim_error(&e.to_string())),
            }
        }
        _ => {}
    }
}

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

fn on_line(s: &sim::SimState, id: BuildingId, cell: Cell) -> bool {
    sim::dams::dam(s, id).is_some_and(|d| d.line().any(|c| c.cell == cell)) || sim::canals::canal(s, id).is_some_and(|k| k.cells.iter().any(|c| c.cell == cell))
}

fn dam_end_near(s: &sim::SimState, sel: Selection, thing: Selection, cell: Cell) -> Option<(BuildingId, Cell)> {
    let under = match thing {
        Selection::Site(id) => Some(id),
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

fn harvest(s: &mut sim::SimState, hud: &mut HudState, h: Harvest, a: Cell, b: Cell) {
    use sim::resources::ResourceKind as R;
    let who = panels::order_crew(s, hud);
    if who.is_empty() {
        hud.say(Some((Icon::Beaver, BAD)), Some((Icon::Cross, BAD)), "0");
        return;
    }
    let max = s.world.size - 1;
    let area = if a == b {
        let r = 6;
        sim::work::Area { min: (a.0.saturating_sub(r), a.1.saturating_sub(r)), max: ((a.0 + r).min(max), (a.1 + r).min(max)) }
    } else {
        sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) }
    };
    let wanted: Vec<R> = R::all().filter(|k| match h {
        Harvest::Cut => k.is_tree(),
        Harvest::Gather => !k.is_tree() && !k.is_felled() && !k.aquatic() && k.row().needs.is_none(),
    }).collect();
    let here = |k: R| s.resources.items().iter().any(|r| r.kind == k && area.contains(r.cell) && (r.amount > 0.0 || k.flowing()));
    let at_click = s.resources.at(a).find(|r| wanted.contains(&r.kind)).map(|r| r.kind);
    let Some(kind) = at_click.or_else(|| wanted.iter().copied().find(|k| here(*k))) else {
        hud.say(Some((if h == Harvest::Cut { crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()) } else { crate::hud::words::resource_icon(sim::resources::ResourceKind::food_plant(false)) }, BAD)), Some((Icon::Cross, BAD)), "");
        return;
    };
    match s.apply(sim::Command::Assign { who: who.clone(), to: Some(sim::agents::Assignment::Area(kind, area)), queue: false }) {
        Ok(_) => {
            hud.say(Some((resource_icon(kind), GOOD)), Some((Icon::Beaver, ui::FUR)), format!("{}", who.len()));
            hud.set_tool(Tool::None);
        }
        Err(e) => hud.refuse(sim_error(&e.to_string())),
    }
}

fn letter(s: &str) -> Option<KeyCode> {
    use KeyCode::*;
    const KEYS: [KeyCode; 26] = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
    let c = s.chars().next()?.to_ascii_uppercase();
    c.is_ascii_uppercase().then(|| KEYS[(c as u8 - b'A') as usize])
}
