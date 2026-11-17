use super::*;
use crate::build::BuildUi;
use crate::ui::explain::audit;
use sim::starts::{build, Start};
use sim::Command;

fn start2() -> SimState {
    build(Start::EarlyLake, 1).expect("start 2")
}

fn legend(o: &Overlay, label: &str) -> Explained {
    o.legend.iter().find(|r| r.num.label == label).unwrap_or_else(|| panic!("no legend number '{label}'")).num.clone()
}

/// A dry cell with nothing on it, away from the camp.
fn bare_cell(sim: &SimState, skip: usize) -> Cell {
    let n = sim.world.size;
    let mut seen = 0;
    for y in n / 3..n {
        for x in n / 3..n {
            let dry = sim.water.depth.get(x, y).copied().unwrap_or(1.0) < 0.001;
            if dry && sim.resources.at((x, y)).next().is_none() && sim.buildings.buildings.iter().all(|b| b.cells().all(|c| c != (x, y))) {
                if seen == skip {
                    return (x, y);
                }
                seen += 1;
            }
        }
    }
    panic!("no bare cell");
}

fn wet_cell(sim: &SimState) -> Cell {
    let n = sim.world.size;
    (0..n * n).map(|i| (i % n, i / n)).find(|&c| sim::resources::is_wet(&sim.world, c) && sim.resources.at(c).next().is_none()).expect("wet cell")
}

#[test]
fn water_overlay_follows_depth_and_flow() {
    let mut sim = start2();
    let a = overlay(&sim, MapMode::Water);
    assert!(a.tints.len() > 50, "the lake is painted");
    let cells = legend(&a, "Water cells");
    assert_eq!(cells.value as usize, a.tints.len(), "the number counts the painted cells");
    assert!(!a.arrows.is_empty(), "the river flows, so there are arrows");
    assert!(a.arrows.iter().all(|r| (r.dir.0.hypot(r.dir.1) - 1.0).abs() < 1e-3));
    // more water on a dry cell: one more painted cell, more volume
    let dry = bare_cell(&sim, 0);
    *sim.water.depth.get_mut(dry.0, dry.1).unwrap() = 1.4;
    let b = overlay(&sim, MapMode::Water);
    assert_eq!(b.tints.len(), a.tints.len() + 1);
    assert_eq!(legend(&b, "Water cells").value, cells.value + 1.0);
    assert!(legend(&b, "Water held").value > legend(&a, "Water held").value);
    assert!(legend(&b, "Deepest water").value >= 1.4);
    // the current changes when the water moves
    sim.step(0.5).unwrap();
    let c = overlay(&sim, MapMode::Water);
    assert_ne!(c.arrows, b.arrows, "arrows follow the flow");
    // dams show as marks and in the dam count
    let dams = legend(&c, "Dam cells").value as usize;
    assert_eq!(dams, c.marks.iter().filter(|m| m.kind == MarkKind::Dam).count());
    assert!(dams > 0, "start 2 founds a dam");
}

#[test]
fn forest_overlay_follows_trees_and_felling() {
    let mut sim = start2();
    let a = overlay(&sim, MapMode::Forest);
    let standing = legend(&a, "Standing trees").value;
    assert!(standing > 10.0);
    // a new tree: one more standing tree and one more tint
    let cell = bare_cell(&sim, 3);
    let id = sim.resources.add(ResourceKind::Tree, cell, sim::resources::FELL_WORK);
    let b = overlay(&sim, MapMode::Forest);
    assert_eq!(legend(&b, "Standing trees").value, standing + 1.0);
    assert_eq!(b.tints.len(), a.tints.len() + 1);
    // marking ground for felling: its trees turn from "Not marked" to "Marked for felling"
    let who = crew(&sim, 1);
    let area = Area { min: (cell.0.saturating_sub(1), cell.1.saturating_sub(1)), max: (cell.0 + 1, cell.1 + 1) };
    sim.apply(Command::Assign { who, to: Some(Assignment::Area(ResourceKind::Tree, area)) }).unwrap();
    let c = overlay(&sim, MapMode::Forest);
    let marked = |o: &Overlay| legend(o, "Standing trees").parts[0].value;
    assert!(marked(&c) > marked(&b), "the order marks the tree");
    assert!(!c.outlines.is_empty());
    assert!(legend(&c, "Fellers at work").value >= 1.0);
    // felling it: one fewer standing, one more felled tree with logs
    sim.resources.fell(id, 100.0).unwrap();
    let d = overlay(&sim, MapMode::Forest);
    assert_eq!(legend(&d, "Standing trees").value, standing);
    assert_eq!(legend(&d, "Felled trees").value, legend(&c, "Felled trees").value + 1.0);
    assert!(legend(&d, "Logs lying").value > legend(&c, "Logs lying").value);
    assert!(d.marks.iter().any(|m| m.kind == MarkKind::Felled && m.cell == cell));
}

#[test]
fn food_overlay_follows_plants_and_bark() {
    let mut sim = start2();
    let a = overlay(&sim, MapMode::Food);
    let cell = wet_cell(&sim);
    let world = sim.world.clone();
    let p = sim.resources.add_plant(&world, cell).unwrap();
    let b = overlay(&sim, MapMode::Food);
    assert_eq!(legend(&b, "Plants").value, legend(&a, "Plants").value + 1.0);
    assert!(legend(&b, "Food on plants").value > legend(&a, "Food on plants").value);
    // eating from the plant lowers its stock and fades its tint
    sim.resources.take(p, 2.0).unwrap();
    let c = overlay(&sim, MapMode::Food);
    assert!(legend(&c, "Food on plants").value < legend(&b, "Food on plants").value);
    assert_ne!(b.tints, c.tints);
    // a felled tree carries bark until it is stripped
    let t = sim.resources.add(ResourceKind::Tree, bare_cell(&sim, 5), sim::resources::FELL_WORK);
    sim.resources.fell(t, 100.0).unwrap();
    let d = overlay(&sim, MapMode::Food);
    assert!(legend(&d, "Bark waiting").value >= sim::resources::TREE_BARK);
    assert!(d.marks.iter().any(|m| m.kind == MarkKind::Bark));
    sim.resources.strip(t, 100.0).unwrap();
    let e = overlay(&sim, MapMode::Food);
    assert!(legend(&e, "Bark waiting").value < legend(&d, "Bark waiting").value);
    // food in the larder
    sim.colony.food += 7.0;
    assert_eq!(legend(&overlay(&sim, MapMode::Food), "Larder").value, legend(&e, "Larder").value + 7.0);
}

#[test]
fn work_overlay_follows_areas_and_workers() {
    let mut sim = start2();
    let a = overlay(&sim, MapMode::Work);
    assert!(!a.outlines.is_empty(), "start 2 has work areas");
    let (b, area) = sim.work.places.iter().find_map(|p| p.area.map(|x| (p.building, x))).unwrap();
    // moving the area moves its outline and tint
    let moved = Area { min: (area.min.0 + 2, area.min.1), max: (area.max.0 + 2, area.max.1) };
    sim.apply(Command::SetWorkArea { building: b, area: moved }).unwrap();
    let m = overlay(&sim, MapMode::Work);
    assert_ne!(a.outlines, m.outlines);
    assert!(m.outlines.iter().any(|o| o.area == moved));
    // a free slot filled by a beaver: one fewer open, one more filled, a worker mark appears
    let (pb, slot) = sim.work.places.iter().find_map(|p| p.slots.iter().position(|s| s.worker.is_none()).map(|i| (p.building, i))).expect("an open slot");
    let open = |o: &Overlay| legend(o, "Slots open").value;
    let filled = |o: &Overlay| legend(o, "Slots filled").value;
    let who = crew(&sim, 1)[0];
    sim.apply(Command::FillSlot { building: pb, slot, who }).unwrap();
    let f = overlay(&sim, MapMode::Work);
    assert_eq!(open(&f), open(&m) - 1.0);
    assert_eq!(filled(&f), filled(&m) + 1.0);
    let workers = |o: &Overlay| o.marks.iter().filter(|k| k.kind == MarkKind::Worker).count();
    assert!(workers(&f) > workers(&m));
}

fn ready() -> (SimState, Shell, Option<Selection>, BuildUi) {
    (start2(), Shell::default(), None, BuildUi::default())
}

#[test]
fn every_action_has_an_icon_and_every_number_explains_itself() {
    let sim = start2();
    let dir = crate::ui::icons::asset_root();
    let has = |i: Icon| dir.join(i.file()).is_file();
    for a in ModeAction::ALL {
        assert!(has(a.icon()), "{a:?} icon file");
        assert!(!a.hint().is_empty() && !a.label().is_empty());
    }
    for m in MapMode::ALL {
        assert!(has(m.icon()));
    }
    let forbidden = ["flood", "drought", "predator", "guard", "rival", "trade", "scout", "raid"];
    let mut texts: Vec<String> = Vec::new();
    for m in MapMode::PAINTED {
        let o = overlay(&sim, m);
        assert!(!o.legend.is_empty(), "{m:?} has numbers");
        let problems = audit(&o);
        assert!(problems.is_empty(), "{m:?}: {problems:?}");
        for r in &o.legend {
            assert!(r.icon.is_some_and(has), "{} icon", r.num.label);
            assert!(!r.num.parts.is_empty() && r.num.sums(), "{} explains itself", r.num.label);
            texts.push(r.num.label.clone());
            texts.extend(r.num.parts.iter().map(|p| p.label.clone()));
        }
        texts.push(m.label().into());
        texts.push(m.hint().into());
        for a in m.actions() {
            texts.push(a.label().into());
            texts.push(a.hint().into());
        }
    }
    for t in &texts {
        let l = t.to_lowercase();
        assert!(!forbidden.iter().any(|f| l.contains(f)), "'{t}' names a disabled layer");
    }
    // the plain map paints nothing
    assert!(overlay(&sim, MapMode::Normal).legend.is_empty());
}
