//! Turns HUD actions into simulation commands and user feedback.
use super::*;

pub(super) fn commit_line(sim: &mut sim::SimState, hud: &mut HudState, now: f32) {
    if hud.line.is_empty() {
        return;
    }
    let order = match hud.tool {
        Tool::Dam => match hud.dam_crest {
            Some(crest) => BuildOrder::DamTo { cells: hud.line.clone(), height: hud.dam_height, crest },
            None => BuildOrder::Dam { cells: hud.line.clone(), height: hud.dam_height },
        },
        Tool::Canal => BuildOrder::Canal { cells: hud.line.clone() },
        _ => return,
    };
    let canal = matches!(order, BuildOrder::Canal { .. });
    match build_logic::emit(sim, &order) {
        Ok(_) => {
            let _ = canal;
            let item = sim.buildings.buildings.last().map(|b| UndoItem::Site(b.id));
            hud.undo.extend(item);
            hud.line.clear();
            hud.dam_crest = None;
            hud.preview = None;
        }
        Err(e) => hud.refuse(place_error(&e), now),
    }
}

pub(super) fn extend_dam(s: &sim::SimState, hud: &mut HudState) {
    if hud.tool != Tool::Dam || hud.line.is_empty() {
        return;
    }
    if let Some((cells, crest)) = build_logic::extend_line(s, &hud.line, hud.dam_height, hud.dam_crest) {
        hud.line = cells;
        hud.dam_crest = Some(crest);
    }
}

pub(super) fn set_route(s: &mut sim::SimState, hud: &mut HudState, barge: BargeId, now: f32) {
    let d = RouteDraft::of(s, hud, barge);
    let (Some(from), Some(to)) = (d.from, d.to) else { return };
    match s.apply(sim::Command::SetBargeRoute { barge, from, to, goods: d.goods.clone() }) {
        Ok(()) => {
            hud.route_draft = None;
            hud.route_err = None;
        }
        Err(e) => {
            let m = e.to_string();
            hud.refuse(sim_error(&m), now);
            hud.route_err = Some((barge, m));
        }
    }
}

pub(super) fn undo_last(sim: &mut sim::SimState, hud: &mut HudState, now: f32) {
    while let Some(item) = hud.undo.pop() {
        match item {
            UndoItem::Site(id) => {
                let Some(kind) = sim.buildings.building(id).map(|b| b.kind) else { continue };
                let stock: Vec<(sim::buildings::Good, f32)> = sim::buildings::Good::ALL.iter().map(|g| (*g, sim.buildings.stores.amount(*g))).collect();
                match sim.apply(sim::Command::CancelSite { building: id }).map(|_| stock.iter().map(|(g, a)| (*g, sim.buildings.stores.amount(*g) - a)).collect::<Vec<_>>()) {
                    Ok(back) => {
                        let refund = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| format!("+{}", a.1.round() as i64));
                        let icon2 = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| (good_icon(a.0), good_color(a.0)));
                        hud.msg = Some((Msg { icon: Some((Icon::Undo, INK)), icon2: icon2.or(Some((building_icon(kind), INK))), text: refund.unwrap_or_default() }, now));
                    }
                    Err(e) => hud.say(Some((Icon::Undo, INK)), Some((Icon::Cross, BAD)), sim_error(&e.to_string()), now),
                }
                return;
            }
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
) {
    let now = time.elapsed_secs();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let actions: Vec<HudAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| a.clone()).collect();
    for a in actions {
        let s = &mut sim.0;
        match a {
            HudAction::Help => hud.help = !hud.help,
            HudAction::Order(mode) => {
                hud.set_tool(Tool::None);
                hud.order_mode = Some(mode);
            }
            HudAction::SelectFamily => {
                hud.selected = s.colony.beavers.iter().filter(|b| b.stage != sim::colony::Stage::Kit).map(|b| b.id).collect();
                if let Some(&id) = hud.selected.first() {
                    *sel = Selection::Beaver(id);
                    flow.focus = world::beaver_world(s, id);
                }
            }
            HudAction::StopSelected => {
                for id in hud.selected.clone() { free_beaver(s, &mut hud, id, now); }
                hud.order_mode = None;
                hud.say(Some((Icon::Check, GOOD)), None, "Orders cancelled; available for colony jobs", now);
            }
            HudAction::Fix(fix, at) => match fix {
                Fix::Place(kind) => hud.set_tool(if kind == BuildingKind::Dam { Tool::Dam } else { Tool::Building(kind) }),
                Fix::OpenJobs => hud.colony = Some(ColonyTab::Jobs),
                Fix::Focus => {
                    if let Some(c) = at {
                        flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32)));
                    }
                }
                f => {
                    if let Err(e) = sim::hud::apply_fix(s, f) {
                        hud.refuse(sim_error(&e), now);
                    }
                }
            },
            HudAction::Focus(c) => flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32))),
            HudAction::CycleIdle => {
                if shift {
                    hud.colony = Some(ColonyTab::Jobs);
                } else {
                    next_idle(s, &mut hud, &mut sel, &mut flow);
                }
            }
            HudAction::Pause => {
                if !clock.paused { clock.toggle_pause(); }
                hud.set_changed();
            }
            HudAction::Speed(sp) => {
                clock.set_speed(sp);
                hud.set_changed();
            }
            HudAction::OpenCat(i) => {
                hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) };
                hud.search = None;
            }
            HudAction::Pick(kind) => {
                hud.help = false;
                let t = match kind {
                    BuildingKind::Dam => Tool::Dam,
                    BuildingKind::Canal => Tool::Canal,
                    k => Tool::Building(k),
                };
                if s.can_build(kind) {
                    let next = if hud.tool == t { Tool::None } else { t };
                    hud.set_tool(next);
                    hud.search = None;
                    *sel = Selection::None;
                } else {
                    hud.refuse(format!("Build {} first", s.missing_prerequisites(kind).iter().map(|k| k.spec().name).collect::<Vec<_>>().join(", ")), now);
                }
            }
            HudAction::CancelTool => hud.set_tool(Tool::None),
            HudAction::DamHeight(d) => hud.step_height(d),
            HudAction::ExtendDam => extend_dam(s, &mut hud),
            HudAction::RaiseDam(c) => match build_logic::raise_order(s, c) {
                Some(order) => match build_logic::emit(s, &order) {
                    Ok(_) => {
                        if let Some(b) = s.buildings.buildings.last() {
                            hud.undo.push(UndoItem::Site(b.id));
                        }
                    }
                    Err(e) => hud.refuse(place_error(&e), now),
                },
                None => hud.refuse(format!("Max {}", num(MAX_DAM_HEIGHT)), now),
            },
            HudAction::SelectBarge(id) => {
                *sel = Selection::Barge(id);
                hud.selected.clear();
                if let Some(b) = s.buildings.barge(id) {
                    flow.focus = Some(cell_world(&s.height, (b.cell.0 as f32 + 0.5, b.cell.1 as f32 + 0.5)));
                }
            }
            HudAction::RouteFrom(barge, dock) => {
                let mut d = RouteDraft::of(s, &hud, barge);
                d.from = if d.from == Some(dock) { None } else { Some(dock) };
                if d.to == d.from {
                    d.to = None;
                }
                hud.route_draft = Some(d);
            }
            HudAction::RouteTo(barge, dock) => {
                let mut d = RouteDraft::of(s, &hud, barge);
                d.to = if d.to == Some(dock) { None } else { Some(dock) };
                if d.to == d.from {
                    d.from = None;
                }
                hud.route_draft = Some(d);
            }
            HudAction::RouteGood(barge, g) => {
                let mut d = RouteDraft::of(s, &hud, barge);
                match d.goods.iter().position(|x| *x == g) {
                    Some(i) => {
                        d.goods.remove(i);
                    }
                    None => d.goods.push(g),
                }
                hud.route_draft = Some(d);
            }
            HudAction::SetRoute(barge) => set_route(s, &mut hud, barge, now),
            HudAction::ClearRoute(barge) => {
                cmd(s, &mut hud, sim::Command::ClearBargeRoute { barge }, now);
                hud.route_draft = None;
                hud.route_err = None;
            }
            HudAction::ConfirmLine => commit_line(s, &mut hud, now),
            HudAction::ClearLine => {
                hud.line.clear();
                hud.dam_crest = None;
                hud.preview = None;
            }
            HudAction::CloseInspector => {
                *sel = Selection::None;
                hud.selected.clear();
                hud.follow = None;
            }
            HudAction::SetLens(l) => {
                *lens = l;
                hud.set_changed();
            }
            HudAction::Undo => undo_last(s, &mut hud, now),
            HudAction::OpenSearch => hud.search = if hud.search.is_some() { None } else { Some(String::new()) },
            HudAction::Colony(t) => hud.colony = t,
            HudAction::ToggleDetails => hud.details_open = !hud.details_open,
            HudAction::ToggleMinimap => hud.minimap_collapsed = !hud.minimap_collapsed,
            HudAction::ToggleProfile => hud.profile_collapsed = !hud.profile_collapsed,
            HudAction::SelectBeaver(id) => {
                *sel = Selection::Beaver(id);
                if shift {
                    if hud.selected.contains(&id) { hud.selected.retain(|b| *b != id); }
                    else { hud.selected.push(id); }
                } else {
                    hud.selected = vec![id];
                }
            }
            HudAction::SelectBuilding(id) => {
                *sel = Selection::Site(id);
                hud.selected.clear();
                if let Some(b) = s.buildings.building(id) {
                    flow.focus = Some(cell_world(&s.height, (b.pos.0 as f32, b.pos.1 as f32)));
                }
            }
            HudAction::Follow(id) => hud.follow = if hud.follow == Some(id) { None } else { Some(id) },
            HudAction::FreeBeaver(id) => free_beaver(s, &mut hud, id, now),
            HudAction::AddWorker(building) => cmd(s, &mut hud, sim::Command::AddWorker { building }, now),
            HudAction::RemoveWorker(building) => cmd(s, &mut hud, sim::Command::RemoveWorker { building }, now),
            HudAction::RoleAdjust(role, delta) => cmd(s, &mut hud, sim::Command::AdjustRole { role, delta }, now),
            HudAction::RoleWeight(role, weight) => cmd(s, &mut hud, sim::Command::SetRoleWeight { role, weight }, now),
            HudAction::RoleUp(role) => {
                let mut order = s.work.role_order.clone();
                if let Some(i) = order.iter().position(|r| *r == role).filter(|i| *i > 0) {
                    order.swap(i, i - 1);
                    cmd(s, &mut hud, sim::Command::SetRoleOrder { order }, now);
                }
            }
            HudAction::Priority(building, priority) => cmd(s, &mut hud, sim::Command::SetPriority { building, priority }, now),
            HudAction::Paused(building, paused) => cmd(s, &mut hud, sim::Command::SetPaused { building, paused }, now),
            HudAction::Unlock(who) => cmd(s, &mut hud, sim::Command::Unlock { who }, now),
            HudAction::RepairBuilding(building) => cmd(s, &mut hud, sim::Command::RepairBuilding { building }, now),
            HudAction::ReinforceBuilding(building) => cmd(s, &mut hud, sim::Command::ReinforceBuilding { building }, now),
            HudAction::CancelMaintenance(building) => cmd(s, &mut hud, sim::Command::CancelSite { building }, now),
            HudAction::AutoStaff(on) => cmd(s, &mut hud, sim::Command::SetAutoStaff { on }, now),
            HudAction::SortPop(k) => { hud.pop_sort = k; hud.pop_page = 0; }
            HudAction::PopPage(delta) => {
                hud.pop_page = hud.pop_page.saturating_add_signed(delta as isize)
                    .min(sim.0.colony.population().saturating_sub(1) / 18);
            }
            HudAction::Balance(building, on) => cmd(s, &mut hud, sim::Command::SetBalance { building, on }, now),
            HudAction::CutRule(building, rule) => cmd(s, &mut hud, sim::Command::SetCutRule { building, rule }, now),
            HudAction::PlantRule(building, rule) => cmd(s, &mut hud, sim::Command::SetPlantRule { building, rule }, now),
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
            HudAction::EditArea(b) => {
                hud.area_edit = if hud.area_edit == Some(b) { None } else { Some(b) };
                hud.area_from = None;
            }
        }
    }
}

fn cmd(s: &mut sim::SimState, hud: &mut HudState, c: sim::Command, now: f32) {
    if let Err(e) = s.apply(c) {
        hud.refuse(sim_error(&e.to_string()), now);
    }
}

fn free_beaver(s: &mut sim::SimState, hud: &mut HudState, id: BeaverId, now: f32) {
    let res = match s.work.slot_of(id) {
        Some((b, slot, _)) => s.apply(sim::Command::EmptySlot { building: b, slot }),
        None => s.apply(sim::Command::Assign { who: vec![id], to: None }),
    };
    if let Err(e) = res {
        hud.refuse(sim_error(&e.to_string()), now);
    }
}

pub fn drawn_area(s: &sim::SimState, id: BuildingId, a: Cell, b: Cell) -> sim::work::Area {
    let max = s.buildings.building(id).and_then(|bd| sim::forestry::max_area(bd.kind)).unwrap_or(usize::MAX);
    let clamp = |from: usize, to: usize| if to >= from { from + (to - from).min(max - 1) } else { from - (from - to).min(max - 1) };
    let b = (clamp(a.0, b.0), clamp(a.1, b.1));
    sim::work::Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) }
}

pub(super) fn forest_kind(kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::LoggingCamp | BuildingKind::ForesterLodge)
}
