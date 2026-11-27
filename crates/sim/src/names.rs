use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::buildings::BuildingId;
use crate::colony::{BeaverId, FamilyId};
use crate::trees::Tree;
use crate::water::Cell;
use crate::SimState;

pub fn pool(key: &str) -> &'static [&'static str] {
    crate::content::content().names.get(key).map_or(&[], |w| w.as_slice())
}

const THEMES: [&str; 8] = ["water", "stone", "sky", "wood", "shore", "creature", "light", "flora"];

const SITE_REACH: i64 = 6;
const POND_REACH: i64 = 2;
const POND_SAMPLE: usize = 48;

struct Draw(u64);

impl Draw {
    fn new(seed: u64, salt: u64) -> Self {
        let mut d = Draw(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        d.next();
        d
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> Option<T> {
        (!xs.is_empty()).then(|| xs[self.below(xs.len())])
    }
    fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            xs.swap(i, self.below(i + 1));
        }
    }
}

pub fn region(map: &str) -> &'static str {
    match map {
        m if m.starts_with("adirondacks") => "adirondacks",
        m if m.starts_with("algonquin") => "ontario",
        m if m.starts_with("voyageurs") => "minnesota",
        m if m.starts_with("bavarian") => "bavaria",
        m if m.starts_with("telemark") || m.starts_with("agder") => "norway",
        _ => "default",
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pond {
    pub dam: Option<BuildingId>,
    pub name: String,
    cells: Vec<Cell>,
}

impl Pond {
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Names {
    pub ponds: Vec<Pond>,
    day: i64,
}

pub fn pond_name(sim: &SimState, dam: BuildingId) -> Option<&str> {
    sim.names.ponds.iter().find(|p| p.dam == Some(dam)).map(|p| p.name.as_str())
}

pub fn step(sim: &mut SimState) {
    if sim.colony.beavers.iter().any(|b| b.name.is_empty()) {
        name_beavers(sim);
    }
    if sim.colony.families.iter().any(|f| f.name.is_empty()) {
        name_families(sim);
    }
    let day = sim.time_days.floor() as i64;
    if day != sim.names.day {
        sim.names.day = day;
        name_ponds(sim);
    }
}

fn theme_words(theme: &str, map: &str) -> Vec<&'static str> {
    if theme != "flora" {
        return pool(&format!("litter/{theme}")).to_vec();
    }
    let trees = crate::trees::flora(map);
    let plants = crate::plants::flora(map);
    let mut out: Vec<&'static str> = Tree::all().zip(trees.iter())
        .filter(|(_, m)| m.dry > 0.0 || m.wet > 0.0 || m.upland > 0.0 || m.open > 0.0)
        .flat_map(|(t, _)| pool(&format!("flora/{t:?}")).iter().copied())
        .collect();
    out.extend(crate::plants::Plant::all().filter(|p| plants[p.index()] > 0.0).flat_map(|p| pool(&format!("flora/{p:?}")).iter().copied()));
    out
}

fn theme_of(name: &str, map: &str) -> Option<&'static str> {
    THEMES.into_iter().find(|t| theme_words(t, map).contains(&name))
}

fn name_beavers(sim: &mut SimState) {
    let map = sim.resources.flora.clone();
    let mut taken: BTreeSet<String> = sim.colony.beavers.iter().filter(|b| !b.name.is_empty()).map(|b| b.name.clone()).collect();
    let mut litters: BTreeMap<(u64, u64), Vec<BeaverId>> = BTreeMap::new();
    for b in sim.colony.beavers.iter().filter(|b| b.name.is_empty()) {
        let key = match b.mother { Some(m) => (m.0, 0), None => (u64::MAX, b.id.0) };
        litters.entry(key).or_default().push(b.id);
    }
    for ((mother, _), kits) in litters {
        let first = kits.iter().map(|k| k.0).min().unwrap_or(0);
        let mut d = Draw::new(sim.seed, first ^ 0x4B17);
        let last = (mother != u64::MAX).then(|| {
            sim.colony.beavers.iter()
                .filter(|b| b.mother == Some(BeaverId(mother)) && !b.name.is_empty())
                .min_by(|a, b| a.age_days.total_cmp(&b.age_days))
                .and_then(|b| theme_of(&b.name, &map))
        }).flatten();
        let mut themes: Vec<&str> = THEMES.into_iter().filter(|t| Some(*t) != last).collect();
        d.shuffle(&mut themes);
        for id in kits {
            let name = draw_name(&mut d, &themes, &map, &taken);
            taken.insert(name.clone());
            if let Some(b) = sim.colony.beavers.iter_mut().find(|b| b.id == id) {
                b.name = name;
            }
        }
    }
}

fn draw_name(d: &mut Draw, themes: &[&str], map: &str, taken: &BTreeSet<String>) -> String {
    for theme in themes {
        let free: Vec<&str> = theme_words(theme, map).into_iter().filter(|w| !taken.contains(*w)).collect();
        if let Some(w) = d.pick(&free) {
            return w.to_string();
        }
    }
    let all: Vec<&str> = themes.iter().flat_map(|t| theme_words(t, map)).collect();
    for _ in 0..64 {
        let w = format!("Little {}", d.pick(&all).unwrap_or("Kit"));
        if !taken.contains(&w) {
            return w;
        }
    }
    format!("Kit {}", d.next() % 1000)
}

#[derive(Default)]
struct Site {
    trees: BTreeMap<Tree, u32>,
    felled: BTreeMap<Tree, u32>,
    plants: BTreeMap<crate::plants::Plant, u32>,
    boulders: u32,
    pebbles: u32,
    mud: u32,
    deep: u32,
    lake: u32,
    river: u32,
    relief: f32,
}

fn survey(sim: &SimState, at: Cell) -> Site {
    let mut s = Site::default();
    let near = |c: Cell| (c.0 as i64 - at.0 as i64).abs() <= SITE_REACH && (c.1 as i64 - at.1 as i64).abs() <= SITE_REACH;
    for r in sim.resources.items().iter().filter(|r| near(r.cell)) {
        let roles = &crate::content::content().roles;
        match r.kind {
            k if k.is_tree() && !r.snag => *s.trees.entry(r.species.expect("a tree has a species")).or_default() += 1,
            k if k.is_felled() => *s.felled.entry(r.species.expect("a tree has a species")).or_default() += 1,
            k if k == roles.outcrop || k == roles.gravel => s.pebbles += 1,
            k if k == roles.bank_mud || k == roles.stream_mud => s.mud += 1,
            _ => {}
        }
        if let Some(p) = r.plant {
            *s.plants.entry(p).or_default() += 1;
        }
    }
    s.boulders = sim.resources.boulders.iter().filter(|b| near(b.cell)).count() as u32;
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let (w, h) = (sim.height.width() as i64, sim.height.height() as i64);
    for dy in -SITE_REACH..=SITE_REACH {
        for dx in -SITE_REACH..=SITE_REACH {
            let (x, y) = (at.0 as i64 + dx, at.1 as i64 + dy);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let (x, y) = (x as usize, y as usize);
            if let Some(&g) = sim.height.get(x, y) {
                lo = lo.min(g);
                hi = hi.max(g);
            }
            if sim.water.depth.get(x, y).is_some_and(|&d| d > 0.8) {
                s.deep += 1;
            }
            if sim.world.lake.get(x, y).copied().unwrap_or(false) {
                s.lake += 1;
            }
            if sim.world.river.get(x, y).copied().unwrap_or(false) {
                s.river += 1;
            }
        }
    }
    s.relief = if hi > lo { hi - lo } else { 0.0 };
    s
}

fn setting(s: &Site, cell_m: f32) -> &'static str {
    if s.lake > s.river.max(4) || s.deep > 30 {
        "still"
    } else if s.river > 0 {
        "stream"
    } else if s.relief > 0.6 * cell_m * SITE_REACH as f32 * 0.25 {
        "slope"
    } else {
        "wet"
    }
}

fn candidates(sim: &SimState, s: &Site, d: &mut Draw) -> Vec<String> {
    let region = region(&sim.resources.flora);
    let land = |d: &mut Draw| d.pick(pool(&format!("landform/{region}/{}", setting(s, sim.water.cell_m)))).unwrap_or("Pond");
    let most = |m: &BTreeMap<Tree, u32>| m.iter().max_by_key(|(t, n)| (**n, std::cmp::Reverse(**t))).map(|(t, _)| *t);
    let mut out = Vec::new();
    if let Some(t) = most(&s.trees) {
        out.push(format!("{} {}", t.name(), land(d)));
        if let Some(suffix) = d.pick(pool(&format!("compound/{region}"))) {
            out.push(format!("{}{suffix}", t.name()));
        }
    }
    for (t, n) in &s.trees {
        match *n {
            1 => if let Some(w) = d.pick(pool("feature/lone")) { out.push(format!("{w} {}", t.name())) },
            2..=5 => if let Some(w) = pool("count").get(*n as usize - 2) { out.push(format!("{w} {}", t.name())) },
            _ => {}
        }
    }
    if let (Some(t), Some(w)) = (most(&s.felled), d.pick(pool("feature/felled"))) {
        out.push(format!("{w} {}", t.name()));
    }
    if s.boulders > 0 {
        if let Some(w) = d.pick(pool("feature/boulder")) { out.push(format!("{w} {}", land(d))) }
    }
    if s.pebbles > 0 {
        if let Some(w) = d.pick(pool("feature/gravel")) { out.push(format!("{w} {}", land(d))) }
    }
    if let Some((p, _)) = s.plants.iter().max_by_key(|(p, n)| (**n, std::cmp::Reverse(**p))) {
        if let Some(w) = pool(&format!("flora/{p:?}")).first() { out.push(format!("{w} {}", land(d))) }
    }
    let pale = most(&s.trees).is_some_and(|t| t.row().pale);
    let colours = [(s.deep > 12, "deep"), (s.boulders + s.pebbles > 2, "stone"), (s.mud > 0, "mud"), (s.plants.values().sum::<u32>() > 12, "green"), (pale, "pale")];
    for (_, c) in colours.into_iter().filter(|(on, _)| *on) {
        if let Some(w) = d.pick(pool(&format!("colour/{c}"))) { out.push(format!("{w} {}", land(d))) }
    }
    if out.is_empty() {
        out.push(land(d).to_string());
    }
    d.shuffle(&mut out);
    out
}

fn place_name(sim: &SimState, at: Cell, salt: u64, taken: &BTreeSet<String>) -> String {
    let mut d = Draw::new(sim.seed, salt ^ 0xFA11);
    let site = survey(sim, at);
    let options = candidates(sim, &site, &mut d);
    if let Some(n) = options.iter().find(|n| !taken.contains(*n)) {
        return n.clone();
    }
    for n in &options {
        for p in pool("position") {
            let named = format!("{p} {n}");
            if !taken.contains(&named) {
                return named;
            }
        }
    }
    format!("{} {}", options[0], d.next() % 100)
}

fn name_families(sim: &mut SimState) {
    let mut taken: BTreeSet<String> = sim.colony.families.iter().filter(|f| !f.name.is_empty()).map(|f| f.name.clone()).collect();
    let unnamed: Vec<FamilyId> = sim.colony.families.iter().filter(|f| f.name.is_empty()).map(|f| f.id).collect();
    for id in unnamed {
        let at = sim.buildings.buildings.iter().find(|b| b.is_home_ready() && sim.colony.lodge_family(b.id) == Some(id)).map(|b| b.centre())
            .or_else(|| sim.colony.beavers.iter().find(|b| b.family == id).map(|b| b.pos))
            .unwrap_or((0.0, 0.0));
        let name = place_name(sim, (at.0.max(0.0) as usize, at.1.max(0.0) as usize), id.0, &taken);
        taken.insert(name.clone());
        if let Some(f) = sim.colony.families.iter_mut().find(|f| f.id == id) {
            f.name = name;
        }
    }
}

fn name_ponds(sim: &mut SimState) {
    let standing: HashSet<BuildingId> = sim.buildings.buildings.iter().map(|b| b.id).collect();
    for p in sim.names.ponds.iter_mut().filter(|p| p.dam.is_some_and(|d| !standing.contains(&d))) {
        p.dam = None;
    }
    let held: Vec<(BuildingId, Vec<Cell>)> = sim.buildings.buildings.iter()
        .filter(|b| !sim.names.ponds.iter().any(|p| p.dam == Some(b.id)))
        .filter_map(|b| Some((b.id, b.dam.as_deref()?.pond.as_ref()?.flooded.clone())))
        .filter(|(_, c)| !c.is_empty())
        .collect();
    for (dam, cells) in held {
        let wet: HashSet<Cell> = cells.iter().copied().collect();
        let known = sim.names.ponds.iter().position(|p| p.cells.iter().any(|c| wet.contains(c)));
        let sample: Vec<Cell> = cells.iter().step_by((cells.len() / POND_SAMPLE).max(1)).copied().take(POND_SAMPLE).collect();
        if let Some(i) = known {
            if sim.names.ponds[i].dam.is_none() {
                sim.names.ponds[i].dam = Some(dam);
                sim.names.ponds[i].cells = sample;
            } else {
                let name = sim.names.ponds[i].name.clone();
                sim.names.ponds.push(Pond { dam: Some(dam), name, cells: sample });
            }
            continue;
        }
        let on = |pos: (f32, f32)| {
            let (x, y) = (pos.0 as i64, pos.1 as i64);
            (-POND_REACH..=POND_REACH).any(|dy| (-POND_REACH..=POND_REACH).any(|dx| {
                let (cx, cy) = (x + dx, y + dy);
                cx >= 0 && cy >= 0 && wet.contains(&(cx as usize, cy as usize))
            }))
        };
        let first = sim.buildings.buildings.iter()
            .filter(|b| b.is_home_ready() && on(b.centre()))
            .filter_map(|b| Some((b.id, sim.colony.lodge_family(b.id)?)))
            .min_by_key(|&(id, fam)| (id, fam)).map(|(_, fam)| fam);
        if let Some(f) = first.and_then(|f| sim.colony.family(f)).filter(|f| !f.name.is_empty()) {
            let name = f.name.clone();
            sim.names.ponds.push(Pond { dam: Some(dam), name, cells: sample });
        }
    }
}
