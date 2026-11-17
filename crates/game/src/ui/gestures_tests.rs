//! Gesture tests: pointer gestures to sim commands, and a headless run of the resulting orders.
use super::*;
use crate::ui::orders::gesture_command;
use sim::buildings::BuildingKind;
use sim::colony::Stage;

fn start2() -> SimState {
    let mut sim = sim::lakes::early_lake(1).expect("early lake fixture").0;
    sim.work.auto_organise = false; // only the player orders move beavers
    for p in &mut sim.work.places {
        p.slots.iter_mut().for_each(|s| s.worker = None); // the start staffs its camps; the player gives the orders here
    }
    sim.agents.by_id.values_mut().for_each(|a| a.assignment = None);
    sim
}

fn adults(sim: &SimState, n: usize) -> Vec<BeaverId> {
    sim.colony.beavers.iter().filter(|b| b.stage == Stage::Adult).map(|b| b.id).take(n).collect()
}

/// The cell of the first resource of `kind` that still has something to gather.
fn resource_cell(sim: &SimState, kind: ResourceKind) -> Option<(Cell, ResourceKind)> {
    sim.resources.items.iter().find(|r| same_kind(r.kind, kind) && r.has_yield()).map(|r| (r.cell, r.kind))
}

/// A free spot where a site of `kind` can be placed, scanning outwards from the first beaver.
fn place_near_camp(sim: &mut SimState, kind: BuildingKind) -> BuildingId {
    let c = sim.colony.beavers[0].pos;
    let (cx, cy) = (c.0 as usize, c.1 as usize);
    for r in 3..30usize {
        for (x, y) in (cx.saturating_sub(r)..=cx + r).flat_map(|x| (cy.saturating_sub(r)..=cy + r).map(move |y| (x, y))) {
            if let Ok(id) = sim.place_site(kind, (x, y)) {
                return id;
            }
        }
    }
    panic!("no room for a {kind:?}");
}

#[test]
fn right_click_issues_collect_for_each_resource_kind_and_builder_for_sites() {
    let mut sim = start2();
    let who = adults(&sim, 2);
    assert_eq!(who.len(), 2, "start 2 has adults");
    let mp = MapPointer::default();
    let mut seen = 0;
    for kind in [ResourceKind::Tree, ResourceKind::Plant, ResourceKind::MudBank, ResourceKind::StoneOutcrop] {
        let Some((cell, real)) = resource_cell(&sim, kind) else { continue };
        seen += 1;
        let id = sim.resources.at(cell).find(|r| r.kind == real && r.has_yield()).unwrap().id;
        let want = Command::Assign { who: who.clone(), to: Some(Assignment::Resource(id)) };
        let g = Gesture::RightClickResource { kind, cell };
        assert_eq!(gesture_command(&sim, &who, &g), Some(want.clone()), "{kind:?} gesture");
        assert_eq!(mp.right_click(&sim, &who, cell), vec![want], "{kind:?} right-click on {cell:?}");
    }
    assert!(seen >= 2, "start 2 shows several resource kinds, saw {seen}");
    let site = place_near_camp(&mut sim, BuildingKind::LoggingCamp);
    let pos = sim.buildings.building(site).unwrap().pos;
    let cmds = mp.right_click(&sim, &who, pos);
    assert_eq!(
        cmds,
        vec![
            Command::FillSlot { building: site, slot: 0, who: who[0] },
            Command::FillSlot { building: site, slot: 1, who: who[1] },
        ],
        "two selected beavers take the two builder slots"
    );
    assert_eq!(gesture_command(&sim, &who, &Gesture::RightClickSite { building: site }), Some(cmds[0].clone()));
    assert!(mp.right_click(&sim, &[], pos).is_empty(), "nothing selected, nothing ordered");
}

#[test]
fn dragging_a_rectangle_issues_area_collect() {
    let sim = start2();
    let who = adults(&sim, 1);
    let (cell, _) = resource_cell(&sim, ResourceKind::Tree).expect("a tree");
    let (a, b) = ((cell.0.saturating_sub(3), cell.1.saturating_sub(3)), (cell.0 + 3, cell.1 + 3));
    let mut mp = MapPointer::default();
    let covers_area = work_area_hit(&sim, a).is_some();
    assert_eq!(mp.left_down(&sim, true, a), true);
    mp.moved((a.0 + 2, a.1 + 2));
    assert!(mp.preview(&sim).is_some(), "a rectangle is drawn while dragging");
    let cmds = mp.left_up(&sim, &who, b);
    if !covers_area {
        let area = area_between(a, b);
        let kind = dominant_kind(&sim, &area).unwrap();
        assert_eq!(cmds, vec![Command::Assign { who: who.clone(), to: Some(Assignment::Area(kind, area)) }]);
        assert_eq!(gesture_command(&sim, &who, &Gesture::DragArea { kind, area }), Some(cmds[0].clone()));
    }
    // a click without movement is no order
    let mut mp = MapPointer::default();
    mp.left_down(&sim, true, a);
    assert!(mp.left_up(&sim, &who, a).is_empty());
}

#[test]
fn dragging_a_work_area_moves_it_and_its_edge_resizes_it() {
    let sim = start2();
    let (building, area) = sim.work.places.iter().find_map(|p| p.area.map(|a| (p.building, a))).expect("start 2 has a camp with a work area");
    let size = sim.world.size;
    let centre = area.centre();
    let hit = work_area_hit(&sim, centre).unwrap();
    assert_eq!(hit.1, Handle::Move, "inside the rectangle moves it");
    let building = hit.0;
    let area = sim.work.place(building).unwrap().area.unwrap();
    let mut mp = MapPointer::default();
    assert!(mp.left_down(&sim, false, centre), "grabbing an area captures the drag even with nobody selected");
    let cmds = mp.left_up(&sim, &[], (centre.0 + 3, centre.1 + 2));
    let moved = Area { min: (area.min.0 + 3, area.min.1 + 2), max: (area.max.0 + 3, area.max.1 + 2) };
    assert_eq!(cmds, vec![Command::SetWorkArea { building, area: moved }]);
    // east edge resizes
    let edge = (area.max.0, area.centre().1);
    let hit = work_area_hit(&sim, edge).unwrap();
    assert_eq!(hit, (building, Handle::Edge { west: false, east: true, north: false, south: false }));
    mp.left_down(&sim, false, edge);
    mp.moved((edge.0 + 4, edge.1 + 7)); // the y wobble must not matter for an east edge
    let cmds = mp.left_up(&sim, &[], (edge.0 + 4, edge.1 + 7));
    let grown = Area { min: area.min, max: (area.max.0 + 4, area.max.1) };
    assert_eq!(cmds, vec![Command::SetWorkArea { building, area: grown }]);
    // a corner resizes two sides, never inverts and never leaves the map
    let h = Handle::Edge { west: true, east: false, north: true, south: false };
    let tiny = drag_work_area(area, h, area.min, (area.max.0 + 50, area.max.1 + 50), size);
    assert_eq!(tiny, Area { min: area.max, max: area.max });
    assert_eq!(drag_work_area(area, Handle::Move, centre, (0, 0), size).min, (0, 0));
    assert_eq!(drag_work_area(area, Handle::Move, centre, (size + 9, size + 9), size).max, (size - 1, size - 1));
    // a drag that ends where it began issues nothing
    mp.left_down(&sim, false, centre);
    assert!(mp.left_up(&sim, &[], centre).is_empty());
    // applied for real
    let mut sim = sim;
    for c in cmds {
        sim.apply(c).unwrap();
    }
    assert_eq!(sim.work.place(building).unwrap().area, Some(grown));
}

/// Right-click on a site: the selected beaver takes a builder slot and delivers goods to it.
#[test]
fn after_a_right_click_on_a_site_the_beaver_delivers_to_it() {
    let mut sim = start2();
    let site = place_near_camp(&mut sim, BuildingKind::LoggingCamp);
    // the second adult: the first one starts boxed in on its cell in this seed and never moves (a sim quirk, not an order problem)
    let who = vec![adults(&sim, 2)[1]];
    let pos = sim.buildings.building(site).unwrap().pos;
    let delivered = |s: &SimState| s.work.place(site).and_then(|p| p.site.as_ref()).map_or(0.0, |p| p.delivered.iter().map(|a| a.1).sum::<f32>());
    let before = delivered(&sim);
    for c in MapPointer::default().right_click(&sim, &who, pos) {
        sim.apply(c).unwrap();
    }
    assert_eq!(sim.work.slot_of(who[0]).map(|s| s.0), Some(site));
    for _ in 0..1500 {
        sim.step(0.05).unwrap();
        if delivered(&sim) > before || sim.work.place(site).is_none_or(|p| p.site.is_none()) {
            break;
        }
    }
    let done = sim.work.place(site).is_none_or(|p| p.site.is_none());
    assert!(done || delivered(&sim) > before, "the builder delivered materials to the site");
}
