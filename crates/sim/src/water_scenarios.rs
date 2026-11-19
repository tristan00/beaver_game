//! Physical scenarios for the water engine (`water.rs`): small landscapes built by hand, run with
//! the real engine at the game's own step (`STEP_DAYS`), each checked against what water does:
//! volume is conserved, still water lies level, water runs downhill and never appears from
//! nowhere, a river carries off what flows into it, a dam fills to its crest at a rate set by the
//! inflow and then passes the inflow on, a breach drains the pond, a canal lowers a pond to its
//! floor and no further, and results do not depend on the time step.
use crate::grid::HeightGrid;
use crate::water::{Water, WaterConfig};
use crate::STEP_DAYS;

const N: usize = 40;

/// No rain, evaporation, seepage or dam wear: only flow, sources and edges move water.
fn still() -> WaterConfig {
    WaterConfig {
        rain: 0.0,
        evaporation: 0.0,
        seepage: 0.0,
        soil_absorb: 0.0,
        dam_decay_per_day: 0.0,
        overtop_erosion: 0.0,
        ..WaterConfig::default()
    }
}

struct Scene {
    h: HeightGrid,
    w: Water,
}

impl Scene {
    fn new(h: HeightGrid, cfg: WaterConfig) -> Self {
        let mut w = Water::new(h.width());
        w.config = cfg;
        Scene { h, w }
    }

    /// Runs `days` at step `dt`.
    fn run(&mut self, days: f32, dt: f32) {
        for _ in 0..(days / dt).round() as usize {
            self.w.step(&self.h, dt);
        }
    }

    fn depth(&self, x: usize, y: usize) -> f32 { *self.w.depth.get(x, y).unwrap() }
    fn ground(&self, x: usize, y: usize) -> f32 { *self.h.get(x, y).unwrap() + *self.w.dam_height.get(x, y).unwrap() }
    fn level(&self, x: usize, y: usize) -> f32 { self.ground(x, y) + self.depth(x, y) }
    fn set_depth(&mut self, x: usize, y: usize, d: f32) { *self.w.depth.get_mut(x, y).unwrap() = d; }
    fn volume(&self) -> f64 { self.w.total_volume() }
    /// Volume of the cells with `x` in `xs`.
    fn volume_in(&self, xs: std::ops::Range<usize>) -> f64 {
        (0..N).flat_map(|y| xs.clone().map(move |x| (x, y))).map(|(x, y)| self.depth(x, y) as f64).sum()
    }
    /// What the books say the volume should be now, from `v0` when the counters stood at zero.
    fn booked(&self, v0: f64) -> f64 { v0 + self.w.added_volume - self.w.lost_volume }
    /// Volume now and a snapshot of the counters, to check the books from this moment.
    fn mark(&self) -> (f64, f64, f64) { (self.volume(), self.w.added_volume, self.w.lost_volume) }
    /// Volume the books expect now, from a `mark`.
    fn booked_since(&self, m: (f64, f64, f64)) -> f64 { m.0 + (self.w.added_volume - m.1) - (self.w.lost_volume - m.2) }
}

/// A valley along +x falling `slope` per cell to the open edge at x = N-1: a flat floor three cells
/// wide at y = N/2, banks rising `bank` per cell to either side.
fn valley(slope: f32, bank: f32) -> HeightGrid {
    let mut h = HeightGrid::new(N, N, 0.0);
    let mid = (N / 2) as f32;
    for y in 0..N {
        for x in 0..N {
            let side = ((y as f32 - mid).abs() - 1.0).max(0.0);
            *h.get_mut(x, y).unwrap() = 10.0 - slope * x as f32 + bank * side;
        }
    }
    h
}

/// A smooth round bowl, lowest at the centre.
fn bowl() -> HeightGrid {
    let mut h = HeightGrid::new(N, N, 0.0);
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let r2 = (x as f32 - c).powi(2) + (y as f32 - c).powi(2);
            *h.get_mut(x, y).unwrap() = 5.0 + 0.01 * r2;
        }
    }
    h
}

const MID: usize = N / 2;
const DAM_X: usize = 26;
const DT: f32 = STEP_DAYS;

/// The valley with a full-width dam of `height` at `DAM_X` (its crest is level across the valley
/// floor, higher on the banks, so water can only pass over it).
fn dammed_valley(height: f32, cfg: WaterConfig) -> Scene {
    let mut s = Scene::new(valley(0.02, 0.4), cfg);
    let cells: Vec<(usize, usize)> = (0..N).map(|y| (DAM_X, y)).collect();
    s.w.build_dam(&cells, height).unwrap();
    s
}

/// Water surface just upstream of the dam, mid valley.
fn pond_level(s: &Scene) -> f32 { s.level(DAM_X - 1, MID) }

// ------------------------------------------------------------------------------------------------
// Conservation and rest
// ------------------------------------------------------------------------------------------------

#[test]
fn a_closed_bowl_keeps_its_water_and_comes_to_rest_level() {
    let mut s = Scene::new(bowl(), WaterConfig { open_edges: false, ..still() });
    for y in 6..12 {
        for x in 6..12 {
            s.set_depth(x, y, 3.0);
        }
    }
    let v0 = s.volume();
    s.run(60.0, DT);
    assert!(((s.volume() - v0) / v0).abs() < 1e-4, "volume {} -> {}", v0, s.volume());
    let wet: Vec<f32> = (0..N).flat_map(|y| (0..N).map(move |x| (x, y))).filter(|&(x, y)| s.depth(x, y) > 0.05).map(|(x, y)| s.level(x, y)).collect();
    let (lo, hi) = wet.iter().fold((f32::MAX, f32::MIN), |(a, b), &l| (a.min(l), b.max(l)));
    assert!(hi - lo < 0.02, "still water lies level: surface spans {lo:.3}..{hi:.3}");
    let c = N / 2;
    let (fx, fy) = s.w.flow_at(c, c);
    assert!(fx.abs() + fy.abs() < 1e-3, "and stops moving: flow {fx}, {fy}");
}

#[test]
fn dry_land_stays_dry_with_no_source() {
    // the default climate: light rain that the ground soaks up, evaporation, seepage
    let mut s = Scene::new(valley(0.05, 0.3), WaterConfig::default());
    s.run(120.0, DT);
    assert_eq!(s.volume(), 0.0, "no water appears from nowhere");
}

#[test]
fn the_books_balance_with_sources_climate_edges_and_dams() {
    let mut s = dammed_valley(1.2, WaterConfig::default());
    s.w.add_source(2, MID, 3.0).unwrap();
    let v0 = s.volume();
    s.run(150.0, DT);
    let (have, books) = (s.volume(), s.booked(v0));
    let through = s.w.added_volume + s.w.lost_volume;
    assert!((have - books).abs() < 1e-4 * through, "volume {have:.4} vs books {books:.4} after {through:.0} through");
}

#[test]
fn water_never_climbs_a_wall() {
    // a bowl walled at x = 20: water poured on the low side stays there
    let mut h = bowl();
    for y in 0..N {
        *h.get_mut(20, y).unwrap() = 50.0;
    }
    let mut s = Scene::new(h, WaterConfig { open_edges: false, ..still() });
    for y in 15..25 {
        for x in 10..16 {
            s.set_depth(x, y, 2.0);
        }
    }
    s.run(40.0, DT);
    assert_eq!(s.volume_in(21..N), 0.0, "nothing crosses a wall higher than the water");
}

#[test]
fn two_basins_joined_by_a_channel_settle_to_one_level() {
    // two pits 10 cells apart joined by a shallow channel; only one starts full
    let mut h = HeightGrid::new(N, N, 6.0);
    for y in 14..26 {
        for x in 4..14 {
            *h.get_mut(x, y).unwrap() = 2.0;
        }
        for x in 26..36 {
            *h.get_mut(x, y).unwrap() = 1.0;
        }
    }
    for x in 14..26 {
        *h.get_mut(x, 20).unwrap() = 3.0; // the channel floor
    }
    let mut s = Scene::new(h, WaterConfig { open_edges: false, ..still() });
    for y in 14..26 {
        for x in 4..14 {
            s.set_depth(x, y, 3.0); // level 5, above the channel floor
        }
    }
    s.run(120.0, DT);
    let (a, b) = (s.level(8, 20), s.level(30, 20));
    // 360 units shared between two 120-cell pits: one level, 3.0, which is also the channel floor
    assert!((a - 3.0).abs() < 0.03 && (b - 3.0).abs() < 0.03, "both settle at 3.0: {a:.3} and {b:.3}");
}

// ------------------------------------------------------------------------------------------------
// Rivers
// ------------------------------------------------------------------------------------------------

/// Steady discharge out of the map over `days` (outflow per day), after `warm` days of warm-up.
fn steady_outflow(s: &mut Scene, warm: f32, days: f32) -> (f64, f64) {
    s.run(warm, DT);
    let (lost0, v0) = (s.w.lost_volume, s.volume());
    s.run(days, DT);
    ((s.w.lost_volume - lost0) / days as f64, (s.volume() - v0) / days as f64)
}

#[test]
fn a_steady_river_carries_off_exactly_what_flows_into_it() {
    let q = 2.0;
    let mut s = Scene::new(valley(0.02, 0.4), still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    let (out, dv) = steady_outflow(&mut s, 120.0, 30.0);
    assert!((out - q as f64).abs() < 0.03 * q as f64, "outflow {out:.3}/day for inflow {q}/day");
    assert!(dv.abs() < 0.01 * q as f64, "the river is steady: volume changes {dv:.4}/day");
    assert!(s.depth(N - 3, MID) > 0.0, "and reaches the map edge");
}

#[test]
fn more_inflow_makes_a_deeper_river() {
    let depth_for = |q: f32| {
        let mut s = Scene::new(valley(0.02, 0.4), still());
        s.w.add_source(2, MID, q).unwrap();
        s.w.sources[0].seasonality = 0.0;
        s.run(60.0, DT);
        s.depth(MID, MID)
    };
    let (a, b, c) = (depth_for(0.5), depth_for(2.0), depth_for(8.0));
    assert!(a > 0.0 && a < b && b < c, "depths {a:.4} {b:.4} {c:.4}");
}

#[test]
fn how_far_water_runs_in_a_day_does_not_depend_on_the_step() {
    // the same river front, stepped at the game's step and at a finer and a coarser one
    let front_after = |dt: f32| {
        let mut s = Scene::new(valley(0.02, 0.4), still());
        s.w.add_source(2, MID, 2.0).unwrap();
        s.w.sources[0].seasonality = 0.0;
        s.run(1.0, dt);
        (0..N).rev().find(|&x| s.depth(x, MID) > 0.01).unwrap_or(0)
    };
    let (fine, game, coarse) = (front_after(0.01), front_after(DT), front_after(0.25));
    assert!(fine.abs_diff(game) <= 2 && coarse.abs_diff(game) <= 2, "front after a day: {fine} (dt 0.01), {game} (dt {DT}), {coarse} (dt 0.25)");
}

// ------------------------------------------------------------------------------------------------
// Dams
// ------------------------------------------------------------------------------------------------

/// Cells and volume the dam's pond holds when full (bed below the crest, upstream of the dam).
fn pond_capacity(s: &Scene, crest: f32) -> f64 {
    (0..N)
        .flat_map(|y| (0..DAM_X).map(move |x| (x, y)))
        .map(|(x, y)| (crest - s.ground(x, y)).max(0.0) as f64)
        .sum()
}

#[test]
fn a_dam_fills_its_pond_smoothly_to_the_crest_in_the_time_the_inflow_takes() {
    let (q, h) = (2.0f32, 1.5f32);
    let mut s = dammed_valley(h, still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    let crest = s.ground(DAM_X, MID);
    let cap = pond_capacity(&s, crest);
    let mut levels = vec![pond_level(&s)];
    let mut full_at = None;
    for day in 1..=200 {
        s.run(1.0, DT);
        let l = pond_level(&s);
        levels.push(l);
        if full_at.is_none() && l >= crest - 0.05 {
            full_at = Some(day as f64);
        }
    }
    let full_at = full_at.expect("the pond fills");
    let expected = cap / q as f64;
    assert!((full_at - expected).abs() < 0.3 * expected + 2.0, "filled in {full_at} days; capacity {cap:.1} at {q}/day gives {expected:.1}");
    // it rises every day until full (no long flat spell, then a jump) and never far above the crest
    let rising = &levels[..full_at as usize];
    let mean = (rising[rising.len() - 1] - rising[0]) / (rising.len() - 1) as f32;
    let worst = rising.windows(2).map(|w| w[1] - w[0]).fold(f32::MAX, f32::min);
    assert!(worst > 0.1 * mean, "the level rises every day while filling, never stalling: smallest daily rise {worst:.4} vs a mean of {mean:.4}");
    let biggest = rising.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
    assert!(biggest < 0.35 * h, "no sudden jump: biggest daily rise {biggest:.3} of a {h} dam");
    assert!(levels.iter().all(|&l| l < crest + 0.2), "never far over the crest");
}

#[test]
fn a_full_pond_passes_its_inflow_on_downstream() {
    let q = 2.0;
    let mut s = dammed_valley(1.5, still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    let (out, dv) = steady_outflow(&mut s, 200.0, 30.0);
    assert!((out - q as f64).abs() < 0.05 * q as f64, "a full pond spills its inflow: {out:.3}/day for {q}/day");
    assert!(dv.abs() < 0.02 * q as f64, "and holds steady: {dv:.4}/day");
}

#[test]
fn while_the_pond_fills_less_water_reaches_downstream() {
    let q = 2.0;
    let mut s = dammed_valley(1.5, still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(3.0, DT); // the water already below the dam runs off
    let lost0 = s.w.lost_volume;
    s.run(10.0, DT);
    let out = (s.w.lost_volume - lost0) / 10.0;
    assert!(out < 0.1 * q as f64, "the dam holds back the river while filling: {out:.3}/day of {q}/day leaves");
}

#[test]
fn a_dam_that_does_not_reach_across_the_valley_leaks_round_its_end() {
    let mut s = Scene::new(valley(0.02, 0.4), still());
    // a 1.5 dam over the floor and one bank only: the other bank is open at the floor's height + 0.4
    let cells: Vec<(usize, usize)> = (MID - 1..N).map(|y| (DAM_X, y)).collect();
    s.w.build_dam(&cells, 1.5).unwrap();
    s.w.add_source(2, MID, 2.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(150.0, DT);
    let spill = s.ground(DAM_X, MID - 2); // the open bank beside the dam's end
    let (level, crest) = (pond_level(&s), s.ground(DAM_X, MID));
    // it stands just above the gap (the head that pushes the river round), far below the crest
    assert!(level < spill + 0.3 && level < crest - 0.5, "the pond stops at the gap round the end: level {level:.3}, gap {spill:.3}, crest {crest:.3}");
}

#[test]
fn a_half_built_dam_holds_a_lower_pond_and_raising_it_raises_the_pond() {
    let mut s = dammed_valley(0.7, still());
    s.w.add_source(2, MID, 2.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(150.0, DT);
    let half = pond_level(&s);
    let floor = *s.h.get(DAM_X, MID).unwrap();
    assert!((half - (floor + 0.7)).abs() < 0.15, "a 0.7 dam holds the pond near its crest: {half:.3} vs {:.3}", floor + 0.7);
    let cells: Vec<((usize, usize), f32)> = (0..N).map(|y| ((DAM_X, y), 1.5)).collect();
    s.w.raise_dam(&cells).unwrap();
    s.run(150.0, DT);
    assert!(pond_level(&s) > half + 0.6, "raised, it holds more: {:.3} after {half:.3}", pond_level(&s));
}

#[test]
fn a_breached_dam_drains_its_pond_in_a_flood_and_keeps_the_books() {
    let q = 1.0;
    let mut s = dammed_valley(1.5, still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(250.0, DT);
    let m = s.mark();
    let held = s.volume_in(0..DAM_X);
    for y in 0..N {
        s.w.breach(DAM_X, y).unwrap();
    }
    let lost0 = s.w.lost_volume;
    s.run(1.0, DT);
    let flood = s.w.lost_volume - lost0;
    s.run(30.0, DT);
    let after = s.volume_in(0..DAM_X);
    assert!(flood > 3.0 * q as f64, "the breach sends a flood off downstream: {flood:.2} in the first day for {q}/day inflow");
    assert!(after < 0.3 * held, "the pond empties: {held:.1} held, {after:.1} left after 30 days");
    let books = s.booked_since(m);
    assert!((s.volume() - books).abs() < 1e-3 * m.0, "no water created or lost: {:.3} vs {books:.3}", s.volume());
}

#[test]
fn water_spilling_over_a_dam_wears_it_away_and_a_dam_not_overtopped_holds() {
    let cfg = WaterConfig { dam_decay_per_day: 0.0, ..still() };
    let cfg = WaterConfig { overtop_erosion: crate::water::OVERTOP_EROSION, ..cfg };
    // a big river over a low dam: deep overtopping
    let mut s = dammed_valley(0.5, cfg.clone());
    s.w.add_source(2, MID, 160.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(60.0, DT);
    assert!(!s.w.has_dam(DAM_X, MID), "a torrent over its crest breaks the dam");
    // a trickle that never fills it: no wear
    let mut s = dammed_valley(1.5, cfg);
    s.w.add_source(2, MID, 0.05).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(60.0, DT);
    assert!((*s.w.dam_integrity.get(DAM_X, MID).unwrap() - 1.0).abs() < 1e-6, "a dam the water never tops does not wear");
}

// ------------------------------------------------------------------------------------------------
// Canals
// ------------------------------------------------------------------------------------------------

/// A reservoir pit (floor 1, rim 6) beside a lower drain pit (floor 0), parted by a ridge; the
/// reservoir starts with its surface at 4.
fn reservoir() -> Scene {
    let mut h = HeightGrid::new(N, N, 6.0);
    for y in 10..30 {
        for x in 4..18 {
            *h.get_mut(x, y).unwrap() = 1.0;
        }
        for x in 24..36 {
            *h.get_mut(x, y).unwrap() = 0.0;
        }
    }
    let mut s = Scene::new(h, WaterConfig { open_edges: false, ..still() });
    for y in 10..30 {
        for x in 4..18 {
            s.set_depth(x, y, 3.0);
        }
    }
    s
}

#[test]
fn a_canal_lowers_a_reservoir_to_its_floor_and_no_further() {
    let mut s = reservoir();
    // a canal through the ridge (rim 6) dug down to 2.5
    let cut: Vec<(usize, usize)> = (18..24).map(|x| (x, 20)).collect();
    s.w.dig_canal(&mut s.h, &cut, 3.5).unwrap();
    s.run(80.0, DT);
    let level = s.level(10, 20);
    assert!(level > 2.45 && level < 2.6, "the reservoir drains to the canal floor (2.5): {level:.3}");
    assert!(s.volume_in(24..36) > 10.0, "the water went down the canal into the drain pit");
}

#[test]
fn a_canal_cut_above_the_water_carries_nothing() {
    let mut s = reservoir();
    let cut: Vec<(usize, usize)> = (18..24).map(|x| (x, 20)).collect();
    s.w.dig_canal(&mut s.h, &cut, 1.5).unwrap(); // floor 4.5, above the surface at 4
    s.run(40.0, DT);
    assert!((s.level(10, 20) - 4.0).abs() < 1e-3, "level {:.4}", s.level(10, 20));
    assert_eq!(s.volume_in(24..36), 0.0);
}

// ------------------------------------------------------------------------------------------------
// Climate
// ------------------------------------------------------------------------------------------------

#[test]
fn a_still_pond_loses_evaporation_and_seepage_at_their_rates_and_a_fed_one_does_not() {
    // a walled pit, so the wet area stays 10 x 10 cells
    let pit = |fed: bool| {
        let cfg = WaterConfig { open_edges: false, rain: 0.0, ..WaterConfig::default() };
        let mut h = HeightGrid::new(N, N, 5.0);
        for y in 10..20 {
            for x in 10..20 {
                *h.get_mut(x, y).unwrap() = 1.0;
            }
        }
        let mut s = Scene::new(h, cfg);
        for y in 10..20 {
            for x in 10..20 {
                s.set_depth(x, y, 1.0);
            }
        }
        if fed {
            for f in s.w.fed.as_mut_slice() {
                *f = true;
            }
        }
        s.run(5.0, DT);
        s
    };
    let s = pit(false);
    let rate = s.w.config.evaporation * crate::water::Season::Spring.evap_mult() + s.w.config.seepage;
    let per_cell_day = (100.0 - s.volume()) / 5.0 / 100.0;
    assert!((per_cell_day - rate as f64).abs() < 0.02 * rate as f64, "loses evaporation + seepage: {per_cell_day:.4}/day vs {rate:.4}");
    // a pond the water table keeps (river, lake) does not shrink
    assert!((pit(true).volume() - 100.0).abs() < 1e-3);
}

// ------------------------------------------------------------------------------------------------
// Robustness
// ------------------------------------------------------------------------------------------------

#[test]
fn rough_ground_many_sources_and_dams_stay_finite_non_negative_and_balanced() {
    let mut h = HeightGrid::new(N, N, 0.0);
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            *h.get_mut(x, y).unwrap() = 8.0 - 0.1 * fx + (fx * 0.7).sin() * 1.3 + (fy * 0.9 + fx * 0.3).cos() * 1.1;
        }
    }
    let mut s = Scene::new(h, WaterConfig::default());
    for (i, (x, y)) in [(3, 5), (5, 30), (12, 18), (20, 7), (2, 22)].into_iter().enumerate() {
        s.w.add_source(x, y, 0.5 + i as f32).unwrap();
    }
    s.w.build_dam(&(0..N).map(|y| (25, y)).collect::<Vec<_>>(), 2.0).unwrap();
    s.w.build_dam(&(5..20).map(|x| (x, 12)).collect::<Vec<_>>(), 1.0).unwrap();
    let v0 = s.volume();
    s.run(200.0, DT);
    assert!(s.w.depth.as_slice().iter().all(|d| d.is_finite() && *d >= 0.0), "every depth finite and non-negative");
    let books = s.booked(v0);
    assert!((s.volume() - books).abs() < 1e-3 * books.max(1.0), "{} vs {books}", s.volume());
}

#[test]
#[ignore]
fn probe_canal_drain_timeline() {
    let mut s = reservoir();
    let cut: Vec<(usize, usize)> = (18..24).map(|x| (x, 20)).collect();
    s.w.dig_canal(&mut s.h, &cut, 3.5).unwrap();
    for d in 0..=40 {
        if d % 4 == 0 {
            println!("day {:>3}: reservoir {:.3} drain {:.3} canal depth {:.3}", d * 10, s.level(10, 20), s.level(30, 20), s.depth(20, 20));
        }
        s.run(10.0, DT);
    }
}

#[test]
#[ignore]
fn probe_water_cost_on_a_real_map() {
    let id = crate::maps::all().first().unwrap().id.clone();
    let (s, _) = crate::starts::new_game_on_map(&id).unwrap();
    let mut w = s.water.clone();
    let t = std::time::Instant::now();
    for _ in 0..20 {
        w.step(&s.height, STEP_DAYS);
    }
    let per_day = t.elapsed().as_secs_f64();
    println!("map {} size {}: one sim day of water = {:.1} ms", id, s.world.size, per_day * 1000.0);
}

#[test]
#[ignore]
fn probe_river_depths_on_a_real_map() {
    for info in crate::maps::all() {
        let t = std::time::Instant::now();
        let (s, dam) = crate::starts::new_game_on_map(&info.id).unwrap();
        let build = t.elapsed().as_secs_f32();
        let mut w = s.water.clone();
        for _ in 0..(30.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let river: Vec<f32> = s.world.river.as_slice().iter().zip(w.depth.as_slice()).filter(|(r, _)| **r).map(|(_, d)| *d).collect();
        let mut sorted = river.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let wet = w.depth.as_slice().iter().filter(|d| **d > 0.05).count();
        println!("{:<10} build {build:.2}s river cells {} depth median {:.3} p90 {:.3} | wet cells {} ({:.2}x river) | dam {} cells h {:.1}", info.id, river.len(), sorted[sorted.len() / 2], sorted[sorted.len() * 9 / 10], wet, wet as f32 / river.len() as f32, dam.cells.len(), dam.height);
    }
}

#[test]
#[ignore]
fn probe_stream_capacity() {
    // a stream at the start rivers' slope: what inflow gives it 0.3 depth, and what a day of water costs
    for q in [1.0f32, 5.0, 20.0, 40.0] {
        let mut s = Scene::new(valley(0.02, 0.4), still());
        s.w.add_source(2, MID, q).unwrap();
        s.w.sources[0].seasonality = 0.0;
        let t = std::time::Instant::now();
        s.run(30.0, DT);
        let ms = t.elapsed().as_secs_f64() * 1000.0 / 30.0;
        println!("inflow {q:>5}: depth mid {:.3}, width wet {} | {ms:.2} ms/day on {N}x{N}", s.depth(MID, MID), (0..N).filter(|&y| s.depth(MID, y) > 0.02).count());
    }
}

#[test]
#[ignore]
fn probe_breach_books() {
    let mut s = dammed_valley(1.5, still());
    s.w.add_source(2, MID, 1.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    let v0 = s.volume();
    for d in 0..250 {
        s.run(1.0, DT);
        let gap = s.volume() - s.booked(v0);
        if gap.abs() > 0.01 {
            println!("before breach, day {d}: gap {gap:.3}");
            break;
        }
    }
    let v1 = s.volume();
    let (a1, l1) = (s.w.added_volume, s.w.lost_volume);
    for y in 0..N {
        s.w.breach(DAM_X, y).unwrap();
    }
    for k in 0..40 {
        s.w.step(&s.h, DT);
        let gap = s.volume() - (v1 + (s.w.added_volume - a1) - (s.w.lost_volume - l1));
        if k < 6 || gap.abs() > 0.5 {
            println!("after breach step {k}: volume {:.3} gap {gap:.3}", s.volume());
        }
        if gap.abs() > 0.5 {
            break;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The real maps
// ------------------------------------------------------------------------------------------------

/// On every bundled map, from its new game with nobody touching it: water enters only at rivers
/// and lakes (springs, baseflow, rivers from beyond the map); over a stretch of days, what enters
/// equals what leaves plus what is stored; most of it leaves the way rivers do, off the map
/// downstream, not by vanishing; and a river that enters the map leaves it again.
#[test]
fn on_every_map_the_water_that_enters_leaves_by_the_rivers_and_the_books_balance() {
    for info in crate::maps::all() {
        let (s, _) = crate::starts::new_game_on_map(&info.id).unwrap();
        let world = &s.world;
        for input in &world.water_inputs {
            let (x, y) = input.cell;
            let n = world.size;
            if matches!(input.kind, crate::worldgen::InputKind::Inflow) {
                // water carried in from beyond the map enters right at the border
                let edge = x.min(y).min(n - 1 - x).min(n - 1 - y);
                assert!(edge <= 1, "{}: inflow {input:?} is {edge} cells inside the map", info.id);
            } else {
                let wet = world.river.get(x, y).copied().unwrap_or(false) || world.lake.get(x, y).copied().unwrap_or(false);
                assert!(wet, "{}: spring or baseflow {input:?} is not on a river or lake: water from nowhere", info.id);
            }
        }
        let mut w = s.water.clone();
        for _ in 0..(20.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let (b0, v0) = (w.budget(), w.total_volume());
        let days = 10.0;
        for _ in 0..(days / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let (b1, v1) = (w.budget(), w.total_volume());
        let added = b1.added() - b0.added();
        let lost = b1.lost() - b0.lost();
        let off_map = b1.edges - b0.edges;
        let stored = v1 - v0;
        assert!(added > 0.0, "{}: no water enters", info.id);
        assert!((added - lost - stored).abs() < 0.01 * added, "{}: in {added:.1} vs out {lost:.1} + stored {stored:.1}", info.id);
        assert!(stored.abs() < 0.15 * added, "{}: the rivers have settled: storage changes {stored:.1} of {added:.1} in", info.id);
        assert!(off_map > 0.5 * added, "{}: most water leaves down the rivers: {off_map:.1} of {added:.1} (evaporation {:.1}, seepage {:.1})", info.id, b1.evaporation - b0.evaporation, b1.seepage - b0.seepage);
        // what the rivers from beyond the map actually brought in over the window (their seasonal rate)
        let season = w.season();
        let inflow: f64 = w
            .sources
            .iter()
            .zip(&world.water_inputs)
            .filter(|(_, i)| matches!(i.kind, crate::worldgen::InputKind::Inflow))
            .map(|(src, _)| (src.rate * (1.0 + src.seasonality * (season.inflow_mult() - 1.0))) as f64 * days as f64)
            .sum();
        if inflow > 0.0 {
            assert!(off_map > 0.8 * inflow, "{}: a river from beyond the map flows on off it: {off_map:.1} out for {inflow:.1} carried in", info.id);
        }
        println!("{:<10} in {added:>7.1} out {off_map:>7.1} (evap+seep {:>6.1}) stored {stored:>6.2} | from beyond the map {inflow:.1}", info.id, lost - off_map);
    }
}

// ------------------------------------------------------------------------------------------------
// More scenarios (from an independent review of what the suite missed)
// ------------------------------------------------------------------------------------------------

/// A filled dammed valley at inflow `q`, no wear unless `cfg` says so.
fn filled(q: f32, h: f32, cfg: WaterConfig, days: f32) -> Scene {
    let mut s = dammed_valley(h, cfg);
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(days, DT);
    s
}

#[test]
fn a_dam_wears_faster_the_more_water_pours_over_it_and_ordinary_spill_does_not_wear_it() {
    // integrity lost in 20 days over a full pond, for growing floods (no age wear)
    let wear = |q: f32| {
        let cfg = WaterConfig { dam_decay_per_day: 0.0, overtop_erosion: crate::water::OVERTOP_EROSION, ..still() };
        let mut s = dammed_valley(1.5, WaterConfig { overtop_erosion: 0.0, ..cfg.clone() });
        s.w.add_source(2, MID, q).unwrap();
        s.w.sources[0].seasonality = 0.0;
        s.run(150.0, DT); // full, without wear
        s.w.config = cfg;
        s.run(20.0, DT);
        1.0 - *s.w.dam_integrity.get(DAM_X, MID).unwrap()
    };
    let (normal, flood, torrent) = (wear(2.0), wear(80.0), wear(160.0));
    assert!(normal < 1e-6, "a river's ordinary spill over its full pond does not wear the dam: {normal}");
    assert!(flood > 0.0 && torrent > flood, "wear grows with the flood: {flood:.3} then {torrent:.3}");
}

/// A dam is tied into its banks, which stand higher than its crest: dry ground beside a dam cell
/// pours nothing over it. And a full pond passes its river over the crest: up to the biggest river
/// a map brings in, that ordinary spill does not wear the dam either. No cell of the dam wears.
#[test]
fn neither_the_dry_banks_a_dam_is_tied_into_nor_its_ordinary_spill_wear_it() {
    for q in [2.0, 10.0] {
        let cfg = WaterConfig { dam_decay_per_day: 0.0, overtop_erosion: crate::water::OVERTOP_EROSION, ..still() };
        let mut s = dammed_valley(1.5, cfg);
        s.w.add_source(2, MID, q).unwrap();
        s.w.sources[0].seasonality = 0.0;
        s.run(60.0, DT);
        for y in 0..N {
            let integrity = *s.w.dam_integrity.get(DAM_X, y).unwrap();
            assert_eq!(integrity, 1.0, "river {q}: dam cell at y {y} wore to {integrity}");
        }
    }
}

#[test]
fn the_previewed_fill_time_matches_the_engine_with_evaporation_and_seepage() {
    for seed in 1..=4u64 {
        let Ok((mut s, dam)) = crate::starts::new_game_with_dam(seed) else { continue };
        let p = crate::dam_preview::preview(&s, &dam.cells, dam.height).expect("a preview");
        s.water.build_dam(&dam.cells, dam.height).unwrap();
        let full = |s: &crate::SimState| p.flooded.iter().all(|&(x, y)| *s.water.depth.get(x, y).unwrap() + s.height.get(x, y).unwrap() >= p.level - 0.1);
        let mut t = 0.0f32;
        while !full(&s) && t < 400.0 {
            s.water.step(&s.height, STEP_DAYS);
            t += STEP_DAYS;
        }
        if p.fill_days.is_infinite() {
            assert!(t >= 400.0, "seed {seed}: previewed never full, filled in {t}");
            continue;
        }
        assert!(((t - p.fill_days) / p.fill_days).abs() < 0.2, "seed {seed}: filled in {t:.1} days, previewed {:.1}", p.fill_days);
    }
}

#[test]
fn a_year_of_seasons_with_winter_ice_keeps_the_books_and_the_pond() {
    let mut s = dammed_valley(1.5, WaterConfig::default());
    s.w.add_source(2, MID, 6.0).unwrap(); // a snowmelt river: full seasonal swing
    let m = s.mark();
    let mut ice_in_winter = 0.0f32;
    let mut lowest = f32::MAX;
    for day in 0..360 {
        s.run(1.0, DT);
        if day >= 60 {
            lowest = lowest.min(pond_level(&s));
        }
        if (275..300).contains(&day) {
            ice_in_winter = ice_in_winter.max(*s.w.ice.get(DAM_X - 3, MID).unwrap());
        }
    }
    let through = (s.w.added_volume - m.1) + (s.w.lost_volume - m.2);
    assert!((s.volume() - s.booked_since(m)).abs() < 1e-4 * through, "books balance over the year");
    assert!(ice_in_winter > 0.0, "the pond froze in winter");
    s.run(40.0, DT);
    assert!(*s.w.ice.get(DAM_X - 3, MID).unwrap() < 1e-6, "and thawed in spring");
    let floor = *s.h.get(DAM_X - 1, MID).unwrap();
    assert!(lowest > floor + 0.5, "the pond outlasts the dry summer and winter: lowest {lowest:.3} over a floor of {floor:.3}");
}

#[test]
fn a_dam_rising_as_it_is_built_backs_the_pond_up_steadily() {
    // the dam rises in even daily steps over 40 days, as builders raise it
    let (q, h) = (2.0f32, 1.5f32);
    let mut s = Scene::new(valley(0.02, 0.4), still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(20.0, DT); // the river is running
    let mut levels = vec![pond_level(&s)];
    for day in 1..=40 {
        let cells: Vec<((usize, usize), f32)> = (0..N).map(|y| ((DAM_X, y), h * day as f32 / 40.0)).collect();
        s.w.raise_dam(&cells).unwrap();
        s.run(1.0, DT);
        levels.push(pond_level(&s));
    }
    let rises: Vec<f32> = levels.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = rises.iter().sum::<f32>() / rises.len() as f32;
    assert!(mean > 0.0);
    let calm = rises.iter().filter(|r| **r < 0.3 * mean).count();
    assert!(calm <= 3, "the pond rises with the dam nearly every day: {calm} days of the 40 barely moved (mean rise {mean:.4}/day)");
}

#[test]
fn flow_is_reported_per_day_and_points_downstream() {
    let q = 2.0;
    let mut s = Scene::new(valley(0.02, 0.4), still());
    s.w.add_source(2, MID, q).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(120.0, DT);
    let section: f32 = (0..N).map(|y| s.w.flow_at(MID, y).0).sum();
    let across: f32 = (0..N).map(|y| s.w.flow_at(MID, y).1).sum();
    assert!((section - q).abs() < 0.1 * q, "the flow across a section is the river's discharge per day: {section:.3} for {q}");
    assert!(across.abs() < 0.05 * q, "and it runs down the valley, not across it: {across:.4}");
}

#[test]
fn a_breach_upstream_floods_through_a_dam_below_and_the_books_balance() {
    let mut s = Scene::new(valley(0.05, 0.4), still());
    let upper: Vec<(usize, usize)> = (0..N).map(|y| (12, y)).collect();
    let lower: Vec<(usize, usize)> = (0..N).map(|y| (28, y)).collect();
    s.w.build_dam(&upper, 1.5).unwrap();
    s.w.build_dam(&lower, 1.5).unwrap();
    s.w.add_source(2, MID, 2.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(250.0, DT);
    let between = s.volume_in(13..28);
    let m = s.mark();
    let out0 = s.w.lost_volume;
    for &(x, y) in &upper {
        s.w.breach(x, y).unwrap();
    }
    s.run(3.0, DT);
    let flood_out = (s.w.lost_volume - out0) / 3.0;
    assert!(s.volume_in(13..28) >= between - 0.5, "the lower pond takes the flood (it was full already: it holds)");
    assert!(flood_out > 2.0 * 2.0, "the lower dam passes the flood on: {flood_out:.2}/day leaving for a 2/day river");
    assert!((s.volume() - s.booked_since(m)).abs() < 1e-3 * m.0, "no water made or lost");
}

#[test]
fn two_rivers_into_one_pond_fill_it_in_the_time_their_sum_takes() {
    let (q1, q2) = (1.5f32, 1.0f32);
    let mut s = dammed_valley(1.5, still());
    s.w.add_source(2, MID, q1).unwrap();
    s.w.add_source(10, MID + 4, q2).unwrap(); // a side stream down the bank
    for src in &mut s.w.sources {
        src.seasonality = 0.0;
    }
    let crest = s.ground(DAM_X, MID);
    let expected = pond_capacity(&s, crest) / (q1 + q2) as f64;
    let mut t = 0.0;
    while pond_level(&s) < crest - 0.05 && t < 400.0 {
        s.run(1.0, DT);
        t += 1.0;
    }
    assert!((t - expected).abs() < 0.3 * expected + 2.0, "filled in {t} days; {expected:.1} expected from both rivers");
}

#[test]
fn a_breach_of_one_cell_drains_the_pond_more_slowly_than_a_full_breach_but_drains_it() {
    let drained = |cells: &[usize]| {
        let mut s = dammed_valley(1.5, still());
        s.w.add_source(2, MID, 0.5).unwrap();
        s.w.sources[0].seasonality = 0.0;
        s.run(400.0, DT);
        let held = s.volume_in(0..DAM_X);
        for &y in cells {
            s.w.breach(DAM_X, y).unwrap();
        }
        s.run(5.0, DT);
        let after5 = s.volume_in(0..DAM_X) / held;
        s.run(55.0, DT);
        (after5, s.volume_in(0..DAM_X) / held)
    };
    let (one5, one60) = drained(&[MID]);
    let all: Vec<usize> = (0..N).collect();
    let (all5, _) = drained(&all);
    assert!(one5 > all5, "a narrow gap drains slower: {one5:.2} left after 5 days vs {all5:.2}");
    assert!(one60 < 0.5, "but it drains: {one60:.2} left after 60 days");
}

#[test]
fn a_lake_against_a_flat_map_edge_runs_off_it_and_one_against_rising_ground_stays() {
    // a flat lake bed running to the west edge
    let flat = HeightGrid::new(N, N, 1.0);
    let mut s = Scene::new(flat, still());
    for y in 10..30 {
        for x in 0..8 {
            s.set_depth(x, y, 1.0);
        }
    }
    s.run(20.0, DT);
    assert!(s.volume() < 1.0, "the land goes on flat beyond the edge: the lake runs off ({:.2} left)", s.volume());
    // the same lake where the ground rises to the edge
    let mut h = HeightGrid::new(N, N, 1.0);
    for y in 0..N {
        for x in 0..4 {
            *h.get_mut(x, y).unwrap() = 1.0 + 0.5 * (4 - x) as f32;
        }
    }
    let mut s = Scene::new(h, still());
    for y in 10..30 {
        for x in 4..12 {
            s.set_depth(x, y, 1.0);
        }
    }
    let v0 = s.volume();
    s.run(20.0, DT);
    assert!(s.volume() > 0.99 * v0, "rising ground at the edge holds it");
}

#[test]
fn a_thin_film_dries_out_and_a_spring_on_a_hillside_runs_down_it() {
    let mut s = Scene::new(HeightGrid::new(N, N, 1.0), WaterConfig { open_edges: false, rain: 0.0, ..WaterConfig::default() });
    for y in 10..20 {
        for x in 10..20 {
            s.set_depth(x, y, 0.004);
        }
    }
    s.run(3.0, DT);
    assert_eq!(s.volume(), 0.0, "a film evaporates completely");
    // a spring high on a slope falling to +x
    let mut h = HeightGrid::new(N, N, 0.0);
    for y in 0..N {
        for x in 0..N {
            *h.get_mut(x, y).unwrap() = 20.0 - 0.3 * x as f32;
        }
    }
    let mut s = Scene::new(h, WaterConfig { open_edges: false, ..still() });
    s.w.add_source(5, MID, 1.0).unwrap();
    s.w.sources[0].seasonality = 0.0;
    s.run(10.0, DT);
    assert!(s.volume_in(0..5) < 0.01 * s.volume(), "no water climbs above the spring");
    assert!(s.volume_in(N - 5..N) > 0.5 * s.volume(), "it gathers at the foot of the slope");
}

#[test]
fn rain_heavier_than_the_ground_soaks_up_adds_water_and_the_books_keep_it() {
    let cfg = WaterConfig { rain: 0.05, evaporation: 0.0, seepage: 0.0, open_edges: false, ..WaterConfig::default() };
    let mut s = Scene::new(bowl(), cfg);
    let m = s.mark();
    s.run(10.0, DT);
    assert!(s.volume() > 0.0, "a downpour wets the ground");
    assert!((s.volume() - s.booked_since(m)).abs() < 1e-4 * s.w.added_volume.max(1.0), "and every drop is booked");
}

#[test]
#[ignore]
fn probe_walk_orders_in_a_new_game() {
    let (mut s, dam) = crate::starts::new_game_on_map("algonquin").unwrap();
    s.apply(crate::Command::Settle { cell: dam.camp }).unwrap();
    for _ in 0..20 { s.step(STEP_DAYS).unwrap(); }
    let id = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult).unwrap().id;
    let from = s.agents.get(id).unwrap().cell;
    // targets: the nearest unexplored in-map cells in four directions
    let n = s.world.size as i64;
    let mut targets = Vec::new();
    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
        let mut c = (from.0 as i64, from.1 as i64);
        while c.0 > 0 && c.1 > 0 && c.0 < n - 1 && c.1 < n - 1 && s.fog.explored((c.0 as usize, c.1 as usize)) {
            c = (c.0 + dx, c.1 + dy);
        }
        targets.push((c.0 + 2 * dx, c.1 + 2 * dy));
    }
    for (tx, ty) in targets {
        let to = (tx.clamp(0, n - 1) as usize, ty.clamp(0, n - 1) as usize);
        let r = s.apply(crate::Command::Assign { who: vec![id], to: Some(crate::agents::Assignment::Go(to)) });
        let explored = s.fog.explored(to);
        let mut moved_to = s.agents.get(id).unwrap().cell;
        for _ in 0..60 { s.step(STEP_DAYS).unwrap(); moved_to = s.agents.get(id).unwrap().cell; }
        let ag = s.agents.get(id).unwrap();
        println!("go {to:?} (explored {explored}, surface {:?}): apply {:?} -> at {moved_to:?}, assignment {:?}, task {:?}, path {:?}", s.nav.surface_at(to), r.map(|_| ()), ag.assignment, ag.task, ag.path.as_ref().map(|p| p.steps.len()));
    }
}

#[test]
#[ignore]
fn probe_map_fidelity() {
    for info in crate::maps::all() {
        let (s, dam) = crate::starts::new_game_on_map(&info.id).unwrap();
        let mut w = s.water.clone();
        for _ in 0..(30.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let n = s.world.size;
        let (mut river, mut river_vis, mut lake, mut lake_wet, mut land, mut land_wet) = (0, 0, 0, 0, 0, 0);
        for y in 0..n {
            for x in 0..n {
                let d = *w.depth.get(x, y).unwrap();
                if s.world.river.get(x, y) == Some(&true) {
                    river += 1;
                    river_vis += (d >= 0.1) as usize;
                } else if s.world.lake.get(x, y) == Some(&true) {
                    lake += 1;
                    lake_wet += (d >= 0.1) as usize;
                } else {
                    land += 1;
                    land_wet += (d >= 0.1) as usize;
                }
            }
        }
        let c = dam.camp;
        let edge = c.0.min(c.1).min(n - 1 - c.0).min(n - 1 - c.1);
        let inflows = s.world.water_inputs.iter().filter(|i| matches!(i.kind, crate::worldgen::InputKind::Inflow)).count();
        println!("{:<10} rivers {:>4} visible {:>5.1}% | lakes {:>4} wet {:>5.1}% | land {:>5} flooded {:>4} | camp {:?} {} from edge | inflows {inflows}",
            info.id, river, 100.0 * river_vis as f32 / river.max(1) as f32, lake, 100.0 * lake_wet as f32 / lake.max(1) as f32, land, land_wet, c, edge);
    }
}

#[test]
#[ignore]
fn probe_channels_vs_flow() {
    use crate::worldgen::InputKind;
    for info in crate::maps::all() {
        let (s, _) = crate::starts::new_game_on_map(&info.id).unwrap();
        let wd = &s.world;
        let n = wd.size;
        let mut w = s.water.clone();
        for _ in 0..(40.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let idx = |x: usize, y: usize| y * n + x;
        let (mut over, mut riv, mut chan_sum) = (0, 0, 0.0f32);
        let near_river = |x: usize, y: usize| {
            (y.saturating_sub(2)..=(y + 2).min(n - 1)).any(|j| (x.saturating_sub(2)..=(x + 2).min(n - 1)).any(|i| wd.river.as_slice()[idx(i, j)]))
        };
        let (mut flood_near, mut flood_far) = (0, 0);
        for y in 0..n {
            for x in 0..n {
                let i = idx(x, y);
                let d = w.depth.as_slice()[i];
                if wd.river.as_slice()[i] {
                    riv += 1;
                    let chan = wd.surface.as_slice()[i] - wd.height.as_slice()[i];
                    chan_sum += chan;
                    over += (d > chan + 0.05) as usize;
                } else if d >= 0.1 {
                    if near_river(x, y) { flood_near += 1 } else { flood_far += 1 }
                }
            }
        }
        let (mut inflow, mut spring, mut base) = (0.0f32, 0.0f32, 0.0f32);
        for (src, inp) in w.sources.iter().zip(&wd.water_inputs) {
            match inp.kind {
                InputKind::Inflow => inflow += src.rate,
                InputKind::Spring => spring += src.rate,
                InputKind::Baseflow => base += src.rate,
            }
        }
        println!("{:<10} channel mean {:.2} | river cells over the bank {:>3}/{} | flooded land near rivers {:>4}, far {:>4} | water in/day: edge inflows {:.1}, interior springs {:.1}, baseflow {:.1}",
            info.id, chan_sum / riv as f32, over, riv, flood_near, flood_far, inflow, spring, base);
    }
}

