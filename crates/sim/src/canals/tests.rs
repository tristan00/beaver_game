use super::*;
use crate::buildings::Good;
use crate::Command;

/// A flat pond (y 15..26, x 8..17, 0.3 m deep at ground 5 m) with dry land rising east of it.
fn pond_and_bank() -> SimState {
    let mut s = SimState::new(1);
    s.sandbox = false;
    for y in 10..32 {
        for x in 8..48 {
            let g = if x < 17 { 5.0 } else { 5.4 + 0.01 * (x - 17) as f32 };
            *s.height.get_mut(x, y).unwrap() = g;
            *s.water.depth.get_mut(x, y).unwrap() = if x < 17 && (15..26).contains(&y) { 0.3 } else { 0.0 };
        }
    }
    s.fog = crate::fog::Fog::clear(s.world.size);
    // Canals are unlocked by a finished dam: one stands far off in a corner.
    s.buildings.buildings.push(crate::buildings::Building {
        id: crate::buildings::BuildingId(900),
        kind: crate::buildings::BuildingKind::Dam,
        pos: (0, 0),
        status: crate::buildings::Status::Complete,
        output: 0.0,
        condition: 1.0,
        reinforced: false,
        dam: None,
        canal: None,
    });
    s
}

fn route(from: Cell, to: Cell) -> Vec<Cell> {
    crate::sandbox::canal_line(&[from, to])
}

fn stock_sticks(s: &mut SimState, amount: f32) {
    s.work.supplies = Some((5, 5));
    s.buildings.stores.put_at(crate::agents::SUPPLIES, Good::Sticks, amount).unwrap();
}

#[test]
fn a_canal_must_start_at_water_and_run_cell_to_cell() {
    let s = pond_and_bank();
    assert_eq!(plan(&s, &route((20, 20), (30, 20))).unwrap_err(), "Must start at water");
    assert!(plan(&s, &[(16, 20), (17, 21)]).is_err(), "a corner-only step is refused");
    let p = plan(&s, &route((16, 20), (30, 20))).unwrap();
    assert_eq!(p.gate, (17, 20), "the first land cell is the gate");
    assert_eq!(p.cells[0].cell, (16, 20), "the drawn water is part of the canal");
    assert_eq!(p.source, (16, 20));
    assert!(!p.through);
}

#[test]
fn the_bed_falls_from_the_intake_and_never_rises() {
    let mut s = pond_and_bank();
    // A dip and a rise along the route: the bed follows the dip down and does not climb back.
    for (x, g) in [(24, 4.9), (25, 4.8), (26, 5.6), (27, 5.8)] {
        *s.height.get_mut(x, 20).unwrap() = g;
    }
    let p = plan(&s, &route((16, 20), (30, 20))).unwrap();
    let level = 5.3;
    assert!((p.cells[1].target - (level - INTAKE_BELOW_LEVEL)).abs() < 1e-4);
    for w in p.cells[1..].windows(2) {
        assert!(w[1].target <= w[0].target - BED_FALL + 1e-4, "bed rises at {:?}", w[1].cell);
        assert!(w[1].target <= w[1].ground - MIN_COVER + 1e-4);
    }
    assert!(p.too_deep.is_empty());
}

#[test]
fn cuts_deeper_than_the_limit_are_refused_and_marked() {
    let mut s = pond_and_bank();
    *s.height.get_mut(25, 20).unwrap() = 10.0;
    let p = plan(&s, &route((16, 20), (30, 20))).unwrap();
    assert_eq!(p.too_deep, vec![(25, 20)]);
    assert!(place(&mut s, &route((16, 20), (30, 20))).is_err(), "the command refuses what the preview marks");
    assert!(s.buildings.buildings.iter().all(|b| b.kind != crate::buildings::BuildingKind::Canal), "nothing is placed on refusal");
}

#[test]
fn buildings_dams_and_length_limit_the_route() {
    let mut s = pond_and_bank();
    s.water.build_dam(&[(22, 20)], 1.0).unwrap();
    assert!(plan(&s, &route((16, 20), (30, 20))).is_err(), "a dam cell blocks the route");
    let s = pond_and_bank();
    let n = s.height.width();
    let mut long: Vec<Cell> = (16..n).map(|x| (x, 20)).collect();
    let up = (MAX_CELLS + 3).saturating_sub(long.len());
    long.extend((21..21 + up).map(|y| (n - 1, y)));
    assert!(long.len() > MAX_CELLS && long.iter().all(|c| c.1 < n));
    assert!(plan(&s, &long).unwrap_err().starts_with("Too long"));
}

#[test]
fn diggers_work_far_end_first_and_the_gate_cell_last() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    assert_eq!(s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Canal).count(), 1);
    assert_eq!(next_job(&s, id, &[], false), Some(((24, 20), false)));
    assert_eq!(next_job(&s, id, &[(24, 20)], false), Some(((23, 20), false)), "a claimed cell is skipped");
    let k = canal(&s, id).unwrap();
    let gate = k.gate.cell;
    let all_but_gate: Vec<Cell> = k.cells.iter().map(|c| c.cell).filter(|&c| c != gate).collect();
    assert_eq!(next_job(&s, id, &all_but_gate, false), Some((gate, false)));
}

#[test]
fn a_visit_digs_one_step_and_leaves_clay_on_the_bank() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let before = ground(&s, (24, 20));
    let clay_before: f32 = s.resources.items().iter().filter(|r| r.kind == ResourceKind::MudBank).map(|r| r.amount).sum();
    dig(&mut s, id, (24, 20)).unwrap();
    assert!((before - ground(&s, (24, 20)) - DIG_STEP).abs() < 1e-4);
    assert!(*s.water.canal.get(24, 20).unwrap());
    let clay: f32 = s.resources.items().iter().filter(|r| r.kind == ResourceKind::MudBank).map(|r| r.amount).sum();
    assert!((clay - clay_before - CLAY_PER_STEP).abs() < 1e-4);
    while cell_needs_work(&s, id, (24, 20), false) {
        dig(&mut s, id, (24, 20)).unwrap();
    }
    let c = canal(&s, id).unwrap().cells.iter().find(|c| c.cell == (24, 20)).copied().unwrap();
    assert!(c.done && (ground(&s, c.cell) - c.target).abs() < 1e-3, "digging stops at the target");
}

/// A finished Logistics building: canal gates can be built.
fn logistics(s: &mut SimState) {
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(901), kind: BuildingKind::Logistics, pos: (2, 40), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
}

/// Build the gate with an order and one visit, shut.
fn build_gate(s: &mut SimState, id: BuildingId) {
    let gate = canal(s, id).unwrap().gate.cell;
    s.apply(Command::SetGate { building: id, cell: gate, setting: GateSetting::Shut }).unwrap();
    turn_gate(s, id).unwrap();
}

#[test]
fn a_canal_has_no_gate_until_logistics_stands_and_one_is_ordered() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let gate = canal(&s, id).unwrap().gate.cell;
    dig(&mut s, id, gate).unwrap();
    assert!(!canal(&s, id).unwrap().gate.built, "digging the intake builds no gate");
    assert!(!*s.water.gate.get(gate.0, gate.1).unwrap(), "the canal runs free");
    assert!(s.apply(Command::SetGate { building: id, cell: gate, setting: GateSetting::Shut }).is_err(), "gates need Logistics");
    logistics(&mut s);
    s.apply(Command::SetGate { building: id, cell: gate, setting: GateSetting::Shut }).unwrap();
    assert_eq!(next_job(&s, id, &[], false), Some((gate, true)), "a builder goes to build it");
    assert!(turn_gate(&mut s, id).is_err(), "no sticks, no gate");
    stock_sticks(&mut s, 20.0);
    turn_gate(&mut s, id).unwrap();
    let k = canal(&s, id).unwrap();
    assert!(k.gate.built && k.gate.setting == GateSetting::Shut && k.gate.pending.is_none());
    let sill = ground(&s, gate) + s.water.dam_height.get(gate.0, gate.1).unwrap();
    assert!((sill - k.gate.top).abs() < 1e-4, "a shut gate stands at its top");
    assert!(k.gate.top >= 5.3 + GATE_FREEBOARD - 1e-4);
    assert!(*s.water.gate.get(gate.0, gate.1).unwrap());
}

#[test]
fn a_gate_change_waits_for_a_visit() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 20.0);
    logistics(&mut s);
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let gate = canal(&s, id).unwrap().gate.cell;
    dig(&mut s, id, gate).unwrap();
    build_gate(&mut s, id);
    s.apply(Command::SetGate { building: id, cell: gate, setting: GateSetting::Open }).unwrap();
    assert_eq!(canal(&s, id).unwrap().gate.setting, GateSetting::Shut, "nothing changes until a beaver visits");
    assert!(has_work(&s, id));
    assert_eq!(next_job(&s, id, &[], false), Some((gate, true)), "the gate visit comes first");
    turn_gate(&mut s, id).unwrap();
    let k = canal(&s, id).unwrap();
    assert_eq!(k.gate.setting, GateSetting::Open);
    assert_eq!(*s.water.dam_height.get(gate.0, gate.1).unwrap(), (k.gate.floor - ground(&s, gate)).max(0.0));
}

#[test]
fn silt_fills_an_unkept_canal_and_diggers_clear_a_kept_one() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (20, 20))).unwrap();
    let c = (20, 20);
    while cell_needs_work(&s, id, c, false) {
        dig(&mut s, id, c).unwrap();
    }
    let dug = ground(&s, c);
    for _ in 0..200 {
        silt(&mut s, id, 1.0);
    }
    assert!(ground(&s, c) > dug + 0.15, "200 still days of silt raise the bed");
    assert!(cell_needs_work(&s, id, c, true), "a silted cell needs clearing while maintained");
    assert!(!cell_needs_work(&s, id, c, false), "silt is maintainers' work, not builders'");
    set_maintain(&mut s, id, false).unwrap();
    assert!(!cell_needs_work(&s, id, c, true));
    for _ in 0..2000 {
        silt(&mut s, id, 1.0);
    }
    let k = canal(&s, id).unwrap().cells.iter().find(|k| k.cell == c).copied().unwrap();
    assert!((ground(&s, c) - k.ground).abs() < 1e-3, "an unkept canal fills back to its ground");
    assert!(!*s.water.canal.get(c.0, c.1).unwrap(), "and stops being a canal");
}

#[test]
fn grow_deepens_widens_and_extends() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let before = canal(&s, id).unwrap().cells.clone();
    grow(&mut s, id, CanalGrowth::Deepen, &[(22, 20)]).unwrap();
    let after = canal(&s, id).unwrap();
    let t = |cells: &[CanalCell], c: Cell| cells.iter().find(|k| k.cell == c).unwrap().target;
    assert!((t(&before, (22, 20)) - t(&after.cells, (22, 20)) - DIG_STEP).abs() < 1e-4);
    assert_eq!(t(&before, (21, 20)), t(&after.cells, (21, 20)), "only picked cells deepen");
    grow(&mut s, id, CanalGrowth::Widen, &[]).unwrap();
    assert_eq!(canal(&s, id).unwrap().width(), 2);
    assert!(grow(&mut s, id, CanalGrowth::Widen, &[]).is_err(), "two rows at most");
    let n = canal(&s, id).unwrap().route().count();
    grow(&mut s, id, CanalGrowth::Extend, &route((25, 20), (28, 20))).unwrap();
    assert_eq!(canal(&s, id).unwrap().route().count(), n + 4);
    assert!(grow(&mut s, id, CanalGrowth::Extend, &route((30, 25), (32, 25))).is_err(), "extend runs on from the end");
    assert!(growth_labour(&s, id, CanalGrowth::Deepen, &[]).unwrap() > 0.0);
}

#[test]
fn a_canal_can_be_cancelled_dug_or_not_and_its_ditch_stays() {
    let mut s = pond_and_bank();
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    s.apply(Command::CancelSite { building: id }).unwrap();
    assert!(s.buildings.building(id).is_none());
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let before = s.height.get(24, 20).copied().unwrap();
    dig(&mut s, id, (24, 20)).unwrap();
    let dug = s.height.get(24, 20).copied().unwrap();
    assert!(dug < before);
    s.apply(Command::CancelSite { building: id }).unwrap();
    assert!(s.buildings.building(id).is_none() && s.work.place(id).is_none(), "no canal and no site left");
    assert_eq!(s.height.get(24, 20).copied().unwrap(), dug, "the dug ditch stays");
}

#[test]
fn commands_round_trip_through_script_lines() {
    let cmds = [
        Command::PlaceCanal { cells: vec![(1, 2), (2, 2)] },
        Command::GrowCanal { canal: BuildingId(3), growth: CanalGrowth::Widen, cells: vec![(2, 2)] },
        Command::SetCanalMaintain { canal: BuildingId(3), on: false },
        Command::AddDamGate { dam: BuildingId(4), cell: (5, 6) },
        Command::SetGate { building: BuildingId(4), cell: (5, 6), setting: GateSetting::Level(6.5) },
        Command::SetGate { building: BuildingId(4), cell: (5, 6), setting: GateSetting::Open },
    ];
    for c in cmds {
        let line = crate::script::to_line(&c);
        assert_eq!(crate::script::parse_command(&line).unwrap(), c, "{line}");
    }
}

#[test]
fn a_dam_gate_is_built_by_a_visit_and_stops_wearing() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 20.0);
    let line: Vec<Cell> = (15..26).map(|y| (12, y)).collect();
    let dam = crate::dams::place(&mut s, &line).unwrap();
    s.water.build_dam(&line, 1.0).unwrap();
    assert!(crate::dams::plan_gate(&s, dam, (30, 30)).is_err());
    s.apply(Command::AddDamGate { dam, cell: (12, 20) }).unwrap();
    assert_eq!(crate::dams::gate_job(&s, dam, &[]), Some((12, 20)));
    crate::dams::work_gate(&mut s, dam, (12, 20)).unwrap();
    assert!(*s.water.gate.get(12, 20).unwrap());
    s.apply(Command::SetGate { building: dam, cell: (12, 20), setting: GateSetting::Level(5.5) }).unwrap();
    crate::dams::work_gate(&mut s, dam, (12, 20)).unwrap();
    assert!((*s.water.dam_height.get(12, 20).unwrap() - 0.5).abs() < 1e-4);
    assert!(!crate::dams::has_work(&s, dam) || crate::dams::gate_job(&s, dam, &[]).is_none());
}

#[test]
fn a_load_floats_at_full_pace_in_a_deep_canal() {
    let mut s = pond_and_bank();
    for x in 20..40 {
        s.water.dig_canal(&mut s.height, &[(x, 12)], 0.5).unwrap();
        *s.water.depth.get_mut(x, 12).unwrap() = 0.4;
    }
    let mut nav = crate::nav::NavGrid::new(s.height.width(), s.height.height());
    nav.rebuild(&s);
    assert!(nav.floats((25, 12)));
    assert!(!nav.floats((25, 13)));
}

// ---- Water: every engine, on the fixture valley ----

#[test]
fn a_canal_may_start_on_any_water_that_is_shown() {
    let mut s = pond_and_bank();
    // A thin sheet beside the pond is drawn as water, so a canal may start from it.
    let edge = (20, 20);
    *s.water.depth.get_mut(edge.0, edge.1).unwrap() = 0.01;
    *s.water.depth.get_mut(edge.0 - 1, edge.1).unwrap() = 0.01;
    assert!(s.water.shown(edge.0 as i32, edge.1 as i32));
    assert!(is_water(&s, edge));
    assert!(plan(&s, &route(edge, (30, 20))).is_ok());
    // A damp film on its own is not drawn, and is not water to start from.
    let lone = (40, 40);
    *s.water.depth.get_mut(lone.0, lone.1).unwrap() = 0.01;
    assert!(!s.water.shown(lone.0 as i32, lone.1 as i32));
    assert_eq!(plan(&s, &route(lone, (48, 40))).unwrap_err(), "Must start at water");
}

#[test]
fn a_shut_gate_closes_the_whole_width_of_a_widened_canal() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 20.0);
    logistics(&mut s);
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    build_gate(&mut s, id);
    let gate = canal(&s, id).unwrap().gate.cell;
    while cell_needs_work(&s, id, gate, false) {
        dig(&mut s, id, gate).unwrap();
    }
    grow(&mut s, id, CanalGrowth::Widen, &[]).unwrap();
    let across = gate_cells(canal(&s, id).unwrap());
    assert_eq!(across.len(), 2, "the gate and the widened cell beside it: {across:?}");
    let beside = across[1];
    while cell_needs_work(&s, id, beside, false) {
        dig(&mut s, id, beside).unwrap();
    }
    let top = canal(&s, id).unwrap().gate.top;
    for c in across {
        assert!(*s.water.gate.get(c.0, c.1).unwrap(), "{c:?} is gated");
        let sill = ground(&s, c) + s.water.dam_height.get(c.0, c.1).unwrap();
        assert!((sill - top).abs() < 1e-4, "{c:?} stands shut at the top");
    }
}

#[test]
fn a_gate_is_upgraded_into_a_lock_by_a_visit_once_a_harbor_stands() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 40.0);
    s.buildings.stores.deposit(crate::buildings::Good::Clay, 20.0).unwrap();
    let line: Vec<Cell> = (15..26).map(|y| (12, y)).collect();
    let dam = crate::dams::place(&mut s, &line).unwrap();
    s.water.build_dam(&line, 1.0).unwrap();
    s.apply(Command::AddDamGate { dam, cell: (12, 20) }).unwrap();
    let lock = Command::UpgradeLock { building: dam, cell: (12, 20) };
    assert!(s.apply(lock.clone()).is_err(), "no harbor yet");
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(902), kind: BuildingKind::Harbor, pos: (2, 44), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
    crate::dams::work_gate(&mut s, dam, (12, 20)).unwrap();
    s.apply(lock.clone()).unwrap();
    assert!(crate::locks::pending(&s, (12, 20)) && !crate::locks::is_lock(&s, (12, 20)));
    assert_eq!(crate::dams::gate_job(&s, dam, &[]), Some((12, 20)), "a builder goes to build the lock");
    let sill = *s.water.dam_height.get(12, 20).unwrap();
    crate::dams::work_gate(&mut s, dam, (12, 20)).unwrap();
    assert!(crate::locks::is_lock(&s, (12, 20)));
    assert_eq!(*s.water.dam_height.get(12, 20).unwrap(), sill, "the lock holds the water as the gate did");
    assert_eq!(crate::dams::gate_job(&s, dam, &[]), None);
    assert!(s.apply(lock).is_err(), "already a lock");
    assert_eq!(crate::script::parse_command(&crate::script::to_line(&Command::UpgradeLock { building: dam, cell: (12, 20) })).unwrap(), Command::UpgradeLock { building: dam, cell: (12, 20) });
}

#[test]
fn a_lock_goes_straight_onto_a_dam_cell_gate_and_all_in_one_visit() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 40.0);
    s.buildings.stores.deposit(crate::buildings::Good::Clay, 20.0).unwrap();
    let line: Vec<Cell> = (15..26).map(|y| (12, y)).collect();
    let dam = crate::dams::place(&mut s, &line).unwrap();
    s.water.build_dam(&line, 1.0).unwrap();
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(902), kind: BuildingKind::Harbor, pos: (2, 44), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
    s.apply(Command::UpgradeLock { building: dam, cell: (12, 20) }).unwrap();
    assert!(crate::dams::dam(&s, dam).unwrap().gates.iter().any(|g| g.cell == (12, 20) && !g.built), "the cell is to be a gate");
    assert_eq!(crate::dams::gate_job(&s, dam, &[]), Some((12, 20)), "a builder goes to build it");
    crate::dams::work_gate(&mut s, dam, (12, 20)).unwrap();
    assert!(crate::locks::is_lock(&s, (12, 20)), "gate and lock built in the one visit");
    assert_eq!(crate::dams::gate_job(&s, dam, &[]), None);
}

/// A lock ordered on a built gate is builders' work, and the visit that builds it is offered.
#[test]
fn a_lock_ordered_on_a_built_gate_is_offered_to_builders() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 40.0);
    s.buildings.stores.put_at(crate::agents::SUPPLIES, Good::Clay, 40.0).unwrap();
    logistics(&mut s);
    let id = place(&mut s, &route((16, 20), (24, 20))).unwrap();
    let gate = canal(&s, id).unwrap().gate.cell;
    for c in canal(&s, id).unwrap().cells.iter().map(|c| c.cell).collect::<Vec<_>>() {
        while cell_needs_work(&s, id, c, false) {
            dig(&mut s, id, c).unwrap();
        }
    }
    build_gate(&mut s, id);
    assert!(!has_work_for(&s, id, false), "dug, gate built: nothing for builders");
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(902), kind: BuildingKind::Harbor, pos: (2, 45), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
    crate::locks::order(&mut s, id, gate).unwrap();
    assert!(has_work_for(&s, id, false));
    assert_eq!(next_job(&s, id, &[], false), Some((gate, true)), "the lock is built on a gate visit");
    turn_gate(&mut s, id).unwrap();
    assert!(!crate::locks::pending(&s, gate) && !has_work_for(&s, id, false));
}

/// A lock needs its chamber and ways in clear, and keeps them clear once ordered.
#[test]
fn a_lock_needs_room_and_keeps_it() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 40.0);
    s.buildings.stores.put_at(crate::agents::SUPPLIES, Good::Clay, 40.0).unwrap();
    let line: Vec<Cell> = (15..26).map(|y| (12, y)).collect();
    let dam = crate::dams::place(&mut s, &line).unwrap();
    s.water.build_dam(&line, 1.0).unwrap();
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(902), kind: BuildingKind::Harbor, pos: (2, 44), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
    let room = crate::locks::footprint(&s, &crate::locks::Lock { cell: (12, 20), building: dam, built: false });
    assert!(room.contains(&(9, 20)) && room.contains(&(15, 20)) && room.contains(&(14, 21)), "the chamber and its ways in either side: {room:?}");
    assert!(!room.contains(&(12, 23)), "not along the dam beyond the walls");
    // Something standing in the way in: no lock.
    let shed = crate::buildings::Building { id: BuildingId(903), kind: BuildingKind::Harbor, pos: (15, 19), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
    s.buildings.buildings.push(shed);
    assert!(s.apply(Command::UpgradeLock { building: dam, cell: (12, 20) }).is_err());
    assert!(crate::dams::dam(&s, dam).unwrap().gates.is_empty(), "a refused lock leaves no gate behind");
    s.buildings.buildings.retain(|b| b.id != BuildingId(903));
    s.apply(Command::UpgradeLock { building: dam, cell: (12, 20) }).unwrap();
    // Ordered, its ground is kept clear of buildings, other dams and canals.
    logistics(&mut s);
    let clear = s.check_site(BuildingKind::Harbor, (14, 23));
    assert!(clear.is_ok(), "away from the lock: {clear:?}");
    assert_eq!(s.check_site(BuildingKind::Harbor, (15, 19)), Err(crate::buildings::BuildError::Blocked));
    assert!(crate::dams::check_line(&s, &[(14, 18), (14, 19), (14, 20)]).is_err());
    assert!(plan(&s, &route((10, 20), (30, 20))).is_err());
}

/// A built lock is swum through only down its lane, from gate to gate: its walls are solid.
#[test]
fn a_built_lock_is_passed_down_its_lane_and_not_through_its_walls() {
    let mut s = pond_and_bank();
    stock_sticks(&mut s, 40.0);
    s.buildings.stores.put_at(crate::agents::SUPPLIES, Good::Clay, 40.0).unwrap();
    let line: Vec<Cell> = (15..26).map(|y| (12, y)).collect();
    let dam = crate::dams::place(&mut s, &line).unwrap();
    s.water.build_dam(&line, 1.0).unwrap();
    s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(902), kind: BuildingKind::Harbor, pos: (2, 44), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
    s.apply(Command::UpgradeLock { building: dam, cell: (12, 20) }).unwrap();
    s.locks.at.get_mut(&(12, 20)).unwrap().built = true;
    s.nav.rebuild(&s.clone());
    let l = s.locks.at[&(12, 20)];
    let lane = crate::locks::lock_cells(&s, &l);
    assert!(lane.iter().all(|c| c.1 == 20), "one row straight across the dam: {lane:?}");
    assert!(lane.contains(&(10, 20)) && lane.contains(&(14, 20)), "{lane:?}");
    for c in [(12, 19), (12, 21), (10, 21), (14, 19)] {
        assert_eq!(s.nav.surface_at(c), Some(crate::nav::Surface::Solid), "wall at {c:?}");
    }
    assert_eq!(s.nav.surface_at((12, 20)), Some(crate::nav::Surface::Lock));
}
