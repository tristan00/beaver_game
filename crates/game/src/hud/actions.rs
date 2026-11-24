//! Turns HUD actions into simulation commands and user feedback.
use super::*;

/// Build the drawn line: a new dam, or more cells on the dam being extended.
/// Returns the dam, so its panel can open.
pub(super) fn commit_line(sim: &mut sim::SimState, hud: &mut HudState, now: f32) -> Option<BuildingId> {
    if hud.line.is_empty() || !matches!(hud.tool, Tool::Dam | Tool::Canal) {
        return None;
    }
    if hud.tool == Tool::Canal {
        let cmd = match hud.extending {
            Some(canal) => {
                let end = sim::canals::canal(sim, canal).map(|k| k.end());
                sim::Command::GrowCanal { canal, growth: sim::CanalGrowth::Extend, cells: hud.line.iter().copied().filter(|c| Some(*c) != end).collect() }
            }
            None => sim::Command::PlaceCanal { cells: hud.line.clone() },
        };
        let extending = hud.extending;
        return match sim.apply(cmd) {
            Ok(_) => {
                let id = extending.or_else(|| sim.buildings.buildings.last().map(|b| b.id));
                if extending.is_none() {
                    hud.undo.extend(id.map(UndoItem::Site));
                }
                hud.finish_placement();
                hud.dam_cells.clear();
                id
            }
            Err(e) => {
                hud.refuse(sim_error(&e.to_string()), now);
                None
            }
        };
    }
    if let Some(dam) = hud.extending {
        let cells: Vec<Cell> = hud.line.clone();
        return match sim.apply(sim::Command::GrowDam { dam, growth: sim::DamGrowth::Extend, cells }) {
            Ok(_) => {
                hud.finish_placement();
                hud.dam_ghost = None;
                Some(dam)
            }
            Err(e) => {
                hud.refuse(sim_error(&e.to_string()), now);
                None
            }
        };
    }
    match build_logic::emit(sim, &BuildOrder::Dam { cells: hud.line.clone() }) {
        Ok(_) => {
            let id = sim.buildings.buildings.last().map(|b| b.id);
            hud.undo.extend(id.map(UndoItem::Site));
            hud.finish_placement();
            hud.dam_cells.clear();
            id
        }
        Err(e) => {
            hud.refuse(place_error(&e), now);
            None
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
        sim::trace::event(format!("HUD_ACTION {a:?}"));
        let s = &mut sim.0;
        match a {
            HudAction::Fix(fix, at) => match fix {
                Fix::Place(kind) => hud.set_tool(match kind { BuildingKind::Dam => Tool::Dam, BuildingKind::Canal => Tool::Canal, k => Tool::Building(k) }),
                Fix::OpenJobs => hud.colony = Some(ColonyTab::Jobs),
                Fix::OpenDam { dam } => {
                    *sel = Selection::Site(dam);
                    hud.selected.clear();
                    if let Some(c) = at {
                        flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32)));
                    }
                }
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
            HudAction::FocusWolf => {
                if let Some(to) = sim::predators::in_sight(s).or_else(|| sim::predators::last_seen(s)) {
                    flow.focus = Some(cell_world(&s.height, to));
                }
            }
            HudAction::CycleIdle => {
                if shift {
                    hud.colony = Some(ColonyTab::Jobs);
                } else {
                    next_idle(s, &mut hud, &mut sel, &mut flow);
                }
            }
            HudAction::Pause => {
                clock.toggle_pause();
                hud.set_changed();
            }
            HudAction::Speed(sp) => {
                clock.set_speed(sp);
                hud.set_changed();
            }
            HudAction::Pick(kind) if s.unlocked(kind).is_err() => {}
            HudAction::Pick(kind) => {
                let t = match kind {
                    BuildingKind::Dam => Tool::Dam,
                    BuildingKind::Canal => Tool::Canal,
                    k => Tool::Building(k),
                };
                let next = if hud.tool == t { Tool::None } else { t };
                hud.set_tool(next);
                *sel = Selection::None;
            }
            HudAction::CancelTool => hud.set_tool(Tool::None),
            HudAction::GrowDam(dam, growth) => {
                let cells = if growth == sim::DamGrowth::Raise { hud.dam_cells.clone() } else { Vec::new() };
                cmd(s, &mut hud, sim::Command::GrowDam { dam, growth, cells }, now);
                hud.dam_ghost = None;
            }
            HudAction::DamMaintain(dam, on) => cmd(s, &mut hud, sim::Command::SetDamMaintain { dam, on }, now),
            HudAction::AddDamGate(dam) => {
                if let Some(&cell) = hud.dam_cells.first() {
                    cmd(s, &mut hud, sim::Command::AddDamGate { dam, cell }, now);
                    hud.dam_cells.clear();
                }
            }
            HudAction::SetGate(building, cell, setting) => cmd(s, &mut hud, sim::Command::SetGate { building, cell, setting }, now),
            HudAction::UpgradeLock(building, cell) => cmd(s, &mut hud, sim::Command::UpgradeLock { building, cell }, now),
            HudAction::CanalMaintain(canal, on) => cmd(s, &mut hud, sim::Command::SetCanalMaintain { canal, on }, now),
            HudAction::GrowCanal(canal, growth) => {
                if growth == sim::CanalGrowth::Extend {
                    // Draw on from the open end: the player drags (or clicks corners) the new
                    // run out; the click on the button itself draws nothing.
                    if let Some(end) = sim::canals::canal(s, canal).map(|k| k.end()) {
                        hud.set_tool(Tool::Canal);
                        hud.extending = Some(canal);
                        hud.corners = vec![end];
                        hud.swallow_click();
                    }
                } else {
                    let cells = hud.dam_cells.clone();
                    cmd(s, &mut hud, sim::Command::GrowCanal { canal, growth, cells }, now);
                    hud.dam_cells.clear();
                }
            }
            HudAction::DamCell(c) => {
                match hud.dam_cells.iter().position(|x| *x == c) {
                    Some(i) => {
                        hud.dam_cells.remove(i);
                    }
                    None if shift => hud.dam_cells.push(c),
                    None => hud.dam_cells = vec![c],
                }
            }
            HudAction::DamTab(t) => hud.dam_tab = t,
            HudAction::ConfirmLine => {
                if let Some(id) = commit_line(s, &mut hud, now) {
                    *sel = Selection::Site(id);
                    hud.selected.clear();
                }
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
            HudAction::PathsOnly(r) => {
                super::paths::toggle_only(r);
                hud.set_changed();
            }
            HudAction::BeaverDetails(who) => {
                hud.details = who;
                hud.scroll = 0;
                if who.is_some() {
                    hud.ledger = None;
                    hud.lodge_page = None;
                }
            }
            HudAction::LodgePage(lodge) => {
                hud.lodge_page = lodge;
                hud.scroll = 0;
                if lodge.is_some() {
                    hud.ledger = None;
                    hud.details = None;
                }
            }
            HudAction::Kin(f) => {
                hud.kin = f;
            }
            HudAction::Colony(t) => {
                hud.details = None;
                hud.lodge_page = None;
                hud.colony = t;
                if t.is_some() {
                    hud.ledger = None;
                }
            }
            HudAction::Ledger(t) => {
                if t != hud.ledger {
                    hud.scroll = 0;
                }
                hud.ledger = t;
                if t.is_some() {
                    hud.colony = None;
                }
            }
            HudAction::Span(sp) => hud.span = sp,
            HudAction::Sort(col) => {
                hud.sort = if hud.sort.0 == col { (col, !hud.sort.1) } else { (col, !matches!(col, BeaverCol::Name | BeaverCol::Family | BeaverCol::Job)) };
                hud.scroll = 0;
            }
            HudAction::Scroll(n) => hud.scroll = ((hud.scroll as i64 + n as i64).max(0) as usize).min(super::longest_list(s)),
            HudAction::Journal(open) => hud.journal_open = open,
            HudAction::GoTo(at) => match at {
                NoticeAt::Beaver(id) if s.colony.beaver(id).is_some() => {
                    *sel = Selection::Beaver(id);
                    hud.selected = vec![id];
                    if let Some(b) = s.colony.beaver(id) {
                        flow.focus = Some(cell_world(&s.height, b.pos));
                    }
                }
                NoticeAt::Building(id) if s.buildings.building(id).is_some() => {
                    *sel = Selection::Site(id);
                    hud.selected.clear();
                    if let Some(b) = s.buildings.building(id) {
                        flow.focus = Some(cell_world(&s.height, (b.pos.0 as f32, b.pos.1 as f32)));
                    }
                }
                NoticeAt::Spot(p) => flow.focus = Some(cell_world(&s.height, p)),
                NoticeAt::Wolf(p) => {
                    let to = sim::predators::in_sight(s).or_else(|| sim::predators::last_seen(s)).unwrap_or(p);
                    flow.focus = Some(cell_world(&s.height, to));
                }
                _ => {}
            },
            HudAction::SelectBeaver(id) => {
                *sel = Selection::Beaver(id);
                hud.selected = vec![id];
                if let Some(b) = s.colony.beaver(id) {
                    flow.focus = Some(cell_world(&s.height, b.pos));
                }
            }
            HudAction::SelectBuilding(id) => {
                *sel = Selection::Site(id);
                hud.selected.clear();
                hud.dam_cells.clear();
                if let Some(b) = s.buildings.building(id) {
                    flow.focus = Some(cell_world(&s.height, (b.pos.0 as f32, b.pos.1 as f32)));
                }
            }
            HudAction::Follow(id) => hud.follow = if hud.follow == Some(id) { None } else { Some(id) },
            HudAction::FreeBeaver(id) => free_beaver(s, &mut hud, id, now),
            HudAction::AddWorker(building) => cmd(s, &mut hud, sim::Command::AddWorker { building }, now),
            HudAction::Badge(key) => hud.open_badge = if hud.open_badge.as_ref() == Some(&key) { None } else { Some(key) },
            HudAction::FillStore(store, on) => cmd(s, &mut hud, sim::Command::FillStore { store, on }, now),
            HudAction::Balance(building, on) => cmd(s, &mut hud, sim::Command::SetBalance { building, on }, now),
            HudAction::RemoveWorker(building) => cmd(s, &mut hud, sim::Command::RemoveWorker { building }, now),
            HudAction::RoleAdjust(role, delta) => cmd(s, &mut hud, sim::Command::AdjustRole { role, delta }, now),
            HudAction::ShoreLimit(role, cells) => cmd(s, &mut hud, sim::Command::SetShoreLimit { role, cells }, now),
            HudAction::Paused(building, paused) => cmd(s, &mut hud, sim::Command::SetPaused { building, paused }, now),
            HudAction::RepairBuilding(building) => cmd(s, &mut hud, sim::Command::RepairBuilding { building }, now),
            HudAction::ReinforceBuilding(building) => cmd(s, &mut hud, sim::Command::ReinforceBuilding { building }, now),
            HudAction::PebbleBuilding(building) => cmd(s, &mut hud, sim::Command::PebbleBuilding { building }, now),
            HudAction::SlotOff(building, slot, off) => cmd(s, &mut hud, sim::Command::SetSlotOff { building, slot, off }, now),
            HudAction::CancelMaintenance(building) => cmd(s, &mut hud, sim::Command::CancelSite { building }, now),
            HudAction::CancelBuild(building) => {
                cmd(s, &mut hud, sim::Command::CancelSite { building }, now);
                hud.pinned.retain(|p| *p != Selection::Site(building));
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
