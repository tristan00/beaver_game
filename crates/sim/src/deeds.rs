//! What each beaver has done in its life, kept for its page: how far it walked and swam; the
//! trees it felled, by species, and the great old ones among them; the plants it ate from, and
//! what it found in winter; what it gathered, and where it carried it, on foot or by raft;
//! each dam, canal and building it worked on and what it put into each; the leaks it stopped;
//! the saplings it planted and those that grew into trees; each wolf it spotted; the days it
//! gave each job. Kept for the dead too.
//!
//! And its epithet, its one byname, always "the" and a word or a few: "the Muddy". The first
//! kind comes with a skill nearly mastered, the word from the job and how it was done (a hauler
//! who rafted is "the Rafter", one on foot "the Strong"). The greater kind is rare: a beaver
//! with a skill mastered and a good share of the others learnt besides is named for its
//! greatest deed — "the Keeper of Birch Tarn", "the Aspenbane", "the Icediver". Each epithet
//! is held by one living beaver at a time, and one is never taken back or swapped for a lesser
//! one. They are given at the turn of a season. Saved after the world (see `save`).
use std::collections::{BTreeMap, BTreeSet};

use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::colony::{BeaverId, Event, Job, Stage};
use crate::plants::Plant;
use crate::resources::ResourceId;
use crate::trees::Tree;
use crate::SimState;

include!(concat!(env!("OUT_DIR"), "/deeds_constants.rs"));

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Epithet {
    pub word: String,
    /// The job it was earned in, or the one the deed belongs to.
    pub job: Job,
    /// 1, or 2 for the greater kind.
    pub tier: u8,
    pub day: f64,
    /// The epithet it replaced, when it is the greater kind.
    pub was: Option<String>,
}

/// What a beaver put into one dam, canal or building.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub kind: Option<BuildingKind>,
    /// Building work put in as new work.
    pub labour: f32,
    /// Loads packed into a dam, cells dug of a canal, gates set.
    pub loads: u32,
    /// Repair work on it.
    pub repairs: f32,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// Cells walked and swum.
    pub walked: f32,
    pub swum: f32,
    pub felled: BTreeMap<Tree, u32>,
    /// Trees felled that were great and old (`GREAT_TREE_DAYS`).
    pub great_trees: u32,
    /// Food taken from each plant species, from bark, and in winter.
    pub harvested: BTreeMap<Plant, f32>,
    pub bark: f32,
    pub winter_food: f32,
    /// Goods gathered from the land, by good.
    pub gathered: [f32; Good::ALL.len()],
    /// Goods carried on foot, by good and the kind of place they went to (`Dam` for sticks
    /// packed into a dam).
    pub carried: BTreeMap<(Good, BuildingKind), f32>,
    /// Of that, stock moved out of stores (hauling, not gathering).
    pub hauled: f32,
    /// Goods moved by raft, by good.
    pub rafted: [f32; Good::ALL.len()],
    /// Each dam, canal and building it worked on.
    pub works: BTreeMap<BuildingId, Work>,
    /// Buildings it worked on that were finished, by kind.
    pub built: BTreeMap<BuildingKind, u32>,
    /// Loads packed into dams, canal cells dug and gates set, as new work.
    pub dam_loads: u32,
    pub canal_cells: u32,
    pub gates: u32,
    /// Repair work, and the dam cells it repaired when they were near failing.
    pub mended: f32,
    pub leaks: u32,
    pub planted: u32,
    /// Its planted saplings that have grown into mature trees.
    pub grown: u32,
    /// Seedlings collected from seeding trees.
    pub seedlings: u32,
    /// Wolves spotted, each wolf once, and the beavers out in its lookout's range when it first
    /// slapped the alarm for each.
    pub wolves: u32,
    pub warned: u32,
    /// Days spent at each job (`Job::ALL` order).
    pub days: [f32; Job::ALL.len()],
    pub epithet: Option<Epithet>,
}

impl Record {
    /// Food gathered from plants that grow in open water.
    pub fn water_food(&self) -> f32 {
        self.harvested.iter().filter(|(p, _)| p.aquatic()).map(|(_, f)| f).sum()
    }
    fn dam_labour(&self) -> f32 {
        self.works.values().filter(|w| w.kind.is_some_and(|k| k.is_waterworks())).map(|w| w.labour).sum()
    }
    fn labour(&self) -> f32 {
        self.works.values().map(|w| w.labour).sum()
    }
    pub fn rafted_total(&self) -> f32 {
        self.rafted.iter().sum()
    }
    /// Goods carried on foot to a kind of place.
    pub fn carried_to(&self, goods: &[Good], kinds: &[BuildingKind]) -> f32 {
        self.carried.iter().filter(|((g, k), _)| goods.contains(g) && kinds.contains(k)).map(|(_, v)| v).sum()
    }
    pub(crate) fn work(&mut self, at: BuildingId, kind: Option<BuildingKind>) -> &mut Work {
        let w = self.works.entry(at).or_default();
        if w.kind.is_none() {
            w.kind = kind;
        }
        w
    }
}

fn key(job: Job) -> &'static str {
    match job {
        Job::Forager => "forager",
        Job::Feller => "feller",
        Job::Builder => "builder",
        Job::Digger => "digger",
        Job::Hauler => "hauler",
        Job::Planter => "planter",
        Job::Mender => "mender",
        Job::Sentry => "sentry",
        Job::Pebbler => "pebbler",
    }
}

/// How the work was done, where the first kind's words tell it apart.
fn variant(r: &Record, job: Job) -> Option<&'static str> {
    match job {
        Job::Hauler if r.rafted_total() > r.hauled => Some("raft"),
        Job::Builder if r.dam_labour() * 2.0 > r.labour() => Some("dam"),
        Job::Forager if r.water_food() * 2.0 > r.gathered[Good::Food.index()] => Some("water"),
        _ => None,
    }
}

/// The first kind's words for a job, by how the work was done.
fn words(r: &Record, job: Job) -> &'static [&'static str] {
    let job_key = key(job);
    variant(r, job)
        .map(|v| crate::names::pool(&format!("epithet/{job_key}/{v}")))
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| crate::names::pool(&format!("epithet/{job_key}")))
}

/// Kits a beaver had, and those of them that grew up.
pub fn kits(sim: &SimState, id: BeaverId) -> (u32, u32) {
    let mut born = 0;
    let mut raised = 0;
    for (kit, (f, m)) in &sim.colony.ancestry {
        if *f != Some(id) && *m != Some(id) {
            continue;
        }
        born += 1;
        let grown_here = sim.colony.beaver(*kit).is_some_and(|b| b.stage == Stage::Adult);
        let grew = sim.lives.of.get(kit).is_some_and(|l| l.entries.iter().any(|e| e.what == crate::lives::What::Grew(Stage::Adult)));
        if grown_here || grew {
            raised += 1;
        }
    }
    (born, raised)
}

/// A deed past its mark: how far past (1 at the mark), the job it belongs to, and the names it
/// may give, best first.
struct Deed {
    past: f32,
    job: Option<Job>,
    names: Vec<String>,
}

fn pool_names(key: &str) -> Vec<String> {
    crate::names::pool(&format!("deed/{key}")).iter().map(|s| s.to_string()).collect()
}

/// Every deed of a beaver's that is past its mark.
fn deeds(sim: &SimState, id: BeaverId, r: &Record) -> Vec<Deed> {
    let mut out = Vec::new();
    let mut add = |value: f32, mark: f32, job: Option<Job>, names: Vec<String>| {
        if mark > 0.0 && value >= mark && !names.is_empty() {
            out.push(Deed { past: value / mark, job, names });
        }
    };
    add(r.dam_loads as f32, DAM_LOADS_MARK, Some(Job::Builder), pool_names("dam"));
    // Most of a dam whose pond has a name: its keeper.
    for (dam, w) in r.works.iter().filter(|(_, w)| w.kind == Some(BuildingKind::Dam)) {
        let total = sim.deeds.dam_loads.get(dam).copied().unwrap_or(0).max(1);
        let pond = sim.buildings.building(*dam).and_then(|b| b.dam.as_deref()?.pond.as_ref()).map_or(0, |p| p.area);
        if let Some(name) = crate::names::pond_name(sim, *dam).filter(|_| w.loads * 2 >= total && pond >= KEEPER_POND_CELLS as usize) {
            add(w.loads as f32, KEEPER_LOADS_MARK, Some(Job::Builder), vec![format!("the Keeper of {name}")]);
        }
    }
    add(r.canal_cells as f32, CANAL_CELLS_MARK, Some(Job::Builder), pool_names("canal"));
    add(*r.built.get(&BuildingKind::Lodge).unwrap_or(&0) as f32, LODGES_MARK, Some(Job::Builder), pool_names("lodges"));
    add(r.leaks as f32, LEAKS_MARK, Some(Job::Mender), pool_names("leaks"));
    if let Some((t, n)) = r.felled.iter().max_by_key(|(t, n)| (**n, std::cmp::Reverse(**t))) {
        add(*n as f32, SPECIES_FELLED_MARK, Some(Job::Feller), vec![format!("the {}bane", t.name()), format!("the {}gnawer", t.name())]);
    }
    add(r.great_trees as f32, GREAT_TREES_MARK, Some(Job::Feller), pool_names("great_trees"));
    add(r.water_food(), WATER_FOOD_MARK, Some(Job::Forager), pool_names("water_food"));
    if let Some((p, f)) = r.harvested.iter().max_by(|a, b| a.1.total_cmp(b.1)) {
        add(*f, PLANT_FOOD_MARK, Some(Job::Forager), pool_names(&format!("plant/{p:?}")));
    }
    add(r.winter_food, WINTER_FOOD_MARK, Some(Job::Forager), pool_names("winter_food"));
    add(r.carried_to(&[Good::Sticks, Good::Mud], &[BuildingKind::Dam, BuildingKind::Canal]), TO_DAMS_MARK, Some(Job::Hauler), pool_names("to_dams"));
    add(r.carried_to(&[Good::Food], &[BuildingKind::FoodCache]), TO_CACHE_MARK, Some(Job::Hauler), pool_names("to_cache"));
    add(r.rafted_total(), RAFTED_MARK, Some(Job::Hauler), pool_names("rafted"));
    add(r.swum, SWUM_MARK, None, pool_names("swum"));
    add(r.grown as f32, GROWN_MARK, Some(Job::Planter), pool_names("grown"));
    add(r.wolves as f32, WOLVES_MARK, Some(Job::Sentry), pool_names("wolves"));
    add(kits(sim, id).1 as f32, KITS_RAISED_MARK, None, pool_names("kits"));
    out.sort_by(|a, b| b.past.total_cmp(&a.past));
    out
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Deeds {
    pub of: BTreeMap<BeaverId, Record>,
    /// Who has worked on each unfinished building, credited when it is finished.
    crew: BTreeMap<BuildingId, BTreeSet<BeaverId>>,
    /// Loads packed into each dam by everyone, to tell whose dam it mostly is.
    pub dam_loads: BTreeMap<BuildingId, u32>,
    /// Wolves each beaver has spotted (by the wolf's key), each counted once.
    spotted: BTreeSet<(BeaverId, u64)>,
    /// Planted trees already counted as grown.
    grown: BTreeSet<ResourceId>,
    /// The season epithets were last given in (seasons since the start).
    season: i64,
}

impl Deeds {
    pub fn get(&self, id: BeaverId) -> Option<&Record> {
        self.of.get(&id)
    }
    pub(crate) fn of(&mut self, id: BeaverId) -> &mut Record {
        self.of.entry(id).or_default()
    }
    /// New building work put into a building, and its beaver on its crew.
    pub(crate) fn worked(&mut self, id: BeaverId, site: BuildingId, kind: BuildingKind, labour: f32) {
        self.of(id).work(site, Some(kind)).labour += labour;
        self.crew.entry(site).or_default().insert(id);
    }
    /// A load packed into a dam as new work.
    pub(crate) fn packed(&mut self, id: BeaverId, dam: BuildingId, labour: f32) {
        self.worked(id, dam, BuildingKind::Dam, labour);
        let r = self.of(id);
        r.dam_loads += 1;
        r.work(dam, Some(BuildingKind::Dam)).loads += 1;
        *self.dam_loads.entry(dam).or_default() += 1;
    }
    /// Repair work on a dam, canal or building.
    pub(crate) fn repaired(&mut self, id: BeaverId, at: BuildingId, kind: Option<BuildingKind>, labour: f32) {
        let r = self.of(id);
        r.mended += labour;
        r.work(at, kind).repairs += labour;
    }
    /// A wolf in a beaver's sight: counted the first time only (true then).
    pub(crate) fn spotted(&mut self, id: BeaverId, wolf: u64) -> bool {
        let new = self.spotted.insert((id, wolf));
        if new {
            self.of(id).wolves += 1;
        }
        new
    }
}

/// A beaver's name with its epithet: "Flint the Gnawer".
pub fn full_name(sim: &SimState, id: BeaverId, name: &str) -> String {
    match sim.deeds.get(id).and_then(|r| r.epithet.as_ref()) {
        Some(e) => format!("{name} {}", e.word),
        None => name.to_string(),
    }
}

/// Each step: days at each job; at a day's close, buildings finished credited to their crews
/// and planted trees grown credited to their planters; at a season's turn, epithets.
pub fn step(sim: &mut SimState, dt: f32) {
    for b in &sim.colony.beavers {
        if let Some(j) = b.current_job {
            sim.deeds.of.entry(b.id).or_default().days[j.index()] += dt;
        }
    }
    let new_day = (sim.time_days - dt as f64).floor() < sim.time_days.floor();
    if new_day {
        close_day(sim);
    }
    let season = (sim.time_days / crate::calendar::SEASON_DAYS).floor() as i64;
    if season != sim.deeds.season {
        sim.deeds.season = season;
        give_epithets(sim);
    }
}

fn close_day(sim: &mut SimState) {
    let crews: Vec<BuildingId> = sim.deeds.crew.keys().copied().collect();
    for site in crews {
        let Some(b) = sim.buildings.building(site) else {
            sim.deeds.crew.remove(&site);
            continue;
        };
        if sim.finished(b) {
            let kind = b.kind;
            for id in sim.deeds.crew.remove(&site).unwrap_or_default() {
                *sim.deeds.of(id).built.entry(kind).or_default() += 1;
            }
        }
    }
    let grown: Vec<(ResourceId, BeaverId)> = sim.forestry.planted.iter()
        .filter(|(tree, _)| !sim.deeds.grown.contains(tree))
        .filter(|(tree, _)| sim.resources.get(**tree).is_some_and(|r| r.kind == crate::resources::ResourceKind::Tree && r.age_days >= crate::forestry::MATURE_AGE as f32))
        .map(|(tree, (who, _))| (*tree, *who))
        .collect();
    for (tree, who) in grown {
        sim.deeds.grown.insert(tree);
        sim.deeds.of(who).grown += 1;
    }
}

/// Whether a beaver has learnt enough besides its best skill: the other skills' learning past
/// what every beaver has from the start and from growing up, as shares of mastery.
fn broad(skills: &[f32]) -> bool {
    let best = skills.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(0, |(i, _)| i);
    let rest: f32 = skills.iter().enumerate().filter(|(i, _)| *i != best).map(|(_, s)| (s - SKILL_FLOOR).max(0.0) / EPITHET_SKILL).sum();
    rest >= BREADTH
}

fn give_epithets(sim: &mut SimState) {
    let living: BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let mut held: BTreeSet<String> = sim.deeds.of.iter().filter(|(id, _)| living.contains(id)).filter_map(|(_, r)| r.epithet.as_ref().map(|e| e.word.clone())).collect();
    let mut earned = Vec::new();
    for b in &sim.colony.beavers {
        let Some(r) = sim.deeds.get(b.id) else { continue };
        let was = r.epithet.as_ref();
        if was.is_some_and(|e| e.tier >= 2) {
            continue;
        }
        let Some(best) = Job::ALL.into_iter().filter(|j| b.skills[j.index()] >= EPITHET_SKILL).max_by(|a, c| b.skills[a.index()].total_cmp(&b.skills[c.index()])) else { continue };
        // Which free name: the same each time for the same beaver.
        let choose = |names: &[String]| -> Option<String> {
            let free: Vec<&String> = names.iter().filter(|w| !held.contains(*w)).collect();
            free.get((b.id.0 as usize).wrapping_mul(2654435761) % free.len().max(1)).map(|w| (*w).clone())
        };
        // The greater kind: mastery, breadth besides, and a deed to be named for, the one
        // furthest past its mark that has a name free.
        let greater = if broad(&b.skills) {
            deeds(sim, b.id, r).into_iter().find_map(|d| choose(&d.names).map(|w| (w, d.job.unwrap_or(best))))
        } else {
            None
        };
        let pick = match (greater, was) {
            (Some((word, job)), _) => Some((2, word, job)),
            (None, None) => {
                let names: Vec<String> = words(r, best).iter().map(|s| s.to_string()).collect();
                choose(&names).map(|w| (1, w, best))
            }
            (None, Some(_)) => None,
        };
        let Some((tier, word, job)) = pick else { continue };
        held.insert(word.clone());
        earned.push((b.id, Epithet { word, job, tier, day: sim.time_days, was: was.map(|e| e.word.clone()) }));
    }
    for (id, e) in earned {
        sim.deeds.of(id).epithet = Some(e);
        sim.colony.note(Event::Epithet { id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_job_and_deed_has_its_names_in_one_form_and_no_name_is_in_two_pools() {
        for j in Job::ALL {
            assert!(!words(&Record::default(), j).is_empty(), "{j:?}");
        }
        for k in ["dam", "canal", "lodges", "leaks", "great_trees", "water_food", "winter_food", "to_dams", "to_cache", "rafted", "swum", "grown", "wolves", "kits"] {
            assert!(!pool_names(k).is_empty(), "{k}");
        }
        for p in Plant::ALL {
            assert!(!pool_names(&format!("plant/{p:?}")).is_empty(), "{p:?}");
        }
        let mut seen = BTreeSet::new();
        for (k, w) in crate::names::NAME_POOLS.iter().filter(|(k, _)| k.starts_with("epithet") || k.starts_with("deed")) {
            for x in w.iter() {
                assert!(x.starts_with("the "), "one form: {x}");
                assert!(seen.insert(*x), "{x} is in {k} and another pool");
            }
        }
    }

    #[test]
    fn how_the_work_was_done_picks_the_words() {
        let mut r = Record { hauled: 2.0, ..Default::default() };
        r.rafted[Good::Sticks.index()] = 10.0;
        assert!(words(&r, Job::Hauler).contains(&"the Rafter"));
        r.hauled = 20.0;
        assert!(words(&r, Job::Hauler).contains(&"the Strong"));
        r.work(BuildingId(1), Some(BuildingKind::Dam)).labour = 5.0;
        assert!(words(&r, Job::Builder).contains(&"the Weaver"));
        r.work(BuildingId(2), Some(BuildingKind::Lodge)).labour = 9.0;
        assert!(words(&r, Job::Builder).contains(&"the Heaper"));
    }

    #[test]
    fn breadth_counts_only_what_was_learnt_past_the_start() {
        // A master forager with every other skill at the start: not broad.
        let mut s = [0.1; Job::ALL.len()];
        s[0] = 0.98;
        assert!(!broad(&s));
        // Two other skills at a third each: past half a mastery besides.
        s[1] = 0.36;
        s[2] = 0.36;
        assert!(broad(&s));
    }

    #[test]
    fn a_wolf_is_counted_once_however_often_it_is_seen() {
        let mut d = Deeds::default();
        for _ in 0..50 {
            d.spotted(BeaverId(1), 7);
        }
        d.spotted(BeaverId(1), 8);
        d.spotted(BeaverId(2), 7);
        assert_eq!(d.get(BeaverId(1)).unwrap().wolves, 2);
        assert_eq!(d.get(BeaverId(2)).unwrap().wolves, 1);
    }

    #[test]
    fn whose_dam_it_mostly_is() {
        let mut d = Deeds::default();
        for _ in 0..6 {
            d.packed(BeaverId(1), BuildingId(9), 0.2);
        }
        for _ in 0..4 {
            d.packed(BeaverId(2), BuildingId(9), 0.2);
        }
        assert_eq!(d.dam_loads[&BuildingId(9)], 10);
        assert_eq!(d.get(BeaverId(1)).unwrap().works[&BuildingId(9)].loads, 6);
        assert_eq!(d.get(BeaverId(1)).unwrap().dam_loads, 6);
    }
}
