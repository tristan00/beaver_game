//! Settle choice tests: a click picks usable ground, the card's button sends `Command::Settle`,
//! an unusable cell sends nothing.
use super::*;
use crate::flow::Flow;

fn opening() -> (SimState, Flow, sim::starts::DamSite) {
    let (sim, dam) = sim::starts::new_game_with_dam(1).expect("new game");
    let mut flow = Flow::new(&sim.world);
    flow.fog.reveal_valley(&sim.world, sim.world.start);
    (sim, flow, dam)
}

fn river_cell(sim: &SimState) -> Cell {
    (0..sim.world.size * sim.world.size)
        .map(|i| (i % sim.world.size, i / sim.world.size))
        .find(|&(x, y)| sim.world.river.get(x, y) == Some(&true) && sim.world.valley_of((x, y)) == Some(sim.world.start))
        .expect("a river cell")
}

#[test]
fn the_game_opens_with_three_usable_banks_far_apart_and_nobody_founded() {
    let (sim, flow, _) = opening();
    assert!(!sim.colony.is_founded());
    let c = candidates(&sim, &flow.fog);
    assert_eq!(c.len(), 3, "{c:?}");
    for cand in &c {
        assert!(usable(&sim, &flow.fog, cand.cell).is_ok(), "{cand:?}");
    }
    for (i, a) in c.iter().enumerate() {
        for b in &c[i + 1..] {
            assert_ne!(a.name, b.name);
            let d = ((a.cell.0 as f32 - b.cell.0 as f32).powi(2) + (a.cell.1 as f32 - b.cell.1 as f32).powi(2)).sqrt();
            assert!(d >= SPACING, "{a:?} and {b:?} are {d} apart");
        }
    }
}

#[test]
fn picking_a_usable_cell_and_confirming_sends_the_settle_command() {
    let (mut sim, flow, _) = opening();
    let cell = candidates(&sim, &flow.fog)[0].cell;
    let mut ui = SettleUi::default();
    assert!(ui.click(&sim, &flow.fog, cell));
    assert_eq!(ui.chosen, Some(cell));
    let cmd = ui.confirm(&sim, &flow.fog).expect("a command");
    assert_eq!(cmd, Command::Settle { cell });
    assert_eq!(ui.chosen, None, "the pick is spent");
    sim.apply(cmd).expect("the sim accepts it");
    assert!(sim.colony.is_founded());
}

#[test]
fn an_unusable_cell_sends_no_command_and_says_why() {
    let (sim, flow, _) = opening();
    let mut ui = SettleUi::default();
    for bad in [river_cell(&sim), (sim.world.size + 5, 0)] {
        assert!(!ui.click(&sim, &flow.fog, bad), "{bad:?}");
        assert_eq!(ui.chosen, None);
        assert!(!ui.message.is_empty());
        assert_eq!(ui.confirm(&sim, &flow.fog), None);
    }
    // a bad click after a good pick keeps the pick
    let good = candidates(&sim, &flow.fog)[0].cell;
    ui.click(&sim, &flow.fog, good);
    ui.click(&sim, &flow.fog, river_cell(&sim));
    assert_eq!(ui.chosen, Some(good));
}

#[test]
fn confirming_with_nothing_picked_sends_nothing_and_cancel_drops_the_pick() {
    let (sim, flow, _) = opening();
    let mut ui = SettleUi::default();
    assert_eq!(ui.confirm(&sim, &flow.fog), None);
    ui.click(&sim, &flow.fog, candidates(&sim, &flow.fog)[1].cell);
    ui.cancel();
    assert_eq!(ui.chosen, None);
    assert_eq!(ui.confirm(&sim, &flow.fog), None);
}

#[test]
fn once_settled_no_cell_is_usable_again() {
    let (mut sim, flow, dam) = opening();
    let first = candidates(&sim, &flow.fog)[0].cell;
    sim.apply(Command::Settle { cell: dam.camp }).unwrap();
    let mut ui = SettleUi::default();
    assert!(!ui.click(&sim, &flow.fog, first));
    assert_eq!(ui.confirm(&sim, &flow.fog), None);
}

#[test]
fn facts_are_unit_fractions() {
    let (sim, flow, _) = opening();
    for c in candidates(&sim, &flow.fog) {
        let f = facts(&sim, c.cell);
        for v in [f.wood, f.forage, f.flood] {
            assert!((0.0..=1.0).contains(&v), "{c:?} {f:?}");
        }
    }
}

/// Job assignment from the panel: + puts an adult on the job and the panel counts show it at once;
/// a beaver's own job button does the same for that beaver.
#[test]
fn the_job_panel_buttons_assign_beavers_and_the_counts_follow() {
    use crate::ui::hud::job_panel;
    use crate::ui::panels::{apply, ShellUi};
    use sim::colony::Job;
    let (sim, dam) = sim::starts::new_game_with_dam(1).unwrap();
    let mut sim = Sim(sim);
    sim.0.apply(Command::Settle { cell: dam.camp }).unwrap();
    let mut flow = Flow::new(&sim.0.world);
    let mut sel = crate::flow::Selection::None;
    let mut ui = ShellUi::default();
    let mut build = crate::build::BuildUi::default();
    let mut reset = false;
    let line = |s: &SimState, j: Job| job_panel(s, None).lines.iter().find(|l| l.job == j).map(|l| l.assigned.value as usize).unwrap();
    assert_eq!(line(&sim.0, Job::Logger), 0, "nobody has a job before the player assigns one");
    apply(Do::Staff(Job::Logger, 1), &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset);
    assert_eq!(line(&sim.0, Job::Logger), 1);
    apply(Do::Staff(Job::Logger, -1), &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset);
    assert_eq!(line(&sim.0, Job::Logger), 0);
    let who = sim.0.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult).unwrap().id;
    apply(Do::SetJob(who, Some(Job::Forager)), &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset);
    assert_eq!(line(&sim.0, Job::Forager), 1);
    assert_eq!(sim.0.colony.beaver(who).unwrap().job, Some(Job::Forager));
    let p = job_panel(&sim.0, Some(who));
    assert_eq!(p.selected.unwrap().set, Some(Job::Forager));
}

/// Drag gather with nobody selected: the idle adults take the area.
#[test]
fn a_drag_over_trees_with_a_gather_tool_armed_sends_idle_adults() {
    let (sim, dam) = sim::starts::new_game_with_dam(1).unwrap();
    let mut sim = sim;
    sim.apply(Command::Settle { cell: dam.camp }).unwrap();
    let who = crate::ui::mapmodes::crew(&sim, crate::ui::mapmodes::CREW);
    assert!(!who.is_empty());
    let tree = sim.resources.items.iter().find(|r| r.kind == sim::resources::ResourceKind::Tree).unwrap().cell;
    let (a, b) = ((tree.0.saturating_sub(3), tree.1.saturating_sub(3)), (tree.0 + 3, tree.1 + 3));
    let mut mp = crate::ui::gestures::MapPointer::default();
    mp.armed = Some(sim::resources::ResourceKind::Tree);
    assert!(mp.left_down(&sim, true, a));
    mp.moved((a.0 + 2, a.1 + 2));
    assert!(mp.preview(&sim).is_some(), "the rectangle is drawn while dragging");
    let cmds = mp.left_up(&sim, &who, b);
    assert_eq!(cmds.len(), 1);
    for c in cmds {
        sim.apply(c).unwrap();
    }
    assert!(who.iter().all(|id| sim.agents.get(*id).is_some_and(|a| a.assignment.is_some())));
    // Esc / right click before release cancels: no command
    mp.armed = Some(sim::resources::ResourceKind::Tree);
    mp.left_down(&sim, true, a);
    mp.cancel();
    assert!(mp.left_up(&sim, &who, b).is_empty());
}
