use crate::grid::HeightGrid;
use crate::water::{manning_depth, Water, WaterConfig, MANNING_CHANNEL};
use crate::STEP_DAYS;

const N: usize = 40;
const MID: usize = N / 2;
const DAM_X: usize = 26;
const DT: f32 = STEP_DAYS;

fn still() -> WaterConfig {
    WaterConfig { rain: 0.0, evaporation: 0.0, seepage: 0.0, dam_decay_per_day: 0.0, overtop_erosion: 0.0, ..WaterConfig::default() }
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
    fn run(&mut self, days: f32) {
        for _ in 0..(days / DT).round() as usize {
            self.w.step(&self.h, DT);
        }
    }
    fn depth(&self, x: usize, y: usize) -> f32 { *self.w.depth.get(x, y).unwrap() }
    fn level(&self, x: usize, y: usize) -> f32 { self.h.get(x, y).unwrap() + self.w.dam_height.get(x, y).unwrap() + self.depth(x, y) }
    fn deepest_across(&self, x: usize) -> f32 { (0..N).map(|y| self.depth(x, y)).fold(0.0, f32::max) }
    fn source(&mut self, rate: f32) {
        self.w.add_source(1, MID, rate).unwrap();
        self.w.sources[0].seasonality = 0.0;
    }
    fn out_per_day(&mut self, days: f32) -> f64 {
        let e = self.w.budget().edges;
        self.run(days);
        (self.w.budget().edges - e) / days as f64
    }
}

fn grid(f: impl Fn(usize, usize) -> f32) -> HeightGrid {
    let mut h = HeightGrid::new(N, N, 0.0);
    for y in 0..N {
        for x in 0..N {
            *h.get_mut(x, y).unwrap() = f(x, y);
        }
    }
    h
}

fn valley(slope: f32) -> HeightGrid {
    grid(|x, y| 10.0 - slope * x as f32 + 0.4 * ((y as f32 - MID as f32).abs() - 1.0).max(0.0))
}

fn bowl() -> HeightGrid {
    let c = (N as f32 - 1.0) / 2.0;
    grid(|x, y| 5.0 + 0.01 * ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)))
}

fn dammed(height: f32, cfg: WaterConfig) -> Scene {
    let mut s = Scene::new(valley(0.02), cfg);
    let cells: Vec<(usize, usize)> = (0..N).map(|y| (DAM_X, y)).collect();
    s.w.build_dam(&cells, height).unwrap();
    s
}

fn capacity(s: &Scene) -> f64 {
    let crest = s.h.get(DAM_X, MID).unwrap() + s.w.dam_height.get(DAM_X, MID).unwrap();
    (0..N).flat_map(|y| (0..DAM_X).map(move |x| (x, y))).map(|(x, y)| (crest - s.h.get(x, y).unwrap()).max(0.0) as f64).sum()
}

fn pour(s: &mut Scene, xs: std::ops::Range<usize>, ys: std::ops::Range<usize>, d: f32) {
    for y in ys {
        for x in xs.clone() {
            *s.w.depth.get_mut(x, y).unwrap() = d;
        }
    }
}

#[test]
fn a_bowl_keeps_its_water_and_lies_level() {
    let mut s = Scene::new(bowl(), still());
    pour(&mut s, 15..25, 15..25, 0.5);
    let v0 = s.w.total_volume();
    s.run(2.0);
    assert!((s.w.total_volume() - v0).abs() < 1e-3 * v0);
    let levels: Vec<f32> = (0..N).flat_map(|y| (0..N).map(move |x| (x, y))).filter(|&(x, y)| s.depth(x, y) > 1e-3).map(|(x, y)| s.level(x, y)).collect();
    let (lo, hi) = levels.iter().fold((f32::MAX, f32::MIN), |(a, b), &l| (a.min(l), b.max(l)));
    assert!(hi - lo < 1e-3, "the surface is level: {lo} to {hi}");
}

#[test]
fn dry_land_stays_dry_and_water_never_climbs_a_wall() {
    let mut s = Scene::new(valley(0.02), still());
    s.run(1.0);
    assert_eq!(s.w.total_volume(), 0.0);
    let mut s = Scene::new(grid(|x, _| if x == 20 { 20.0 } else { 1.0 + 0.01 * x as f32 }), WaterConfig { open_edges: false, ..still() });
    s.source(50.0);
    s.run(3.0);
    assert!((21..N).all(|x| (0..N).all(|y| s.depth(x, y) == 0.0)), "water got over the wall");
}

#[test]
fn a_river_carries_off_exactly_what_flows_into_it_and_deepens_with_more() {
    let mut depths = Vec::new();
    for rate in [20.0f32, 80.0, 320.0] {
        let mut s = Scene::new(valley(0.02), still());
        s.source(rate);
        s.run(1.0);
        let out = s.out_per_day(2.0);
        assert!((out - rate as f64).abs() < 1e-3 * rate as f64, "{rate} in, {out} out");
        assert!(s.deepest_across(N - 2) > 0.0, "the river reaches the map edge");
        depths.push(s.deepest_across(MID));
    }
    assert!(depths[0] < depths[1] && depths[1] < depths[2], "{depths:?}");
}

#[test]
fn a_stream_is_shallower_on_a_steep_reach_but_only_as_manning_says() {
    let (q, w) = (0.02, 1.0);
    let (steep, flat) = (manning_depth(q, w, 0.1, MANNING_CHANNEL), manning_depth(q, w, 0.002, MANNING_CHANNEL));
    assert!((steep / flat - 50f32.powf(-0.3)).abs() < 1e-3, "{steep} vs {flat}");
    assert!(steep > 0.02, "a 20 l/s stream on a 10% slope still runs {steep} m deep");
}

#[test]
fn a_dam_fills_its_pond_in_the_time_the_inflow_takes_then_passes_it_on() {
    let rate = 40.0;
    let mut s = dammed(1.5, still());
    s.source(rate);
    let days = (capacity(&s) / rate as f64) as f32;
    let held = s.out_per_day(0.8 * days);
    assert!(held < 0.05 * rate as f64, "the pond holds the river while it fills: {held}");
    s.run(0.4 * days);
    let crest = s.h.get(DAM_X, MID).unwrap() + 1.5;
    assert!((s.level(DAM_X - 1, MID) - crest).abs() < 0.05, "full to the crest: {} vs {crest}", s.level(DAM_X - 1, MID));
    let out = s.out_per_day(1.0);
    assert!((out - rate as f64).abs() < 0.02 * rate as f64, "a full pond passes its river on: {out}");
}

#[test]
fn a_dam_that_does_not_reach_across_the_valley_leaks_round_its_end() {
    let mut s = Scene::new(valley(0.02), still());
    let cells: Vec<(usize, usize)> = (MID - 1..=MID + 1).map(|y| (DAM_X, y)).collect();
    s.w.build_dam(&cells, 3.0).unwrap();
    s.source(40.0);
    s.run(1.0);
    assert!(s.out_per_day(1.0) > 39.0);
    assert!(s.level(DAM_X - 1, MID) < s.h.get(DAM_X, MID).unwrap() + 1.0);
}

#[test]
fn raising_a_dam_raises_its_pond_and_a_breach_drains_it_in_a_flood_keeping_the_books() {
    let mut s = dammed(0.7, still());
    s.source(40.0);
    s.run(10.0);
    let low = s.level(DAM_X - 1, MID);
    assert!((low - (s.h.get(DAM_X, MID).unwrap() + 0.7)).abs() < 0.05, "{low}");
    let cells: Vec<(usize, usize)> = (0..N).map(|y| (DAM_X, y)).collect();
    s.w.build_dam(&cells, 1.5).unwrap();
    s.run(15.0);
    assert!(s.level(DAM_X - 1, MID) > low + 0.7);
    let v0 = s.w.total_volume();
    let (a0, l0) = (s.w.added_volume, s.w.lost_volume);
    for y in 0..N {
        s.w.breach(DAM_X, y).unwrap();
    }
    let flood = s.out_per_day(1.0);
    assert!(flood > 3.0 * 40.0, "the breach sends a flood downstream: {flood}");
    s.run(5.0);
    assert!(s.depth(DAM_X - 1, MID) < 0.5);
    let gap = s.w.total_volume() - (v0 + s.w.added_volume - a0 - (s.w.lost_volume - l0));
    assert!(gap.abs() < 1e-3 * v0, "books: {gap}");
}

#[test]
fn a_canal_lowers_a_reservoir_to_its_floor_and_no_further() {
    let mut s = Scene::new(grid(|x, y| if (5..18).contains(&x) && (10..30).contains(&y) { 6.0 } else { 10.0 }), still());
    pour(&mut s, 5..18, 10..30, 3.5);
    s.run(0.2);
    let cut: Vec<(usize, usize)> = (18..N).map(|x| (x, 20)).collect();
    s.w.dig_canal(&mut s.h, &cut, 2.5).unwrap();
    s.run(5.0);
    assert!((s.level(10, 20) - 7.5).abs() < 0.05, "down to the canal floor: {}", s.level(10, 20));
}

#[test]
fn a_still_pond_loses_evaporation_and_seepage_at_their_rates_and_a_fed_one_does_not() {
    let pit = |fed: bool| {
        let h = grid(|x, y| if (10..20).contains(&x) && (10..20).contains(&y) { 1.0 } else { 5.0 });
        let mut s = Scene::new(h, WaterConfig { rain: 0.0, ..WaterConfig::default() });
        pour(&mut s, 10..20, 10..20, 1.0);
        for y in 10..20 {
            for x in 10..20 {
                *s.w.fed.get_mut(x, y).unwrap() = fed;
            }
        }
        s.run(5.0);
        s.w.total_volume()
    };
    let cfg = WaterConfig::default();
    let rate = cfg.evaporation * crate::water::Season::Spring.evap_mult() + cfg.seepage;
    let per_cell_day = (100.0 - pit(false)) / 5.0 / 100.0;
    assert!((per_cell_day - rate as f64).abs() < 0.02 * rate as f64, "{per_cell_day} vs {rate}");
    assert!((pit(true) - 100.0).abs() < 1e-3);
}

#[test]
fn only_floods_wear_a_dam_not_its_dry_banks_or_ordinary_spill() {
    let wear = |rate: f32| {
        let mut s = dammed(1.5, WaterConfig { overtop_erosion: crate::water::OVERTOP_EROSION, ..still() });
        s.source(rate);
        s.run(30.0);
        (0..N).map(|y| 1.0 - *s.w.dam_integrity.get(DAM_X, y).unwrap()).fold(0.0f32, f32::max)
    };
    assert_eq!(wear(200.0), 0.0);
    assert!(wear(400_000.0) > 0.5, "a torrent over the crest wears it");
}

#[test]
fn ice_forms_on_water_in_winter_and_melts_in_spring() {
    let mut s = Scene::new(bowl(), still());
    pour(&mut s, 15..25, 15..25, 0.5);
    s.run(3.0 * crate::water::SEASON_DAYS as f32 + 10.0);
    assert!(*s.w.ice.get(20, 20).unwrap() > 0.5);
    s.run(crate::water::SEASON_DAYS as f32 + 10.0);
    assert_eq!(*s.w.ice.get(20, 20).unwrap(), 0.0);
}

#[test]
fn a_lake_against_a_flat_map_edge_runs_off_it_and_one_against_rising_ground_stays() {
    let lake = |edge: f32| {
        let inside = |x: usize, y: usize| x < 10 && (10..30).contains(&y);
        let mut s = Scene::new(grid(|x, y| if !inside(x, y) { 5.0 } else if x == 0 { edge } else { 1.0 }), still());
        pour(&mut s, 1..10, 10..30, 1.0);
        s.run(2.0);
        s.w.total_volume()
    };
    assert!(lake(1.0) < 1.0);
    assert!(lake(3.0) > 170.0);
}

#[test]
fn a_finished_dam_holds_the_pond_the_preview_promised() {
    let mut s = crate::SimState::new(1);
    s.height = valley(0.02);
    s.water = Water::new(N);
    s.water.config = still();
    s.ecology.vegetation = crate::grid::Grid::new(N, N, 0.0);
    let cells: Vec<(usize, usize)> = (0..N).map(|y| (DAM_X, y)).collect();
    let p = crate::water::preview_dam(&s, &cells, 1.5).unwrap();
    s.water.build_dam(&cells, 1.5).unwrap();
    s.water.add_source(1, MID, 400.0).unwrap();
    for _ in 0..(5.0 / DT) as usize {
        s.water.step(&s.height, DT);
    }
    let mut wet: Vec<(usize, usize)> = (0..N).flat_map(|y| (0..DAM_X).map(move |x| (x, y))).filter(|&(x, y)| *s.water.depth.get(x, y).unwrap() > 0.01).collect();
    let mut want = p.flooded_cells.clone();
    wet.sort();
    want.sort();
    assert_eq!(wet, want);
}

#[test]
fn on_every_map_the_rivers_run_wet_from_where_they_enter_to_the_edge_and_in_equals_out() {
    for info in crate::maps::all() {
        let (s, _) = crate::starts::new_game_on_map(&info.id).unwrap();
        let mut w = s.water.clone();
        for _ in 0..(2.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        w.reset_books();
        let v0 = w.total_volume();
        for _ in 0..(2.0 / STEP_DAYS) as usize {
            w.step(&s.height, STEP_DAYS);
        }
        let b = w.budget();
        assert!((b.added() - b.lost() - (w.total_volume() - v0)).abs() < 1e-3 * b.added(), "{}: books {b:?}", info.id);
        assert!((b.edges - b.sources).abs() < 0.01 * b.sources, "{}: in {} out {}", info.id, b.sources, b.edges);
        let n = s.world.size;
        let flowing = |i: usize| w.depth.as_slice()[i] >= crate::water::WET_EPS && w.width.as_slice()[i] > 0.0;
        for i in (0..n * n).filter(|&i| s.world.river.as_slice()[i]) {
            assert!(flowing(i) || crate::water::neighbours8(n, i).any(flowing), "{}: no water at or beside river cell ({}, {})", info.id, i % n, i / n);
        }
    }
}
