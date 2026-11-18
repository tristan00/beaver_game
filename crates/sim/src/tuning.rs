//! Every balance parameter of the simulation, grouped by system.
//! Each module re-exports (or imports) its group; structural constants such as grid sizes
//! and solver numerics stay in their modules.

pub mod colony {
    pub const KIT_DAYS: f32 = crate::calendar::YEAR_DAYS as f32;
    pub const ADULT_DAYS: f32 = 2.0 * crate::calendar::YEAR_DAYS as f32;
    pub const LODGE_CAPACITY: usize = 12;
    pub const LODGE_WOOD_COST: f32 = 8.0;
    pub const MIN_LODGE_SPACING: f32 = 5.0;
    pub const MAX_WATER_SCAN: f32 = 10.0;
    pub const WORK_FRACTION: f32 = 0.6;
    pub const CACHE_LIFETIME_DAYS: f64 = 30.0;
    /// Days of continuously bad mood after which an adult or yearling leaves the colony.
    pub const DEPARTURE_BAD_MOOD_DAYS: f32 = 30.0;
    /// Breeding window: the calendar's spring (day 0 of the year is the first day of spring).
    pub const SPRING_START: f64 = 0.0;
    pub const SPRING_END: f64 = crate::calendar::SEASON_DAYS;
    pub const BIRTH_FROM_DAY: f64 = 15.0;
    /// Day of the year with the warmest temperature: the middle of summer.
    pub const MIDSUMMER_DAY: f64 = 1.5 * crate::calendar::SEASON_DAYS;

    // Terrain the colony sees.
    /// A water-depth grid cell deeper than this counts as water.
    pub const WATER_DEPTH: f32 = 0.05;
    /// Half width of the built-in river used when no water grid is supplied.
    pub const DEFAULT_RIVER_HALF_WIDTH: f32 = 2.0;

    // Life stages: share of an adult's work output and food needs.
    pub const KIT_WORK_WEIGHT: f32 = 0.0;
    pub const YEARLING_WORK_WEIGHT: f32 = 0.5;
    pub const KIT_FOOD_WEIGHT: f32 = 0.4;
    pub const YEARLING_FOOD_WEIGHT: f32 = 0.7;

    // Founding and newcomers.
    pub const FOUNDER_ADULT_AGE_YEARS: f32 = 3.0;
    pub const FOUNDER_YEARLING_AGE_YEARS: f32 = 1.5;
    pub const START_FOOD: f32 = 40.0;
    pub const START_WOOD: f32 = 20.0;
    pub const FOUNDING_LODGE_CONDITION: f32 = 1.0;
    /// Condition of a lodge built by a newly paired couple.
    pub const NEW_LODGE_CONDITION: f32 = 0.7;
    pub const START_SOCIAL: f32 = 0.7;
    pub const START_MOOD: f32 = 0.9;
    /// Adults spawn with each skill uniformly in `0..START_SKILL_MAX`.
    pub const START_SKILL_MAX: f32 = 0.1;
    pub const LIFESPAN_MIN_YEARS: f32 = 10.0;
    pub const LIFESPAN_SPREAD_YEARS: f32 = 4.0;
    pub const IMMIGRANT_MIN_AGE_YEARS: f32 = 2.0;
    pub const IMMIGRANT_AGE_SPREAD_YEARS: f32 = 2.0;
    /// Work order a new family gives its members by rank.
    pub const DEFAULT_PRIORITIES: [crate::colony::Job; 8] = {
        use crate::colony::Job::*;
        [Forager, Forager, Logger, Builder, Hauler, Guard, Crafter, Scout]
    };

    // Season.
    /// Temperature below which beavers start to feel cold, and the span over which cold reaches 1.
    pub const COLD_BELOW_TEMP: f32 = 0.2;
    pub const COLD_RANGE: f32 = 1.2;
    /// Foraging season factor: `BASE + TEMP * temperature`, at least `MIN`.
    pub const FORAGE_SEASON_BASE: f32 = 0.75;
    pub const FORAGE_SEASON_TEMP: f32 = 0.25;
    pub const FORAGE_SEASON_MIN: f32 = 0.5;

    // Job choice.
    /// Below this food per head unassigned workers forage, and no kits are born.
    pub const HUNGRY_FOOD_PER_CAPITA: f32 = 4.0;
    /// Spring litters only fill the room left by this many days of stored food per head (at `HUNGER_PER_DAY`
    /// units a day): enough to carry the whole colony, kits included, through a winter.
    pub const BIRTH_MIN_FOOD_DAYS: f32 = 30.0;
    /// Litters stop once the population reaches this multiple of `FORAGE_CROWDING_POPULATION`.
    pub const BIRTH_MAX_CROWDING: f32 = 1.0;
    /// Lodges below this condition draw unassigned builders, if this much wood is stored.
    pub const REPAIR_BELOW_CONDITION: f32 = 0.6;
    pub const REPAIR_MIN_WOOD: f32 = 2.0;

    // Work output.
    /// Distance from home of a job's default work site (plus the job index).
    pub const WORK_SITE_DISTANCE: f32 = 3.0;
    /// Travel cost of a swum cell relative to a walked one.
    pub const SWIM_COST: f32 = 0.5;
    /// Efficiency is `1 / (1 + TRAVEL_COST_PENALTY * travel cost)`.
    pub const TRAVEL_COST_PENALTY: f32 = 0.02;
    pub const WORK_REST_BASE: f32 = 0.3;
    pub const WORK_REST_SCALE: f32 = 0.7;
    pub const WORK_HEALTH_BASE: f32 = 0.5;
    pub const WORK_HEALTH_SCALE: f32 = 0.5;
    pub const WORK_MOOD_BASE: f32 = 0.7;
    pub const WORK_MOOD_SCALE: f32 = 0.3;
    /// Output multiplier is `SKILL_BASE + skill`.
    pub const SKILL_BASE: f32 = 0.5;
    /// Skill gained per work unit, scaled by the skill still missing.
    pub const SKILL_GAIN: f32 = 0.01;
    /// A skill at or above this makes its owner a specialist in that job.
    pub const SPECIALIST_SKILL: f32 = 0.5;
    /// Output multiplier of a specialist working their own job.
    pub const SPECIALIST_BONUS: f32 = 1.25;
    /// Skill gained per day by a kit or yearling watching a working lodge mate, scaled by the skill missing.
    pub const YOUNG_LEARN_RATE: f32 = 0.002;
    /// Population at which foraging yield halves.
    pub const FORAGE_CROWDING_POPULATION: f32 = 100.0;
    pub const HAUL_BOOST_PER_EFFORT: f32 = 0.15;
    pub const HAUL_BOOST_MAX: f32 = 0.6;
    /// Output boost at full tools.
    pub const TOOLS_BOOST: f32 = 0.3;
    /// Foraging bonus per `SCOUT_WORK_PER_BONUS` scouting, up to `SCOUT_BONUS_MAX_STEPS` steps.
    pub const SCOUT_BONUS: f32 = 0.1;
    pub const SCOUT_WORK_PER_BONUS: f32 = 20.0;
    pub const SCOUT_BONUS_MAX_STEPS: f32 = 2.0;
    pub const FOOD_CAP_BASE: f32 = 10.0;
    pub const FOOD_CAP_PER_BEAVER: f32 = 25.0;
    pub const WOOD_CAP_BASE: f32 = 60.0;
    pub const WOOD_CAP_PER_BEAVER: f32 = 10.0;
    pub const FORAGE_YIELD: f32 = 4.0;
    pub const LOG_YIELD: f32 = 1.2;
    /// Lodge condition repaired per unit of builder power.
    pub const REPAIR_YIELD: f32 = 0.04;
    pub const TOOLS_YIELD: f32 = 0.05;
    pub const REPAIR_WOOD_PER_CONDITION: f32 = 4.0;

    // Hazards.
    pub const GUARD_MITIGATION: f32 = 0.4;
    pub const CANAL_MITIGATION: f32 = 0.03;
    pub const CANAL_MITIGATION_MAX_DUG: f32 = 10.0;
    /// Injury chance per working day: base, per land cell travelled, per cell from water.
    pub const HAZARD_BASE: f32 = 0.004;
    pub const HAZARD_PER_LAND_CELL: f32 = 0.003;
    pub const HAZARD_PER_WATER_DIST: f32 = 0.01;
    pub const SAFETY_HAZARD_SCALE: f32 = 6.0;
    pub const SAFETY_MIN: f32 = 0.2;
    pub const INJURY_HEALTH: f32 = 0.15;
    pub const INJURY_SAFETY: f32 = 0.3;

    // Needs (rates are per day).
    pub const GUARD_SAFETY_PER_STRENGTH: f32 = 0.2;
    pub const GUARD_SAFETY_MAX: f32 = 0.2;
    /// Insulation without a lodge, and the extra a lodge in full condition gives.
    pub const OPEN_INSULATION: f32 = 0.05;
    /// How well a beaver with no home rests (sleeping rough in the open): worse than a lodge but
    /// enough to recover, so a family that arrives homeless can still build its first shelter.
    /// Cold still follows `OPEN_INSULATION`.
    pub const OPEN_REST: f32 = 0.4;
    pub const LODGE_INSULATION: f32 = 0.85;
    pub const HUNGER_PER_DAY: f32 = 0.25;
    /// Beavers eat from the stockpile when their food need drops below this.
    pub const EAT_BELOW: f32 = 0.7;
    /// Share of the day an idle beaver is active.
    pub const IDLE_FRACTION: f32 = 0.1;
    pub const TIRE_RATE: f32 = 0.20;
    pub const SHELTER_REST_RATE: f32 = 0.40;
    pub const RESTING_RECOVERY: f32 = 0.3;
    /// Exhausted beavers stop working below `EXHAUSTED_REST` until rest reaches `RESUME_WORK_REST`.
    pub const EXHAUSTED_REST: f32 = 0.25;
    pub const RESUME_WORK_REST: f32 = 0.6;
    pub const WARMTH_RATE: f32 = 0.5;
    pub const SAFETY_RATE: f32 = 0.4;
    pub const IDLE_SAFETY: f32 = 0.9;
    pub const SOCIAL_RATE: f32 = 0.4;
    pub const SOCIAL_BASE: f32 = 0.2;
    pub const SOCIAL_PER_MATE: f32 = 0.2;
    pub const SOCIAL_MAX_MATES: usize = 4;
    pub const MOOD_FOOD_WEIGHT: f32 = 0.4;
    pub const MOOD_WARMTH_WEIGHT: f32 = 0.2;
    pub const MOOD_REST_WEIGHT: f32 = 0.15;
    pub const MOOD_SAFETY_WEIGHT: f32 = 0.15;
    pub const MOOD_SOCIAL_WEIGHT: f32 = 0.1;
    pub const MOOD_HEALTH_BASE: f32 = 0.5;
    pub const MOOD_HEALTH_SCALE: f32 = 0.5;
    pub const MOOD_RATE: f32 = 0.5;
    /// Mood below this counts toward `DEPARTURE_BAD_MOOD_DAYS`.
    pub const LOW_MOOD_BELOW: f32 = 0.2;
    pub const FOOD_SPOILAGE: f32 = 0.003;
    pub const TOOLS_WEAR: f32 = 0.003;
    /// Lodge condition lost per day, multiplied by `1 + LODGE_COLD_WEAR * cold`.
    pub const LODGE_WEAR: f32 = 0.002;
    pub const LODGE_COLD_WEAR: f32 = 2.0;

    // Health (rates are per day).
    pub const KIT_FRAILTY: f32 = 1.5;
    pub const STARVE_RATE: f32 = 0.02;
    pub const STARVE_BELOW: f32 = 0.15;
    pub const FREEZE_RATE: f32 = 0.015;
    pub const FREEZE_BELOW: f32 = 0.2;
    pub const EXHAUST_RATE: f32 = 0.01;
    pub const EXHAUST_BELOW: f32 = 0.05;
    /// Health lost per day per year of age beyond `OLD_AGE_YEARS`.
    pub const OLD_AGE_RATE: f32 = 0.0004;
    pub const OLD_AGE_YEARS: f32 = 8.0;
    pub const HEAL_RATE: f32 = 0.01;
    /// Healing needs food and warmth above this.
    pub const HEAL_ABOVE: f32 = 0.5;

    // Breeding and pairing.
    pub const MAX_BIRTH_AGE_YEARS: f32 = 10.0;
    /// Wanderers asking to join: arrival chance per day, and how many may wait at once.
    pub const JOIN_REQUEST_CHANCE_PER_DAY: f32 = 0.02;
    pub const JOIN_MAX_PENDING: usize = 3;
    /// Chance that a wanderer is an adult (otherwise a yearling).
    pub const JOIN_ADULT_SHARE: f32 = 0.8;
    pub const BREED_MIN_HEALTH: f32 = 0.5;
    pub const BREED_MIN_FOOD: f32 = 0.4;
    /// A litter has `MIN_LITTER + 0..LITTER_SPREAD` kits.
    pub const MIN_LITTER: u32 = 1;
    pub const LITTER_SPREAD: u32 = 3;
    pub const PAIR_MIN_HEALTH: f32 = 0.5;
    /// Daily chance that a wanderer arrives for an unpaired adult.
    pub const WANDERER_CHANCE_PER_DAY: f32 = 0.04;
    /// Distances tried, in order, when looking for a new lodge site.
    pub const LODGE_SITE_RADII: [f32; 6] = [6.0, 8.0, 10.0, 12.0, 14.0, 18.0];
    pub const LODGE_SITE_MAX_WATER_DIST: f32 = 3.0;
}

pub mod ecology {
    /// Ecology's time unit: wetland forms and drains over a few of these. It is a third of a
    /// calendar season (`calendar::SEASON_DAYS`), kept at 30 days so ecology keeps its tuned pace.
    pub const SEASON_DAYS: f32 = 30.0;
    /// Water deeper than this (world units) counts as flooding the cell.
    pub const FLOOD_DEPTH: f32 = 0.05;
    /// Trees drown once a cell has been flooded this long and is at least `DROWN_DEPTH` deep.
    pub const DROWN_FLOOD_DAYS: f32 = 20.0;
    pub const DROWN_DEPTH: f32 = 0.15;
    /// Forest density lost per flooded day once drowning starts.
    pub const DROWN_RATE: f32 = 0.1;
    /// Seasons of continuous flooding before a shallow cell turns to wetland.
    pub const WETLAND_SEASONS: f32 = 2.0;
    /// Deeper than this is open water, never wetland.
    pub const WETLAND_MAX_DEPTH: f32 = 1.5;
    /// Days after a felling during which the cell cannot regrow.
    pub const CUT_COOLDOWN_DAYS: f32 = 90.0;
    /// Distance (cells) beyond which water / lodges give no protection.
    pub const SAFE_RANGE: f32 = 12.0;
    pub const PREDATOR_RANGE: f32 = 10.0;

    // Forest.
    /// Starting forest density is uniform in `START_FOREST_MIN..START_FOREST_MIN + START_FOREST_SPREAD`.
    pub const START_FOREST_MIN: f32 = 0.6;
    pub const START_FOREST_SPREAD: f32 = 0.4;
    /// Daily growth rate: base + own density + mean neighbour density (spread).
    pub const FOREST_GROWTH_BASE: f32 = 0.0005;
    pub const FOREST_GROWTH_SELF: f32 = 0.01;
    pub const FOREST_GROWTH_SPREAD: f32 = 0.01;
    /// Days for a full snag to rot away.
    pub const SNAG_ROT_DAYS: f32 = 730.0;
    /// Days for a full meadow to give way to forest.
    pub const MEADOW_REGROW_DAYS: f32 = 730.0;
    /// Flood-day counter drains this many days per dry day.
    pub const FLOOD_DAYS_DRAIN_RATE: f32 = 2.0;
    /// Seasons for a full wetland to drain to meadow.
    pub const WETLAND_DRAIN_SEASONS: f32 = 2.0;

    // Wildlife (targets scale with nearby wetland `w`).
    /// Fraction of the gap to the wildlife target closed per day.
    pub const WILDLIFE_RATE: f32 = 0.05;
    /// Flood days at which a cell counts as fully stagnant (for mosquitoes).
    pub const STAGNANT_FLOOD_DAYS: f32 = 120.0;
    pub const WATERFOWL_FOREST_SHYNESS: f32 = 0.5;
    pub const HERON_SCALE: f32 = 0.5;
    pub const MOSQUITO_BASE: f32 = 0.3;
    pub const MOSQUITO_STAGNATION: f32 = 0.7;
    pub const OTTER_SCALE: f32 = 0.6;

    // Derived layers.
    pub const VEGETATION_MEADOW: f32 = 0.5;
    pub const VEGETATION_WETLAND: f32 = 0.5;
    pub const FOOD_FISH: f32 = 0.8;
    pub const FOOD_WATERFOWL: f32 = 0.6;
    pub const FOOD_FROGS: f32 = 0.2;
    pub const FOOD_FOREST: f32 = 0.5;
    pub const FOOD_MEADOW: f32 = 0.6;

    // Predators and danger.
    use crate::ecology::PredatorKind::{self, *};
    pub const START_PREDATORS: [PredatorKind; 6] = [Wolf, Wolf, Lynx, Bear, Eagle, Eagle];
    /// Land predators do not enter water deeper than this.
    pub const PREDATOR_MAX_WADE_DEPTH: f32 = 0.3;
    /// Cells moved per day.
    pub fn predator_speed(kind: PredatorKind) -> f32 {
        match kind { Wolf => 2.5, Lynx => 1.5, Bear => 1.0, Eagle => 4.0 }
    }
    /// Threat weight in the danger field.
    pub fn predator_threat(kind: PredatorKind) -> f32 {
        match kind { Wolf => 1.0, Lynx => 0.7, Bear => 0.9, Eagle => 0.5 }
    }
    /// Danger next to safety, plus the extra reached at `SAFE_RANGE`.
    pub const DANGER_BASE: f32 = 0.05;
    pub const DANGER_DISTANCE: f32 = 0.95;
    /// Danger multiplier: `BASE + SCALE * predator pressure`.
    pub const DANGER_PREDATOR_BASE: f32 = 0.5;
    pub const DANGER_PREDATOR_SCALE: f32 = 0.5;
    /// Share of danger a full hideout removes.
    pub const HIDEOUT_PROTECTION: f32 = 0.5;
}

pub mod water {
    /// Days in one season; a year is four seasons: spring, summer, autumn, winter.
    pub const SEASON_DAYS: f64 = crate::calendar::SEASON_DAYS;
    /// Depth below which a cell counts as dry (for ice formation).
    pub const WET_EPS: f32 = 0.01;
    /// Vegetation density above which a cell counts as holding trees.
    pub const TREE_VEGETATION: f32 = 0.5;
    /// Maximum ice thickness.
    pub const MAX_ICE: f32 = 1.0;

    // Seasonal multipliers, indexed by `Season as usize`: spring, summer, autumn, winter.
    /// River inflow: spring melt floods, summer drought.
    pub const INFLOW_MULT: [f32; 4] = [3.0, 0.3, 0.8, 0.1];
    pub const RAIN_MULT: [f32; 4] = [1.2, 0.4, 1.5, 0.3];
    pub const EVAP_MULT: [f32; 4] = [1.0, 3.0, 0.8, 0.2];

    // `WaterConfig` defaults (depth units per day per cell unless noted).
    pub const RAIN: f32 = 0.01;
    pub const EVAPORATION: f32 = 0.01;
    pub const SEEPAGE: f32 = 0.04;
    /// Rain per day dry ground absorbs (more than any season's rain, so dry land stays dry).
    pub const SOIL_ABSORB: f32 = 0.02;
    /// Integrity a dam loses per day from age alone.
    pub const DAM_DECAY_PER_DAY: f32 = 0.002;
    /// Integrity lost per day per unit of water depth above `OVERTOP_DEPTH` on the crest.
    pub const OVERTOP_EROSION: f32 = 1.0;
    pub const OVERTOP_DEPTH: f32 = 0.05;
    pub const ICE_GROWTH_PER_DAY: f32 = 0.1;
    pub const ICE_MELT_PER_DAY: f32 = 0.15;
    /// Depth a spring adds per day at base (scaled by the season) to its stream head.
    pub const SPRING_RATE: f32 = 1.0;
    /// Fraction of its channel a river is filled to at the start.
    /// Share of the seasonal inflow swing springs follow (groundwater runs steadier than snowmelt).
    pub const SPRING_SEASONALITY: f32 = 0.0;
    /// Days (and step size) a new world's springs run to settle the rivers before the game starts.
    pub const SETTLE_DAYS: f32 = 20.0;
    pub const SETTLE_DT: f32 = 0.05;
    pub const RIVER_FILL: f32 = 0.5;
}

pub mod buildings {
    /// Depth at or above which a cell counts as water.
    pub const WET_DEPTH: f32 = 0.05;
    /// Surface drop (per cell) below which a wheel does not turn.
    pub const MIN_FLOW: f32 = 0.01;
    /// Surface drop giving a wheel full output.
    pub const FULL_FLOW: f32 = 0.2;
    /// Base storage per good.
    pub const BASE_CAPACITY: f32 = 100.0;
    pub const WAREHOUSE_CAPACITY: f32 = 150.0;
    /// A dock holds a barge-load or two while goods wait to be shipped or fetched.
    pub const DOCK_CAPACITY: f32 = 60.0;
    pub const FOOD_CACHE_CAPACITY: f32 = 100.0;
    /// Barge cargo limit and speed (cells/day).
    pub const BARGE_CAPACITY: f32 = 20.0;
    pub const BARGE_SPEED: f32 = 4.0;
    /// Workshop batches per day at base speed.
    pub const CRAFT_BATCHES_PER_DAY: f32 = 2.0;
    /// Extra crafting speed of a workshop with a full share of wheel power (1.0 doubles it).
    pub const WHEEL_MAX_BOOST: f32 = 1.0;
    /// Mill (advanced workshop) batches per day at base speed, shared by its two workers.
    pub const MILL_BATCHES_PER_DAY: f32 = 6.0;
    pub const STOREHOUSE_CAPACITY: f32 = 400.0;

    /// Workshop-style batches per day of `kind` (0 if it does not craft).
    pub fn batches_per_day(kind: BuildingKind) -> f32 {
        match kind {
            BuildingKind::Workshop => CRAFT_BATCHES_PER_DAY,
            BuildingKind::Mill => MILL_BATCHES_PER_DAY,
            _ => 0.0,
        }
    }
    /// Storage `kind` adds per good it accepts (0 if it is not a store).
    pub fn store_capacity(kind: BuildingKind) -> f32 {
        match kind {
            BuildingKind::Warehouse => WAREHOUSE_CAPACITY,
            BuildingKind::Dock => DOCK_CAPACITY,
            BuildingKind::FoodCache => FOOD_CACHE_CAPACITY,
            BuildingKind::Storehouse => STOREHOUSE_CAPACITY,
            _ => 0.0,
        }
    }

    use crate::buildings::{Amount, BuildingKind, Good::*, Recipe};

    /// Material cost and builder work units (one adult working day is about 0.6) of a building.
    pub fn cost_and_work(kind: BuildingKind) -> (&'static [Amount], f32) {
        match kind {
            // Felled trees only: the first dam is built from what the family can fell and carry.
            BuildingKind::Dam => (&[(Logs, 15.0), (Sticks, 20.0)], 8.0),
            BuildingKind::Lodge => (&[(Logs, 20.0), (Sticks, 25.0), (Mud, 20.0)], 12.0),
            // Dug by the adults from the bank: labour only, no materials.
            BuildingKind::Burrow => (&[], 6.0),
            BuildingKind::FoodCache => (&[(Logs, 6.0), (Sticks, 10.0), (Mud, 6.0)], 4.0),
            BuildingKind::ScentMound => (&[(Mud, 6.0), (Sticks, 4.0)], 1.5),
            BuildingKind::GuardPost => (&[(Logs, 8.0), (Sticks, 8.0), (Stone, 3.0)], 3.0),
            BuildingKind::Canal => (&[(Mud, 2.0)], 1.0),
            BuildingKind::Lock => (&[(Logs, 10.0), (Stone, 6.0), (Mud, 8.0), (Tools, 1.0)], 6.0),
            BuildingKind::WaterWheel => (&[(Logs, 12.0), (Rope, 2.0), (Tools, 1.0)], 7.0),
            BuildingKind::Workshop => (&[(Logs, 8.0), (Sticks, 6.0), (Mud, 4.0)], 6.0),
            BuildingKind::Warehouse => (&[(Logs, 25.0), (Sticks, 15.0), (Mud, 10.0), (Rope, 2.0)], 10.0),
            // A jetty of logs and sticks; a barge needs rope for its lines.
            BuildingKind::Dock => (&[(Logs, 10.0), (Sticks, 6.0)], 4.0),
            BuildingKind::Barge => (&[(Logs, 12.0), (Sticks, 8.0), (Rope, 2.0)], 5.0),
            BuildingKind::LoggingCamp => (&[(Logs, 4.0), (Sticks, 6.0)], 2.0),
            BuildingKind::ForagerHut => (&[(Sticks, 8.0), (Mud, 4.0)], 2.0),
            BuildingKind::MudPit => (&[(Logs, 3.0), (Sticks, 4.0)], 2.0),
            BuildingKind::QuarryCamp => (&[(Logs, 5.0), (Sticks, 6.0), (Mud, 3.0)], 3.0),
            BuildingKind::Mill => (&[(Logs, 30.0), (Stone, 10.0), (Rope, 4.0), (Tools, 3.0)], 14.0),
            BuildingKind::Storehouse => (&[(Logs, 40.0), (Sticks, 20.0), (Stone, 12.0), (Rope, 4.0)], 16.0),
            BuildingKind::ForesterLodge => (&[(Logs, 4.0), (Sticks, 8.0), (Mud, 2.0)], 3.0),
            BuildingKind::TreeNursery => (&[(Logs, 6.0), (Sticks, 6.0), (Mud, 4.0), (Baskets, 1.0)], 4.0),
        }
    }

    /// Inputs and output of one workshop batch.
    pub fn recipe_batch(recipe: Recipe) -> (&'static [Amount], Amount) {
        match recipe {
            Recipe::Baskets => (&[(Sticks, 2.0)], (Baskets, 1.0)),
            Recipe::Rope => (&[(Sticks, 1.0), (Mud, 0.5)], (Rope, 1.0)),
            Recipe::Tools => (&[(Logs, 1.0), (Stone, 1.0)], (Tools, 1.0)),
        }
    }
}

pub mod region {
    /// Friendliness below this refuses contact.
    pub const WARY_BELOW: f32 = 0.3;
    pub const MAX_POPULATION: f32 = 40.0;
    pub const MAX_STORE: f32 = 500.0;
    pub const OFFER_LIFETIME_DAYS: f64 = 15.0;
    pub const MAX_OPEN_OFFERS: usize = 3;

    // Generation (each value is `MIN + SPREAD * uniform(0..1)`).
    /// Number of neighbours is `MIN_NEIGHBOURS + 0..NEIGHBOUR_SPREAD`.
    pub const MIN_NEIGHBOURS: u32 = 3;
    pub const NEIGHBOUR_SPREAD: u32 = 3;
    /// Share of neighbours generated wary (below `WARY_BELOW`).
    pub const WARY_CHANCE: f32 = 0.2;
    pub const WARY_FRIENDLINESS_MIN: f32 = 0.05;
    pub const WARY_FRIENDLINESS_SPREAD: f32 = 0.2;
    pub const FRIENDLINESS_MIN: f32 = 0.5;
    pub const FRIENDLINESS_SPREAD: f32 = 0.5;
    pub const START_POPULATION_MIN: f32 = 6.0;
    pub const START_POPULATION_SPREAD: f32 = 14.0;
    /// Side of the square region map neighbours are placed on.
    pub const REGION_EXTENT: f32 = 100.0;
    pub const START_FOOD_PER_HEAD_MIN: f32 = 2.0;
    pub const START_FOOD_PER_HEAD_SPREAD: f32 = 4.0;
    pub const START_WOOD_PER_HEAD_MIN: f32 = 1.0;
    pub const START_WOOD_PER_HEAD_SPREAD: f32 = 2.0;

    // Daily economy, per head.
    pub const BASE_FOOD_GAIN: f32 = 0.04;
    pub const BASE_WOOD_GAIN: f32 = 0.015;
    pub const FOOD_USE: f32 = 0.05;
    pub const WOOD_USE: f32 = 0.02;
    use crate::region::Specialty;
    /// Extra (food, wood) gathered per head per day by a neighbour's specialty.
    pub fn specialty_gain(specialty: Specialty) -> (f32, f32) {
        match specialty {
            Specialty::Fishing => (0.06, 0.0),
            Specialty::Farming => (0.08, 0.0),
            Specialty::Lumber => (0.0, 0.07),
            Specialty::Crafts => (0.0, 0.04),
        }
    }
    /// Stock per head a neighbour keeps: it grows above it and trades only the surplus beyond it.
    pub const RESERVE_PER_HEAD: f32 = 3.0;
    pub const GROWTH_RATE: f32 = 0.01;
    pub const DECLINE_RATE: f32 = 0.01;
    pub const MIN_POPULATION: f32 = 2.0;

    // Migration and trade (chances are per day).
    /// Neighbours above this share of `MAX_POPULATION` send migrants.
    pub const CROWDED_SHARE: f32 = 0.8;
    /// Daily migration chance, scaled by friendliness.
    pub const MIGRATION_CHANCE: f32 = 0.1;
    pub const MIN_OFFER_SURPLUS: f32 = 4.0;
    pub const OFFER_CHANCE: f32 = 0.15;
    /// Share of the surplus offered, and the smallest offer.
    pub const OFFER_SHARE_MIN: f32 = 0.2;
    pub const OFFER_SHARE_SPREAD: f32 = 0.3;
    pub const MIN_OFFER: f32 = 2.0;
    /// Asking price as a multiple of the amount given.
    pub const PRICE_MIN: f32 = 0.7;
    pub const PRICE_SPREAD: f32 = 0.6;
}

pub mod events {
    /// Default calendar length of a season, in days.
    pub const DEFAULT_DAYS_PER_SEASON: u32 = crate::calendar::SEASON_DAYS as u32;
    /// Days an unresolved choice waits before its default (first) option is applied.
    pub const CHOICE_EXPIRY_DAYS: f64 = 5.0;
    /// Water inflow multiplier while a drought lasts.
    pub const DROUGHT_INFLOW: f32 = 0.4;

    // Daily probabilities and the conditions that gate them.
    /// Flood: `CHANCE * season mult * (WATER_BASE + water level)`, spring and summer only.
    pub const FLOOD_CHANCE: f32 = 0.02;
    pub const FLOOD_SPRING_MULT: f32 = 2.0;
    pub const FLOOD_SUMMER_MULT: f32 = 0.7;
    pub const FLOOD_WATER_BASE: f32 = 0.5;
    pub const FLOOD_MIN_WATER_LEVEL: f32 = 0.2;
    /// Drought: `CHANCE * (WATER_BASE - water level)`, summer and autumn only.
    pub const DROUGHT_CHANCE: f32 = 0.012;
    pub const DROUGHT_WATER_BASE: f32 = 1.5;
    pub const HARD_FREEZE_CHANCE: f32 = 0.03;
    /// Fire: `CHANCE * forest`, times `DROUGHT_MULT` during a drought.
    pub const FIRE_CHANCE: f32 = 0.012;
    pub const FIRE_DROUGHT_MULT: f32 = 3.0;
    pub const FIRE_MIN_FOREST: f32 = 0.2;
    /// Disease: `CHANCE * (MOSQUITO_BASE + MOSQUITO_SCALE * mosquitoes) * crowding`, where crowding
    /// is `population / CROWD_POPULATION`, at most `CROWD_MAX`.
    pub const DISEASE_CHANCE: f32 = 0.01;
    pub const DISEASE_MOSQUITO_BASE: f32 = 0.5;
    pub const DISEASE_MOSQUITO_SCALE: f32 = 2.0;
    pub const DISEASE_CROWD_POPULATION: f32 = 10.0;
    pub const DISEASE_CROWD_MAX: f32 = 2.0;
    pub const DISEASE_MIN_POPULATION: u32 = 6;
    /// Predator raid: `CHANCE * danger`.
    pub const PREDATOR_RAID_CHANCE: f32 = 0.04;
    pub const WANDERING_BEAVER_CHANCE: f32 = 0.03;
    /// Wetland arrival: `CHANCE * wetland`, spring and summer only.
    pub const WETLAND_ARRIVAL_CHANCE: f32 = 0.03;
    pub const WETLAND_ARRIVAL_MIN_WETLAND: f32 = 0.1;

    // Effects. `(lo, hi)` pairs are uniform ranges; losses marked "per sqrt pop" scale with the
    // square root of the population.
    /// Wood and food lost to a flood, per sqrt pop.
    pub const FLOOD_WOOD_LOSS: (f32, f32) = (2.0, 6.0);
    pub const FLOOD_FOOD_LOSS: (f32, f32) = (1.0, 4.0);
    /// Drought length in seasons.
    pub const DROUGHT_SEASONS: (f32, f32) = (2.0, 4.0);
    /// Food lost to a hard freeze, per sqrt pop, and the illness it brings.
    pub const HARD_FREEZE_FOOD_LOSS: (f32, f32) = (2.0, 5.0);
    pub const HARD_FREEZE_ILLNESS: f32 = 0.1;
    /// Wood a fire burns if left alone, per sqrt pop.
    pub const FIRE_WOOD_LOSS: (f32, f32) = (4.0, 10.0);
    /// Fighting the fire saves all but this share of the wood, at an illness cost.
    pub const FIRE_FIGHT_LOSS_SHARE: f32 = 0.3;
    pub const FIRE_FIGHT_ILLNESS: f32 = 0.05;
    /// Letting it burn also loses this share of the wood loss as food.
    pub const FIRE_BURN_FOOD_SHARE: f32 = 0.3;
    pub const DISEASE_ILLNESS: (f32, f32) = (0.1, 0.3);
    /// Share of the population a disease kills (rounded down).
    pub const DISEASE_DEATH_SHARE: (f32, f32) = (0.0, 0.08);
    /// Share of the population a raid would kill (rounded up, at least `RAID_MIN_DEATHS`);
    /// standing to fight divides the deaths by `RAID_FIGHT_DEATH_DIVISOR`.
    pub const RAID_DEATH_SHARE: (f32, f32) = (0.03, 0.12);
    pub const RAID_MIN_DEATHS: u32 = 1;
    pub const RAID_FIGHT_DEATH_DIVISOR: u32 = 2;
    pub const RAID_FIGHT_ILLNESS: f32 = 0.05;
    /// Food lost hiding from a raid, per sqrt pop.
    pub const RAID_HIDE_FOOD_LOSS: (f32, f32) = (2.0, 6.0);
    /// Wanderers number `MIN + floor(uniform * SPREAD)`; each costs `WANDERER_FOOD_COST` food.
    pub const WANDERERS_MIN: u32 = 1;
    pub const WANDERERS_SPREAD: f32 = 2.0;
    pub const WANDERER_FOOD_COST: f32 = 2.0;
    /// Food from wetland arrivals, times `1 + wetland`.
    pub const WETLAND_ARRIVAL_FOOD: (f32, f32) = (3.0, 8.0);
}
pub mod tech {
    /// Day on which the first winter has passed (360-day year) and `Achievement::FirstWinter` is earned.
    pub const YEAR_DAYS: f64 = crate::calendar::YEAR_DAYS;
    /// Branch experience needed for a tier-1 tech.
    pub const TIER1_XP: f32 = 3.0;
    /// Branch experience needed for a tier-2 tech.
    pub const TIER2_XP: f32 = 15.0;
    /// Branch experience needed for a tier-3 (industrial) tech.
    pub const TIER3_XP: f32 = 40.0;
    /// Forest Lore: storage capacity multiplier.
    pub const STORAGE_BONUS: f32 = 1.25;
    /// Master Builders: build work multiplier.
    pub const BUILD_WORK_BONUS: f32 = 0.75;
    /// Fine Tools: workshop crafting speed multiplier.
    pub const CRAFT_SPEED_BONUS: f32 = 1.5;
}
