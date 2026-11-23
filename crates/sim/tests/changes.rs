//! The simulation says what changed -- grid tiles by their stamps, sources by the list of
//! changes -- so that whoever draws the world need not compare all of it. Here a played colony
//! is compared in full after every step: whatever differs from what was last reported, by
//! more than is drawn, must have been reported.
use sim::autoplay::{Player, Style};
use sim::resources::{Look, ResourceId};
use sim::{Command, Grid, SimState, STEP_DAYS};
use std::collections::{BTreeMap, HashSet};

const TILE: usize = sim::grid::TILE;
const REACH: usize = sim::grid::REACH;

/// What someone drawing a grid tile by tile would be showing: for each tile, its stamp and the
/// cells it reads (its own and those within reach) as they were when the stamp last changed.
struct Shown<T> {
    name: &'static str,
    stamps: Vec<u64>,
    cells: Vec<Vec<T>>,
    origin: u64,
    /// How many times a tile was redrawn.
    redrawn: usize,
}

fn tiles<T>(g: &Grid<T>) -> (usize, usize) {
    (g.width().div_ceil(TILE), g.height().div_ceil(TILE))
}

/// The cells tile `t` reads, in a fixed order.
fn window<T: Clone>(g: &Grid<T>, t: usize) -> Vec<T> {
    let (cols, _) = tiles(g);
    let (tx, ty) = (t % cols, t / cols);
    let mut out = Vec::new();
    for y in (ty * TILE).saturating_sub(REACH)..((ty + 1) * TILE + REACH).min(g.height()) {
        for x in (tx * TILE).saturating_sub(REACH)..((tx + 1) * TILE + REACH).min(g.width()) {
            out.push(g.as_slice()[y * g.width() + x].clone());
        }
    }
    out
}

fn show<T: Clone>(name: &'static str, g: &Grid<T>) -> Shown<T> {
    let (cols, rows) = tiles(g);
    Shown { name, stamps: (0..cols * rows).map(|t| g.tile_stamp(t % cols, t / cols)).collect(), cells: (0..cols * rows).map(|t| window(g, t)).collect(), origin: g.origin(), redrawn: 0 }
}

/// Redraw the tiles whose stamp changed; no other tile may show a cell that is now `far`
/// from what it shows.
fn check<T: Clone + std::fmt::Debug>(g: &Grid<T>, shown: &mut Shown<T>, far: impl Fn(&T, &T) -> bool) {
    assert_eq!(g.origin(), shown.origin, "{}: a stepped world keeps its grids", shown.name);
    let (cols, _) = tiles(g);
    for t in 0..shown.stamps.len() {
        let now = window(g, t);
        let stamp = g.tile_stamp(t % cols, t / cols);
        if stamp != shown.stamps[t] {
            shown.stamps[t] = stamp;
            shown.cells[t] = now;
            shown.redrawn += 1;
        } else if let Some((a, b)) = now.iter().zip(&shown.cells[t]).find(|(a, b)| far(a, b)) {
            panic!("{}: tile ({}, {}) shows {b:?} where there is now {a:?}, and kept its stamp", shown.name, t % cols, t / cols);
        }
    }
}

fn bits(a: &f32, b: &f32) -> bool { a.to_bits() != b.to_bits() }

/// Further apart than `step`: these grids are drawn no finer, and a tile is stamped when a
/// cell it reads has moved that far from what the tile last showed.
fn beyond(step: f32) -> impl Fn(&f32, &f32) -> bool { move |a, b| (a - b).abs() > step }

fn looks(s: &SimState) -> BTreeMap<ResourceId, Look> {
    s.resources.items().iter().map(|r| (r.id, r.look())).collect()
}

#[test]
fn whatever_changes_in_a_played_colony_is_reported() {
    let (mut s, dam) = sim::starts::new_game_on_map("telemark").expect("map");
    s.apply(Command::Settle { cell: dam.camp }).expect("settle");
    let mut player = Player::new(Style::Attentive, dam.camp, Some((dam.cells.clone(), dam.height)));
    let mut height = show("height", &s.height);
    let mut depth = show("depth", &s.water.depth);
    let mut speed = show("speed", &s.water.velocity);
    let mut ice = show("ice", &s.water.ice);
    let mut dams = show("dams", &s.water.dam_height);
    let mut riverbed = show("riverbed", &s.water.riverbed);
    let mut cover = show("cover", &s.ecology.vegetation);
    let mut canal = show("canal", &s.water.canal);
    let mut shore = show("shore", &s.water.shore);
    let mut seen = looks(&s);
    let mut count = s.resources.changes().count();
    let (mut noted_in_all, mut steps) = (0usize, 0usize);
    for _day in 0..8 {
        player.act(&mut s);
        for _ in 0..(1.0 / STEP_DAYS) as u32 {
            s.step(STEP_DAYS).expect("step");
            s.colony.take_events();
            steps += 1;
            check(&s.height, &mut height, bits);
            check(&s.water.dam_height, &mut dams, bits);
            check(&s.water.ice, &mut ice, bits);
            check(&s.water.canal, &mut canal, |a, b| a != b);
            // The water's surface, the riverbed and the plant cover: a step of what is drawn.
            check(&s.water.depth, &mut depth, beyond(sim::water::DEPTH_STEP));
            check(&s.water.velocity, &mut speed, |a, b| beyond(sim::water::SPEED_STEP)(&a.0, &b.0) || beyond(sim::water::SPEED_STEP)(&a.1, &b.1));
            check(&s.water.riverbed, &mut riverbed, beyond(1.0 / sim::water::RIVERBED_STEPS));
            check(&s.ecology.vegetation, &mut cover, beyond(1.0 / sim::ecology::COVER_STEPS));
            // The water's edge, in the steps it is kept in.
            check(&s.water.shore, &mut shore, |a, b| a != b);
            for (edge, &d) in s.water.shore.as_slice().iter().zip(s.water.depth.as_slice()) {
                assert_eq!(*edge, sim::water::shore_step(d), "the shore is the depth in steps");
            }

            let now = looks(&s);
            let noted: HashSet<ResourceId> = s.resources.changes().since(count).expect("the list reaches back one step").collect();
            for (id, look) in &now {
                assert!(seen.get(id) == Some(look) || noted.contains(id), "{id:?} came to look like {look:?} unnoted");
            }
            for id in seen.keys() {
                assert!(now.contains_key(id) || noted.contains(id), "{id:?} went unnoted");
            }
            noted_in_all += noted.len();
            count = s.resources.changes().count();
            seen = now;
        }
    }
    // The reports are worth having only if they are short: a few sources a step out of
    // thousands, and a tile of ground or water redrawn far less often than every step.
    let all = steps * depth.stamps.len();
    assert!(noted_in_all < steps * 2, "{noted_in_all} sources noted in {steps} steps");
    for (name, redrawn) in [("depth", depth.redrawn), ("speed", speed.redrawn), ("riverbed", riverbed.redrawn), ("cover", cover.redrawn), ("shore", shore.redrawn)] {
        assert!(redrawn < all / 50, "{name}: {redrawn} tiles redrawn, of {} in {steps} steps", depth.stamps.len());
    }
    assert!(noted_in_all > 0 && depth.redrawn > 0 && shore.redrawn > 0);
}

#[test]
fn a_reader_too_far_behind_is_told_to_look_at_everything() {
    let mut s = SimState::new(1);
    let start = s.resources.changes().count();
    let id = s.resources.add(sim::resources::ResourceKind::Plant, (3, 3), 4.0);
    assert_eq!(s.resources.changes().since(start).unwrap().collect::<Vec<_>>(), vec![id]);
    for _ in 0..10_000 {
        s.resources.add(sim::resources::ResourceKind::Plant, (4, 4), 4.0);
    }
    assert!(s.resources.changes().since(start).is_none());
    assert!(s.resources.changes().since(s.resources.changes().count() + 1).is_none());

    // A guess is read with the rest, and kept apart to be looked at again.
    let settled = s.resources.changes().count();
    s.guess_ahead();
    let guess = s.resources.add(sim::resources::ResourceKind::Plant, (5, 5), 4.0);
    assert_eq!(s.resources.changes().count(), settled);
    assert_eq!(s.resources.changes().since(settled).unwrap().collect::<Vec<_>>(), vec![guess]);
    assert_eq!(s.resources.changes().guesses().collect::<Vec<_>>(), vec![guess]);
}
