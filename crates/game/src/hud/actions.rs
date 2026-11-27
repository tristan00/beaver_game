use super::*;

pub(super) fn commit_line(sim: &mut sim::SimState, hud: &mut HudState) -> Option<BuildingId> {
    if hud.line.is_empty() || !matches!(hud.tool, Tool::Dam | Tool::Canal) {
        return None;
    }
    let extending = hud.extending;
    let res: Result<(), String> = match (hud.tool, extending) {
        (Tool::Canal, Some(canal)) => sim.apply(sim::Command::GrowCanal { canal, growth: sim::CanalGrowth::Extend, cells: hud.line.clone() }).map(|_| ()).map_err(|e| sim_error(&e.to_string())),
        (Tool::Canal, None) => sim.apply(sim::Command::PlaceCanal { cells: hud.line.clone() }).map(|_| ()).map_err(|e| sim_error(&e.to_string())),
        (Tool::Dam, Some(dam)) => sim.apply(sim::Command::GrowDam { dam, growth: sim::DamGrowth::Extend, cells: hud.line.clone() }).map(|_| ()).map_err(|e| sim_error(&e.to_string())),
        (Tool::Dam, None) => sim.apply(sim::Command::PlaceDam { cells: hud.line.clone() }).map(|_| ()).map_err(|e| sim_error(&e.to_string())),
        _ => return None,
    };
    match res {
        Ok(()) => {
            let id = extending.or_else(|| sim.buildings.buildings.last().map(|b| b.id));
            if extending.is_none() {
                hud.undo.extend(id);
                hud.dam_cells.clear();
            } else {
                hud.dam_ghost = None;
            }
            hud.finish_placement();
            id
        }
        Err(msg) => {
            hud.refuse(msg);
            None
        }
    }
}

pub(super) fn undo_last(sim: &mut sim::SimState, hud: &mut HudState, sel: &mut Selection) {
    while let Some(id) = hud.undo.pop() {
        let Some(kind) = sim.buildings.building(id).map(|b| b.kind) else { continue };
        let stock: Vec<(sim::buildings::Good, f32)> = sim::buildings::Good::all().map(|g| (g, sim.buildings.stores.amount(g))).collect();
        match sim.apply(sim::Command::CancelSite { building: id }).map(|_| stock.iter().map(|(g, a)| (*g, sim.buildings.stores.amount(*g) - a)).collect::<Vec<_>>()) {
            Ok(back) => {
                clean_removed_site(sim, hud, sel, id);
                let refund = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| format!("+{}", a.1.round() as i64));
                let icon2 = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| (good_icon(a.0), good_color(a.0)));
                hud.msg = Some(Msg { icon: Some((Icon::Undo, INK)), icon2: icon2.or(Some((building_icon(kind), INK))), text: refund.unwrap_or_default() });
            }
            Err(e) => hud.say(Some((Icon::Undo, INK)), Some((Icon::Cross, BAD)), sim_error(&e.to_string())),
        }
        return;
    }
}

fn clean_removed_site(s: &sim::SimState, hud: &mut HudState, sel: &mut Selection, building: BuildingId) {
    if s.buildings.building(building).is_none() {
        hud.pinned.retain(|p| *p != Selection::Site(building));
        hud.dam_cells.clear();
        if *sel == Selection::Site(building) {
            *sel = Selection::None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn button_actions(
    q: Query<(&Interaction, &HudAction), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut clock: ResMut<SimClock>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
    mut lens: ResMut<MapLens>,
    orbit: Res<crate::camera::OrbitCamera>,
) {
    let now = time.elapsed_secs();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let actions: Vec<HudAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| a.clone()).collect();
    for a in actions {
        sim::trace::event(format!("HUD_ACTION {a:?}"));
        let s = &mut sim.0;
        match a {
            HudAction::Cmd(c) => cmd(s, &mut hud, c),
            HudAction::Fix(fix, at) => match fix {
                Fix::Place(kind) => hud.set_tool(Tool::of(kind)),
                Fix::OpenJobs => hud.set_overlay(Some(Overlay::Colony(ColonyTab::Jobs))),
                Fix::OpenDam { dam } => {
                    if hud.page().is_some() { hud.set_overlay(None); }
                    hud.select_building(s, &mut sel, &mut flow, dam);
                    if let Some(c) = at {
                        flow.focus = Some(cell_world(&s.height, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)));
                    }
                }
                Fix::Focus => {
                    if let Some(c) = at {
                        flow.focus = Some(cell_world(&s.height, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)));
                    }
                }
                f => {
                    if let Err(e) = sim::hud::apply_fix(s, f) {
                        hud.refuse(sim_error(&e));
                    }
                }
            },
            HudAction::CycleIdle => {
                if shift {
                    hud.set_overlay(Some(Overlay::Colony(ColonyTab::Jobs)));
                } else {
                    next_idle(s, &mut hud, &mut sel, &mut flow);
                }
            }
            HudAction::Pause => {
                clock.toggle_pause();
                hud.set_changed();
            }
            HudAction::Speed(sp) => {
                clock.pick_speed(sp);
                hud.set_changed();
            }
            HudAction::Pick(kind) if s.unlocked(kind).is_err() => {}
            HudAction::Pick(kind) => {
                let t = Tool::of(kind);
                let next = if hud.tool == t { Tool::None } else { t };
                hud.set_tool(next);
                hud.select(&mut sel, Selection::None);
            }
            HudAction::CancelTool => hud.set_tool(Tool::None),
            HudAction::GrowDam(dam, growth) => {
                let cells = if growth == sim::DamGrowth::Raise { picks_on(s, dam, &hud.dam_cells) } else { Vec::new() };
                cmd(s, &mut hud, sim::Command::GrowDam { dam, growth, cells });
                hud.dam_ghost = None;
                hud.dam_cells.clear();
            }
            HudAction::Opening(dam, o) => {
                if hud.opening() == Some((dam, o)) {
                    hud.set_tool(Tool::None);
                } else {
                    hud.set_tool(Tool::Opening(dam, o));
                    hud.swallow_click();
                    hud.dam_cells.clear();
                    hud.select(&mut sel, Selection::Site(dam));
                    if let Some(mid) = sim::dams::dam(s, dam).and_then(|d| d.line().nth(d.line().count() / 2).map(|c| c.cell)) {
                        let (vx, vy) = crate::flow::world_cell(&s.height, orbit.target);
                        if (vx - mid.0 as f32).hypot(vy - mid.1 as f32) > 24.0 {
                            flow.focus = Some(cell_world(&s.height, (mid.0 as f32 + 0.5, mid.1 as f32 + 0.5)));
                        }
                    }
                }
            }
            HudAction::GateChip(building, cell) => {
                hud.gate_sel = if hud.gate_sel == Some((building, cell)) { None } else { Some((building, cell)) };
                if hud.opening().is_some() { hud.set_tool(Tool::None); }
                flow.focus = Some(cell_world(&s.height, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5)));
            }
            HudAction::GrowCanal(canal, growth) => {
                if growth == sim::CanalGrowth::Extend {
                    if let Some(end) = sim::canals::canal(s, canal).map(|k| k.end()) {
                        hud.set_tool(Tool::Canal);
                        hud.extending = Some(canal);
                        hud.corners = vec![end];
                        hud.swallow_click();
                    }
                } else {
                    let cells = picks_on(s, canal, &hud.dam_cells);
                    cmd(s, &mut hud, sim::Command::GrowCanal { canal, growth, cells });
                    hud.dam_cells.clear();
                }
            }
            HudAction::DamCell(c) => {
                match hud.dam_cells.iter().position(|x| *x == c) {
                    Some(i) => { hud.dam_cells.remove(i); }
                    None if shift => hud.dam_cells.push(c),
                    None => hud.dam_cells = vec![c],
                }
            }
            HudAction::DamTab(t) => hud.dam_tab = t,
            HudAction::ConfirmLine => {
                if let Some(id) = commit_line(s, &mut hud) {
                    hud.select(&mut sel, Selection::Site(id));
                }
            }
            HudAction::SetLens(l) => {
                *lens = l;
                hud.set_changed();
            }
            HudAction::PathsOnly(r) => {
                hud.paths_only = if hud.paths_only == Some(r) { None } else { Some(r) };
            }
            HudAction::Overlay(o) => hud.set_overlay(o),
            HudAction::PageBack => hud.page_back(),
            HudAction::CloseWelcome => flow.welcome = false,
            HudAction::Kin(f) => hud.kin = f,
            HudAction::Span(sp) => hud.span = sp,
            HudAction::Sort(col) => {
                hud.sort = if hud.sort.0 == col { (col, !hud.sort.1) } else { (col, !matches!(col, BeaverCol::Name | BeaverCol::Family | BeaverCol::Job | BeaverCol::Doing)) };
            }
            HudAction::Journal(open) => hud.journal_open = open,
            HudAction::GoTo(at) => {
                if hud.page().is_some() { hud.set_overlay(None); }
                match at {
                    NoticeAt::Beaver(id) => hud.select_beaver(s, &mut sel, &mut flow, id),
                    NoticeAt::Building(id) => hud.select_building(s, &mut sel, &mut flow, id),
                    NoticeAt::Spot(p) => flow.focus = Some(cell_world(&s.height, p)),
                    NoticeAt::Wolf(fallback) => {
                        if let Some(to) = sim::predators::in_sight(s).or_else(|| sim::predators::last_seen(s)).or(fallback) {
                            flow.focus = Some(cell_world(&s.height, to));
                        }
                    }
                }
            }
            HudAction::Follow(id) => hud.follow = if hud.follow == Some(id) { None } else { Some(id) },
            HudAction::Badge(key) => hud.open_badge = if hud.open_badge.as_ref() == Some(&key) { None } else { Some(key) },
            HudAction::ClearPicks => hud.dam_cells.clear(),
            HudAction::ClosePanel(p, selected) => {
                hud.pinned.retain(|x| *x != p);
                if selected {
                    hud.clear_selection(&mut sel);
                }
            }
            HudAction::RemoveSite(building, confirm) => {
                if !confirm || hud.remove_armed.is_some_and(|(b, t)| b == building && now - t <= super::REMOVE_ARM_SECS) {
                    hud.remove_armed = None;
                    cmd(s, &mut hud, sim::Command::CancelSite { building });
                    clean_removed_site(s, &mut hud, &mut sel, building);
                } else {
                    hud.remove_armed = Some((building, now));
                }
            }
            HudAction::Pin(p) => {
                if let Some(i) = hud.pinned.iter().position(|x| *x == p) {
                    hud.pinned.remove(i);
                } else {
                    hud.pinned.push(p);
                    if hud.pinned.len() > 3 {
                        hud.pinned.remove(0);
                    }
                }
            }
        }
    }
}

fn picks_on(s: &sim::SimState, id: sim::buildings::BuildingId, picks: &[Cell]) -> Vec<Cell> {
    let Some(b) = s.buildings.building(id) else { return Vec::new() };
    let on = |c: &Cell| b.dam.as_ref().is_some_and(|d| d.line().any(|x| x.cell == *c)) || b.canal.as_ref().is_some_and(|k| k.cells.iter().any(|x| x.cell == *c));
    picks.iter().copied().filter(|c| on(c)).collect()
}

fn cmd(s: &mut sim::SimState, hud: &mut HudState, c: sim::Command) {
    if let Err(e) = s.apply(c) {
        hud.refuse(sim_error(&e.to_string()));
    }
}
