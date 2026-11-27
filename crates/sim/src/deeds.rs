use std::collections::{BTreeMap, BTreeSet};

use crate::buildings::{PerId, BuildingId, BuildingKind, Good};
use crate::colony::{BeaverId, Event, Job, Stage};
use crate::plants::Plant;
use crate::resources::ResourceId;
use crate::trees::Tree;
use crate::SimState;

include!(concat!(env!("OUT_DIR"), "/deeds_constants.rs"));

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Epithet {
    pub word: String,
    pub job: Job,
    pub tier: u8,
    pub day: f64,
    pub was: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub kind: Option<BuildingKind>,
    pub labour: f32,
    pub loads: u32,
    pub repairs: f32,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub walked: f32,
    pub swum: f32,
    pub felled: BTreeMap<Tree, u32>,
    pub great_trees: u32,
    pub harvested: BTreeMap<Plant, f32>,
    pub bark: f32,
    pub winter_food: f32,
    pub gathered: PerId<Good, f32>,
    pub carried: BTreeMap<(Good, BuildingKind), f32>,
    pub hauled: f32,
    pub rafted: PerId<Good, f32>,
    pub works: BTreeMap<BuildingId, Work>,
    pub built: BTreeMap<BuildingKind, u32>,
    pub dam_loads: u32,
    pub canal_cells: u32,
    pub gates: u32,
    pub mended: f32,
    pub leaks: u32,
    pub planted: u32,
    pub grown: u32,
    pub seedlings: u32,
    pub wolves: u32,
    pub warned: u32,
    pub days: PerId<Job, f32>,
    pub epithet: Option<Epithet>,
}

fn waterworks_materials() -> Vec<Good> {
    let w = &crate::content::content().works;
    let all = BuildingKind::dam().spec().cost.iter().chain(&BuildingKind::canal().spec().cost).chain(&w.gate).chain(&w.lock);
    Good::all().filter(|g| all.clone().any(|c| c.0 == *g)).collect()
}

impl Record {
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
        self.rafted.values().sum()
    }
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

fn key(job: Job) -> String {
    job.key().to_lowercase()
}

fn variant(r: &Record, job: Job) -> Option<&'static str> {
    use crate::content::Duty;
    let forages = crate::resources::ResourceKind::all().any(|k| k.is_food_plant() && k.row().role == job);
    match job.duty() {
        Duty::Haul if r.rafted_total() > r.hauled => Some("raft"),
        Duty::Build if r.dam_labour() * 2.0 > r.labour() => Some("dam"),
        Duty::Gather if forages && r.water_food() * 2.0 > r.gathered[Good::food()] => Some("water"),
        _ => None,
    }
}

fn words(r: &Record, job: Job) -> &'static [&'static str] {
    let job_key = key(job);
    variant(r, job)
        .map(|v| crate::names::pool(&format!("epithet/{job_key}/{v}")))
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| crate::names::pool(&format!("epithet/{job_key}")))
}

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

struct Deed {
    past: f32,
    job: Option<Job>,
    names: Vec<String>,
}

fn pool_names(key: &str) -> Vec<String> {
    crate::names::pool(&format!("deed/{key}")).iter().map(|s| s.to_string()).collect()
}

fn deeds(sim: &SimState, id: BeaverId, r: &Record) -> Vec<Deed> {
    let mut out = Vec::new();
    let mut add = |value: f32, mark: f32, job: Option<Job>, names: Vec<String>| {
        if mark > 0.0 && value >= mark && !names.is_empty() {
            out.push(Deed { past: value / mark, job, names });
        }
    };
    add(r.dam_loads as f32, DAM_LOADS_MARK, Some(Job::with_duty(crate::content::Duty::Build)), pool_names("dam"));
    for (dam, w) in r.works.iter().filter(|(_, w)| w.kind == Some(BuildingKind::dam())) {
        let total = sim.deeds.dam_loads.get(dam).copied().unwrap_or(0).max(1);
        let pond = sim.buildings.building(*dam).and_then(|b| b.dam.as_deref()?.pond.as_ref()).map_or(0, |p| p.area);
        if let Some(name) = crate::names::pond_name(sim, *dam).filter(|_| w.loads * 2 >= total && pond >= KEEPER_POND_CELLS as usize) {
            add(w.loads as f32, KEEPER_LOADS_MARK, Some(Job::with_duty(crate::content::Duty::Build)), vec![format!("the Keeper of {name}")]);
        }
    }
    add(r.canal_cells as f32, CANAL_CELLS_MARK, Some(Job::with_duty(crate::content::Duty::Build)), pool_names("canal"));
    add(*r.built.get(&BuildingKind::home()).unwrap_or(&0) as f32, LODGES_MARK, Some(Job::with_duty(crate::content::Duty::Build)), pool_names("lodges"));
    add(r.leaks as f32, LEAKS_MARK, Some(Job::with_duty(crate::content::Duty::Mend)), pool_names("leaks"));
    if let Some((t, n)) = r.felled.iter().max_by_key(|(t, n)| (**n, std::cmp::Reverse(**t))) {
        add(*n as f32, SPECIES_FELLED_MARK, Some(Job::gathering(crate::resources::ResourceKind::tree())), vec![format!("the {}bane", t.name()), format!("the {}gnawer", t.name())]);
    }
    add(r.great_trees as f32, GREAT_TREES_MARK, Some(Job::gathering(crate::resources::ResourceKind::tree())), pool_names("great_trees"));
    add(r.water_food(), WATER_FOOD_MARK, Some(Job::gathering(crate::resources::ResourceKind::food_plant(false))), pool_names("water_food"));
    if let Some((p, f)) = r.harvested.iter().max_by(|a, b| a.1.total_cmp(b.1)) {
        add(*f, PLANT_FOOD_MARK, Some(Job::gathering(crate::resources::ResourceKind::food_plant(false))), pool_names(&format!("plant/{p:?}")));
    }
    add(r.winter_food, WINTER_FOOD_MARK, Some(Job::gathering(crate::resources::ResourceKind::food_plant(false))), pool_names("winter_food"));
    add(r.carried_to(&waterworks_materials(), &[BuildingKind::dam(), BuildingKind::canal()]), TO_DAMS_MARK, Some(Job::with_duty(crate::content::Duty::Haul)), pool_names("to_dams"));
    add(r.carried_to(&[Good::food()], &BuildingKind::all().filter(|k| k.row().stores.contains(&Good::food())).collect::<Vec<_>>()), TO_CACHE_MARK, Some(Job::with_duty(crate::content::Duty::Haul)), pool_names("to_cache"));
    add(r.rafted_total(), RAFTED_MARK, Some(Job::with_duty(crate::content::Duty::Haul)), pool_names("rafted"));
    add(r.swum, SWUM_MARK, None, pool_names("swum"));
    add(r.grown as f32, GROWN_MARK, Some(Job::with_duty(crate::content::Duty::Forest)), pool_names("grown"));
    add(r.wolves as f32, WOLVES_MARK, Some(Job::with_duty(crate::content::Duty::Watch)), pool_names("wolves"));
    add(kits(sim, id).1 as f32, KITS_RAISED_MARK, None, pool_names("kits"));
    out.sort_by(|a, b| b.past.total_cmp(&a.past));
    out
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Deeds {
    pub of: BTreeMap<BeaverId, Record>,
    crew: BTreeMap<BuildingId, BTreeSet<BeaverId>>,
    pub dam_loads: BTreeMap<BuildingId, u32>,
    spotted: BTreeSet<(BeaverId, u32)>,
    grown: BTreeSet<ResourceId>,
    season: i64,
}

impl Deeds {
    pub fn get(&self, id: BeaverId) -> Option<&Record> {
        self.of.get(&id)
    }
    pub(crate) fn of(&mut self, id: BeaverId) -> &mut Record {
        self.of.entry(id).or_default()
    }
    pub(crate) fn worked(&mut self, id: BeaverId, site: BuildingId, kind: BuildingKind, labour: f32) {
        self.of(id).work(site, Some(kind)).labour += labour;
        self.crew.entry(site).or_default().insert(id);
    }
    pub(crate) fn packed(&mut self, id: BeaverId, dam: BuildingId, labour: f32) {
        self.worked(id, dam, BuildingKind::dam(), labour);
        let r = self.of(id);
        r.dam_loads += 1;
        r.work(dam, Some(BuildingKind::dam())).loads += 1;
        *self.dam_loads.entry(dam).or_default() += 1;
    }
    pub(crate) fn repaired(&mut self, id: BeaverId, at: BuildingId, kind: Option<BuildingKind>, labour: f32) {
        let r = self.of(id);
        r.mended += labour;
        r.work(at, kind).repairs += labour;
    }
    pub(crate) fn spotted(&mut self, id: BeaverId, wolf: u32) -> bool {
        let new = self.spotted.insert((id, wolf));
        if new {
            self.of(id).wolves += 1;
        }
        new
    }
}

pub fn full_name(sim: &SimState, id: BeaverId, name: &str) -> String {
    match sim.deeds.get(id).and_then(|r| r.epithet.as_ref()) {
        Some(e) => format!("{name} {}", e.word),
        None => name.to_string(),
    }
}

pub fn step(sim: &mut SimState, dt: f32, new_day: bool) {
    for b in &sim.colony.beavers {
        if let Some(j) = b.current_job {
            sim.deeds.of.entry(b.id).or_default().days[j] += dt;
        }
    }
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
        .filter(|(tree, _)| sim.resources.get(**tree).is_some_and(|r| r.kind.is_tree() && r.age_days >= crate::forestry::MATURE_AGE as f32))
        .map(|(tree, (who, _))| (*tree, *who))
        .collect();
    for (tree, who) in grown {
        sim.deeds.grown.insert(tree);
        sim.deeds.of(who).grown += 1;
    }
}

fn broad(skills: &PerId<Job, f32>) -> bool {
    let best = skills.values().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(0, |(i, _)| i);
    let rest: f32 = skills.values().enumerate().filter(|(i, _)| *i != best).map(|(_, s)| (s - SKILL_FLOOR).max(0.0) / EPITHET_SKILL).sum();
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
        let Some(best) = Job::all().filter(|&j| b.skills[j] >= EPITHET_SKILL).max_by(|&a, &c| b.skills[a].total_cmp(&b.skills[c])) else { continue };
        let choose = |names: &[String]| -> Option<String> {
            let free: Vec<&String> = names.iter().filter(|w| !held.contains(*w)).collect();
            free.get((b.id.0 as usize).wrapping_mul(2654435761) % free.len().max(1)).map(|w| (*w).clone())
        };
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
