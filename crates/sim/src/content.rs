//! Game content, read from the TOML files in `content/` when the game starts (see the README
//! there). Each row has a stable `key`, written in saves, and ids are positions in their table.
use crate::buildings::{Amount, Site};
use std::marker::PhantomData;
use crate::plants::PlantSpec;
use crate::trees::{Mix, TreeSpec};
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeMap, path::{Path, PathBuf}, sync::OnceLock};

static CONTENT: OnceLock<Content> = OnceLock::new();

/// The loaded content. Loads the default content on first use.
pub fn content() -> &'static Content {
    CONTENT.get_or_init(|| Content::load(&default_path()).unwrap_or_else(|e| panic!("{e}")))
}

/// Use other content (a test fixture, a mod) for the rest of this process. Must run before
/// anything reads content.
pub fn init(dir: &Path) -> Result<(), String> {
    CONTENT.set(Content::load(dir)?).map_err(|_| "content was already loaded".to_string())
}

/// `BEAVER_CONTENT` if set, else the content folder beside the simulation's source.
pub fn default_path() -> PathBuf {
    std::env::var_os("BEAVER_CONTENT").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("content"))
}

/// An id into one content table.
pub trait Id: Copy + Eq + Ord + 'static {
    /// Its table's name in the content files.
    const TABLE: &'static str;
    fn index(self) -> usize;
    fn at(index: usize) -> Self;
    fn count() -> usize;
    fn key(self) -> &'static str;
}

thread_local! {
    /// While content loads: each table's keys, so rows can name rows of any table by key.
    static LOADING: RefCell<Option<BTreeMap<&'static str, Vec<String>>>> = const { RefCell::new(None) };
}

/// The id with this key: in the content being loaded, if any, else in the loaded content.
pub fn by_key<T: Id>(key: &str) -> Option<T> {
    let loading = LOADING.with(|l| l.borrow().as_ref().map(|t| t.get(T::TABLE).and_then(|keys| keys.iter().position(|k| k == key))));
    match loading {
        Some(i) => i.map(T::at),
        None => (0..T::count()).map(T::at).find(|i| i.key() == key),
    }
}

macro_rules! content_id {
    ($(#[$m:meta])* $T:ident, $Row:ty, $field:ident, $table:literal) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $T(u16);
        impl $crate::content::Id for $T {
            const TABLE: &'static str = $table;
            fn index(self) -> usize { self.0 as usize }
            fn at(index: usize) -> Self { Self(index as u16) }
            fn count() -> usize { $crate::content::content().$field.len() }
            fn key(self) -> &'static str { &self.row().key }
        }
        impl $T {
            /// Every row, in table order.
            pub fn all() -> impl DoubleEndedIterator<Item = $T> + ExactSizeIterator + Clone {
                (0..$crate::content::content().$field.len()).map(|i| $T(i as u16))
            }
            pub fn index(self) -> usize { self.0 as usize }
            pub fn key(self) -> &'static str { &self.row().key }
            pub fn name(self) -> &'static str { &self.row().name }
            pub fn row(self) -> &'static $Row { &$crate::content::content().$field[self.0 as usize] }
            /// The row with this key; panics if there is none (tests and tools).
            pub fn of(key: &str) -> $T { $crate::content::by_key(key).unwrap_or_else(|| panic!("no {} {key}", $table)) }
        }
        impl std::fmt::Debug for $T {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.key()) }
        }
        impl serde::Serialize for $T {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(self.key()) }
        }
        impl<'de> serde::Deserialize<'de> for $T {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let key = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
                $crate::content::by_key(&key).ok_or_else(|| serde::de::Error::custom(format!("no {} {key}", $table)))
            }
        }
    };
}

content_id!(
    /// A kind of building: a dam, a lodge, a store and the rest.
    BuildingKind, Spec, buildings, "building"
);
content_id!(
    /// A good beavers carry and store.
    Good, GoodRow, goods, "good"
);
content_id!(
    /// A kind of thing on the map beavers gather from: a tree, a felled trunk, a food plant, a
    /// deposit of mud or pebbles.
    ResourceKind, ResourceRow, resources, "resource"
);
content_id!(
    /// A job a beaver can be given: gathering something, building, hauling and the rest.
    Job, JobRow, jobs, "job"
);
content_id!(
    /// A tree species.
    Tree, TreeRow, trees, "tree"
);
content_id!(
    /// A food plant species.
    Plant, PlantRow, plants, "plant"
);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodRow {
    pub key: String,
    pub name: String,
    /// The most one beaver carries at once.
    pub load: f32,
    /// Share of full pace carrying it in water or on wet ground, and over dry ground.
    pub pace: f32,
    pub land_pace: f32,
    /// Costs nothing to tow along a canal deep enough to swim.
    #[serde(default)] pub floats: bool,
    /// Beavers eat it; it spoils in store.
    #[serde(default)] pub edible: bool,
    /// Each unit keeps the tree species it grows into.
    #[serde(default)] pub carries_species: bool,
    /// Shown as this icon, in this colour (sRGB), and drawn with this model carried or lying.
    pub icon: String,
    pub colour: [f32; 3],
    pub model: String,
    /// Hugged in the forepaws when carried, else carried in the teeth; shown in the paws while
    /// building or digging.
    #[serde(default)] pub held_in_paws: bool,
    #[serde(default)] pub worked_in_paws: bool,
}

/// One value per row of a content table, indexed by its id. Saved as (key, value) pairs.
#[derive(Clone, PartialEq)]
pub struct PerId<K, V> {
    values: Vec<V>,
    _k: PhantomData<fn() -> K>,
}

impl<K: Id, V> PerId<K, V> {
    pub fn from_fn(f: impl FnMut(K) -> V) -> Self {
        Self { values: (0..K::count()).map(K::at).map(f).collect(), _k: PhantomData }
    }
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.values.iter().enumerate().map(|(i, v)| (K::at(i), v))
    }
    pub fn values(&self) -> std::slice::Iter<'_, V> { self.values.iter() }
    pub fn values_mut(&mut self) -> std::slice::IterMut<'_, V> { self.values.iter_mut() }
}
impl<K: Id, V: Clone> PerId<K, V> {
    pub fn filled(v: V) -> Self { Self::from_fn(|_| v.clone()) }
}
impl<K: Id, V: Default> Default for PerId<K, V> {
    fn default() -> Self { Self::from_fn(|_| V::default()) }
}
impl<K: Id, V> std::ops::Index<K> for PerId<K, V> {
    type Output = V;
    fn index(&self, k: K) -> &V { &self.values[k.index()] }
}
impl<K: Id, V> std::ops::IndexMut<K> for PerId<K, V> {
    fn index_mut(&mut self, k: K) -> &mut V { &mut self.values[k.index()] }
}
impl<K: Id + std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for PerId<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_map().entries(self.iter()).finish() }
}
impl<K: Id, V: serde::Serialize> serde::Serialize for PerId<K, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter().map(|(k, v)| (k.key(), v)))
    }
}
impl<'de, K: Id, V: serde::Deserialize<'de> + Default> serde::Deserialize<'de> for PerId<K, V> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut out = Self::default();
        for (key, v) in <Vec<(String, V)> as serde::Deserialize>::deserialize(d)? {
            out[by_key::<K>(&key).ok_or_else(|| serde::de::Error::custom(format!("no {} {key}", K::TABLE)))?] = v;
        }
        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeRow {
    pub key: String,
    pub name: String,
    pub spec: TreeSpec,
    /// Pale-barked: places among these trees may be named for it.
    #[serde(default)] pub pale: bool,
    pub look: TreeLook,
}

/// How a tree species is drawn: a drawing style (a model id in the game) and its build and
/// colours (`0xrrggbb`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeLook {
    pub model: String,
    pub bark: u32,
    /// The trunk's foot: darker, rougher, or black.
    pub foot: u32,
    pub trunk_r: f32,
    /// Heights of the first fork and of the crown's centre, and the crown's size.
    pub fork: f32,
    pub centre: f32,
    pub radius: [f32; 3],
    pub tips: usize,
    pub clump: f32,
    /// Clumps squashed to flat sprays (below 1) or round (1).
    pub flat: f32,
    /// How far outer tips hang down.
    pub droop: f32,
    pub limb: f32,
    pub seed: u32,
    /// Leaf colour through the year (spring, summer, autumn, late autumn): a shade and a light.
    pub leaves: [(u32, u32); 4],
    /// Keeps its needles all year; keeps its dead leaves through winter.
    #[serde(default)] pub evergreen: bool,
    #[serde(default)] pub keeps_leaves: bool,
}

/// How a food plant is drawn: a drawing style (a model id in the game) and, for the styles that
/// take them, its height and colours (`0xrrggbb`) and a second form (`variant`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlantLook {
    pub model: String,
    #[serde(default)] pub height: f32,
    #[serde(default)] pub stem: u32,
    #[serde(default)] pub leaf: (u32, u32),
    #[serde(default)] pub flower: u32,
    #[serde(default)] pub variant: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlantRow {
    pub key: String,
    pub name: String,
    pub spec: PlantSpec,
    /// How much faster water it bears than other water plants.
    #[serde(default = "one")] pub flow: f32,
    pub look: PlantLook,
}

fn one() -> f32 { 1.0 }

/// One kind of building's definition: construction, staffing, storage and progression.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub key: String,
    pub name: String,
    /// Very short (2–4 word) description of what the building does.
    #[serde(default)] pub summary: String,
    pub footprint: (usize, usize),
    pub site: Site,
    #[serde(default)] pub cost: Vec<Amount>,
    pub work: f32,
    #[serde(default)] pub slots: Vec<Job>,
    #[serde(default)] pub area: Option<(usize, usize)>,
    #[serde(default)] pub capacity: f32,
    /// A building that must be finished before this one can be placed.
    #[serde(default)] pub requires: Option<BuildingKind>,
    /// The goods it stores.
    #[serde(default)] pub stores: Vec<crate::buildings::Good>,
    /// The refits it can have; one it lacks it cannot.
    #[serde(default)] pub refits: BTreeMap<crate::maintenance::Refit, RefitRow>,
    /// How it goes up: dropped as a site, or drawn as a line of dam or canal cells.
    #[serde(default)] pub built_as: BuiltAs,
    /// Houses a family, behind a door kept clear.
    #[serde(default)] pub home: bool,
    /// Rafts load and unload at it.
    #[serde(default)] pub docks_rafts: bool,
    /// Condition lost a day, before weather and water.
    pub wear: f32,
    /// Looks: a model id in the game, its footing (`mound`, `ring` or `piles`) and half-size,
    /// its icon, the key that picks it, its place in the build menu, and a round work area.
    pub model: String,
    #[serde(default)] pub footing: Option<String>,
    #[serde(default)] pub footing_size: (f32, f32),
    pub icon: String,
    #[serde(default)] pub hotkey: Option<String>,
    #[serde(default)] pub menu: Option<u32>,
    #[serde(default)] pub round_area: bool,
}

/// How a building goes up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum BuiltAs { #[default] Site, Dam, Canal }

impl BuildingKind {
    pub fn spec(self) -> &'static Spec { self.row() }
    fn one(f: impl Fn(&Spec) -> bool) -> BuildingKind { Self::all().find(|k| f(k.row())).expect("validated on load") }
    /// The one kind built as a dam, the one built as a canal, and the first that is a home.
    pub fn dam() -> BuildingKind { Self::one(|r| r.built_as == BuiltAs::Dam) }
    pub fn canal() -> BuildingKind { Self::one(|r| r.built_as == BuiltAs::Canal) }
    pub fn home() -> BuildingKind { Self::one(|r| r.home) }
    pub fn is_dam(self) -> bool { self.row().built_as == BuiltAs::Dam }
    pub fn is_canal(self) -> bool { self.row().built_as == BuiltAs::Canal }
    pub fn is_home(self) -> bool { self.row().home }
    pub fn docks_rafts(self) -> bool { self.row().docks_rafts }
    /// A seat of a job with this duty.
    pub fn seats(self, d: Duty) -> bool { self.row().slots.iter().any(|j| j.duty() == d) }
    /// Its sentry keeps watch; its haulers even out the stores.
    pub fn watches(self) -> bool { self.seats(Duty::Watch) }
    pub fn balances(self) -> bool { self.seats(Duty::Haul) }
    /// The first kind that stores a good.
    pub fn store_for(g: Good) -> Option<BuildingKind> { Self::all().find(|k| k.row().stores.contains(&g)) }
}

/// Materials for works that are not buildings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Works {
    /// A gate in a dam or canal, and what making a gate a lock adds.
    pub gate: Vec<Amount>,
    pub lock: Vec<Amount>,
    /// The building that must stand finished before a gate, or a lock, can be ordered.
    #[serde(default)] pub gate_needs: Option<BuildingKind>,
    #[serde(default)] pub lock_needs: Option<BuildingKind>,
}

/// What a refit of one kind of building takes, and what it gives (see `maintenance::cost`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefitRow {
    /// Materials: a fixed amount, and so much more per cell of the building (a dam: per dam cell).
    #[serde(default)] pub cost: Vec<Amount>,
    #[serde(default)] pub per_cell: Vec<Amount>,
    #[serde(default)] pub labour: f32,
    #[serde(default)] pub labour_per_cell: f32,
    #[serde(default)] pub min_labour: f32,
    /// How many times longer the building lasts once refitted.
    #[serde(default = "one")] pub lasts: f32,
}

impl RefitRow {
    /// The materials (each rounded up) and labour for a building of `cells` cells.
    pub fn quote(&self, cells: f32) -> (Vec<Amount>, f32) {
        let mut materials: Vec<Amount> = self.cost.clone();
        for &(g, a) in &self.per_cell {
            match materials.iter_mut().find(|m| m.0 == g) {
                Some(m) => m.1 += cells * a,
                None => materials.push((g, cells * a)),
            }
        }
        materials.iter_mut().for_each(|m| m.1 = m.1.ceil());
        (materials, (self.labour + self.labour_per_cell * cells).max(self.min_labour))
    }
}

/// The mechanics a kind of resource follows; all else about it is its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Behaviour {
    /// Grows through its stages, is felled into the `Felled` kind and seeds new trees.
    Tree,
    /// A felled trunk: logs, then branches, and bark that foragers strip for food.
    Felled,
    /// Regrows, spreads and dies back (see `wetland`).
    FoodPlant,
    /// Dug until it is gone, or refilled by its stream if `flowing`.
    Deposit,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRow {
    pub key: String,
    pub name: String,
    pub behaviour: Behaviour,
    /// The goods gathering it gives.
    #[serde(default)] pub yields: Vec<crate::buildings::Good>,
    /// Grows in open water; refilled by its stream.
    #[serde(default)] pub aquatic: bool,
    #[serde(default)] pub flowing: bool,
    /// Closed while the ground is frozen.
    #[serde(default)] pub freezes: bool,
    /// The group its gatherers work, and the job that gathers it.
    pub group: String,
    pub role: Job,
    /// How many beavers work one at once, and how much one of ordinary skill takes a day.
    pub gatherers: usize,
    pub gather_rate: f32,
    /// A building whose tools must stand finished before any is gathered.
    #[serde(default)] pub needs: Option<BuildingKind>,
    /// In the way of walking.
    #[serde(default)] pub solid: bool,
    /// Looks: the steps its fill is drawn in, the words for gathering it, icon, model and pose.
    pub fill_steps: u8,
    pub doing: String,
    pub icon: String,
    pub lens_colour: [f32; 3],
    pub model: String,
    pub pose: String,
    /// Its group's kinds, best first, and the first kind alike to it there (filled in on load).
    #[serde(skip)] pub sources: Vec<ResourceKind>,
    #[serde(skip)] pub alike: Option<ResourceKind>,
}

/// The kinds the world's making puts down.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roles {
    pub outcrop: ResourceKind,
    pub gravel: ResourceKind,
    pub stream_mud: ResourceKind,
    pub bank_mud: ResourceKind,
    pub spoil: ResourceKind,
}

/// The kind of work a job does; which job does it, and what it is called, is its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Duty {
    /// Gathers the kinds of resource whose `role` is this job.
    Gather,
    Build,
    Haul,
    /// Collects seedlings and plants them round a forester.
    Forest,
    /// Keeps watch at a lookout.
    Watch,
    /// Repairs buildings and keeps dams and canals in order, without a seat.
    Mend,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRow {
    pub key: String,
    pub name: String,
    /// Its beavers, in words.
    pub plural: String,
    /// The skill it builds, in words.
    pub skill: String,
    pub duty: Duty,
    /// Its place on the colony panel, first first.
    pub priority: u32,
    /// The player may keep it near the water.
    #[serde(default)] pub shore_limit: bool,
    /// Skill makes it go faster.
    #[serde(default)] pub skill_speeds: bool,
    pub icon: String,
    /// The colour of its beavers' paths (sRGB).
    pub path_colour: [f32; 3],
}

impl Job {
    pub fn duty(self) -> Duty { self.row().duty }
    /// The one job that does a duty (not `Gather`: many jobs gather).
    pub fn with_duty(d: Duty) -> Job {
        Job::all().find(|j| j.duty() == d).unwrap_or_else(|| panic!("no job does {d:?}"))
    }
    /// The job that gathers a kind of resource.
    pub fn gathering(k: ResourceKind) -> Job { k.row().role }
    /// Every job in the colony panel's order.
    pub fn by_priority() -> Vec<Job> {
        let mut v: Vec<Job> = Job::all().collect();
        v.sort_by_key(|j| j.row().priority);
        v
    }
}

/// One map's flora as written: trees by mix, food plants by how common.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Flora {
    #[serde(default)] trees: BTreeMap<Tree, Mix>,
    #[serde(default)] plants: BTreeMap<Plant, f32>,
}

/// One content file: any part of the content. A folder's files are read in name order and added
/// together, so a mod or a test can add rows in a file of its own.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct File {
    good: Vec<GoodRow>,
    repair_fallback: Vec<Amount>,
    works: Option<Works>,
    start: Vec<Amount>,
    unknown: Option<Tree>,
    sapling: Option<Tree>,
    building: Vec<Spec>,
    resource: Vec<ResourceRow>,
    groups: BTreeMap<String, Vec<Vec<ResourceKind>>>,
    roles: Option<Roles>,
    job: Vec<JobRow>,
    tree: Vec<TreeRow>,
    plant: Vec<PlantRow>,
    flora: BTreeMap<String, Flora>,
    names: BTreeMap<String, Vec<String>>,
}

#[derive(Debug)]
pub struct Content {
    pub goods: Vec<GoodRow>,
    /// The good beavers eat, and the good that carries a tree species (see `Good`).
    pub food: Good,
    pub seedling: Good,
    /// What repairing a building that cost nothing takes.
    pub repair_fallback: Vec<Amount>,
    pub works: Works,
    /// What a new colony starts with.
    pub start: Vec<Amount>,
    /// The species taken for a tree of no known species, and for a sapling with no food tree near.
    pub unknown: Tree,
    pub sapling: Tree,
    pub buildings: Vec<Spec>,
    pub resources: Vec<ResourceRow>,
    pub roles: Roles,
    pub jobs: Vec<JobRow>,
    pub trees: Vec<TreeRow>,
    pub plants: Vec<PlantRow>,
    /// Each map's tree mix and food plant weights, by id; `default` for maps without their own.
    pub tree_flora: BTreeMap<String, Vec<Mix>>,
    pub plant_flora: BTreeMap<String, Vec<f32>>,
    /// Word pools for names (see `names`).
    pub names: BTreeMap<String, Vec<&'static str>>,
}

fn check(ok: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(format!("content: {}", what())) }
}

impl Content {
    pub fn load(dir: &Path) -> Result<Content, String> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| format!("content: {}: {e}", dir.display()))?
            .flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
        paths.sort();
        let mut texts = Vec::new();
        for p in paths {
            texts.push((std::fs::read_to_string(&p).map_err(|e| format!("content: {}: {e}", p.display()))?, p));
        }
        // Every row's key first, so that rows can name rows in any table or file.
        let mut keys: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
        for (text, path) in &texts {
            let table: toml::Table = text.parse().map_err(|e| format!("content: {}: {e}", path.display()))?;
            for t in [BuildingKind::TABLE, Good::TABLE, ResourceKind::TABLE, Job::TABLE, Tree::TABLE, Plant::TABLE] {
                for row in table.get(t).and_then(|r| r.as_array()).into_iter().flatten() {
                    keys.entry(t).or_default().push(row.get("key").and_then(|k| k.as_str()).unwrap_or_default().to_string());
                }
            }
        }
        for (t, ks) in &keys {
            check(ks.iter().enumerate().all(|(i, k)| !ks[..i].contains(k)), || format!("{t}: keys must be unique"))?;
        }
        LOADING.with(|l| *l.borrow_mut() = Some(keys));
        let parsed: Result<Vec<File>, String> = texts.iter().map(|(t, p)| toml::from_str(t).map_err(|e| format!("content: {}: {e}", p.display()))).collect();
        LOADING.with(|l| *l.borrow_mut() = None);
        let mut c = File::default();
        for f in parsed? {
            c.good.extend(f.good);
            c.repair_fallback.extend(f.repair_fallback);
            c.works = c.works.or(f.works);
            c.start.extend(f.start);
            c.unknown = c.unknown.or(f.unknown);
            c.sapling = c.sapling.or(f.sapling);
            c.building.extend(f.building);
            c.resource.extend(f.resource);
            c.groups.extend(f.groups);
            c.roles = c.roles.or(f.roles);
            c.job.extend(f.job);
            c.tree.extend(f.tree);
            c.plant.extend(f.plant);
            c.names.extend(f.names);
            for (map, fl) in f.flora {
                let e = c.flora.entry(map).or_default();
                e.trees.extend(fl.trees);
                e.plants.extend(fl.plants);
            }
        }

        let one = |f: fn(&GoodRow) -> bool, what: &str| -> Result<Good, String> {
            let found: Vec<usize> = c.good.iter().enumerate().filter(|(_, g)| f(g)).map(|(i, _)| i).collect();
            check(found.len() == 1, || format!("good: exactly one good must be {what}"))?;
            Ok(Good(found[0] as u16))
        };
        let (food, seedling) = (one(|g| g.edible, "edible")?, one(|g| g.carries_species, "marked carries_species")?);
        for w in [BuiltAs::Dam, BuiltAs::Canal] {
            check(c.building.iter().filter(|b| b.built_as == w).count() == 1, || format!("building: exactly one must be built as a {w:?}"))?;
        }
        check(c.building.iter().any(|b| b.home), || "building: some building must be a home".into())?;
        for b in &c.building {
            check((1..=64).contains(&b.footprint.0) && (1..=64).contains(&b.footprint.1), || format!("building {}: footprint must be 1 to 64 cells a side", b.key))?;
            check(b.area.is_none_or(|a| (1..=1024).contains(&a.0) && a.0 == a.1), || format!("building {}: area must be square, 1 to 1024", b.key))?;
            check(b.requires.is_none_or(|r| c.building[r.index()].key != b.key), || format!("building {}: cannot require itself", b.key))?;
        }
        for b in [Behaviour::Tree, Behaviour::Felled] {
            check(c.resource.iter().filter(|r| r.behaviour == b).count() == 1, || format!("resource: exactly one must be a {b:?}"))?;
        }
        for aquatic in [false, true] {
            check(c.resource.iter().any(|r| r.behaviour == Behaviour::FoodPlant && r.aquatic == aquatic), || "resource: needs a food plant on land and one in water".into())?;
        }
        for i in 0..c.resource.len() {
            let tiers = c.groups.get(&c.resource[i].group).ok_or_else(|| format!("content: resource {}: no group {}", c.resource[i].key, c.resource[i].group))?;
            let me = ResourceKind(i as u16);
            c.resource[i].sources = tiers.iter().flatten().copied().collect();
            c.resource[i].alike = tiers.iter().find(|t| t.contains(&me)).and_then(|t| t.first().copied());
            check(c.resource[i].alike.is_some(), || format!("resource {}: not in its group", c.resource[i].key))?;
        }
        for d in [Duty::Build, Duty::Haul, Duty::Forest, Duty::Watch, Duty::Mend] {
            check(c.job.iter().filter(|j| j.duty == d).count() == 1, || format!("job: exactly one must have duty {d:?}"))?;
        }
        for r in &c.resource {
            check(c.job[r.role.index()].duty == Duty::Gather, || format!("resource {}: its role must be a gathering job", r.key))?;
        }
        for t in &c.tree {
            let s = &t.spec;
            check(s.growth > 0.0 && s.fell_work > 0.0 && s.flood > 0.0 && s.max_depth > 0.0, || format!("tree {}: needs positive growth, felling work, flood tolerance and water depth", t.key))?;
        }
        for p in &c.plant {
            let s = &p.spec;
            check(s.food > 0.0 && s.keep < 1.0 && s.max_depth >= s.min_depth, || format!("plant {}: needs food, a winter share below 1 and a depth range", p.key))?;
        }

        let maps: Vec<&str> = crate::maps::ids().chain(["default"]).collect();
        let (mut tree_flora, mut plant_flora) = (BTreeMap::new(), BTreeMap::new());
        for (map, f) in &c.flora {
            check(maps.contains(&map.as_str()), || format!("flora {map}: names no map"))?;
            let mut trees = vec![Mix { dry: 0.0, wet: 0.0, upland: 0.0, open: 0.0 }; c.tree.len()];
            f.trees.iter().for_each(|(t, m)| trees[t.index()] = *m);
            check(trees.iter().any(|w| w.dry > 0.0) && trees.iter().any(|w| w.wet > 0.0), || format!("flora {map}: needs dry and wet trees"))?;
            let mut plants = vec![0.0; c.plant.len()];
            f.plants.iter().for_each(|(p, w)| plants[p.index()] = *w);
            for h in crate::plants::Habitat::ALL {
                check(plants.iter().zip(&c.plant).any(|(w, p)| *w > 0.0 && p.spec.habitat == h), || format!("flora {map}: needs a {h:?} plant"))?;
            }
            tree_flora.insert(map.clone(), trees);
            plant_flora.insert(map.clone(), plants);
        }
        for m in &maps {
            check(tree_flora.contains_key(*m), || format!("flora {m}: missing"))?;
        }
        Ok(Content {
            unknown: c.unknown.ok_or("content: trees: no `unknown` species")?, sapling: c.sapling.ok_or("content: trees: no `sapling` species")?,
            goods: c.good, food, seedling, repair_fallback: c.repair_fallback, works: c.works.ok_or("content: buildings: no `works`")?, start: c.start, buildings: c.building, resources: c.resource, roles: c.roles.ok_or("content: resources: no `roles`")?, jobs: c.job, trees: c.tree, plants: c.plant, tree_flora, plant_flora,
            names: c.names.into_iter().map(|(k, v)| (k, v.into_iter().map(|w| &*Box::leak(w.into_boxed_str())).collect())).collect(),
        })
    }
}
