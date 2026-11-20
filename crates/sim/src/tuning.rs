pub mod colony {
    pub const MAX_IMMIGRANTS_PER_YEAR: u32 = 2;
    pub const KIT_DAYS: f32 = crate::calendar::YEAR_DAYS as f32;
    pub const ADULT_DAYS: f32 = 2.0 * crate::calendar::YEAR_DAYS as f32;
    pub const LODGE_CAPACITY: usize = 12;
    pub const MAX_WATER_SCAN: f32 = 10.0;
    pub const WORK_FRACTION: f32 = 0.6;
    pub const DEPARTURE_BAD_MOOD_DAYS: f32 = 30.0;
    pub const SPRING_START: f64 = 0.0;
    pub const SPRING_END: f64 = crate::calendar::SEASON_DAYS;
    pub const MATING_FROM_DAY: f64 = 300.0;
    pub const MATING_TO_DAY: f64 = 330.0;
    pub const GESTATION_DAYS: f64 = 107.0;
    pub const MIDSUMMER_DAY: f64 = 1.5 * crate::calendar::SEASON_DAYS;

    pub const WATER_DEPTH: f32 = 0.05;
    pub const DEFAULT_RIVER_HALF_WIDTH: f32 = 2.0;

    pub const KIT_WORK_WEIGHT: f32 = 0.0;
    pub const YEARLING_WORK_WEIGHT: f32 = 0.5;
    pub const KIT_FOOD_WEIGHT: f32 = 0.4;
    pub const YEARLING_FOOD_WEIGHT: f32 = 0.7;

    pub const FOUNDER_ADULT_AGE_YEARS: f32 = 3.0;
    pub const FOUNDER_YEARLING_AGE_YEARS: f32 = 1.5;
    pub const START_FOOD: f32 = 40.0;
    pub const START_WOOD: f32 = 20.0;
    pub const FOUNDING_LODGE_CONDITION: f32 = 1.0;
    pub const START_SOCIAL: f32 = 0.7;
    pub const START_MOOD: f32 = 0.9;
    pub const START_SKILL_MAX: f32 = 0.1;
    pub const LIFESPAN_MIN_YEARS: f32 = 10.0;
    pub const LIFESPAN_SPREAD_YEARS: f32 = 4.0;
    pub const IMMIGRANT_MIN_AGE_YEARS: f32 = 2.0;
    pub const IMMIGRANT_AGE_SPREAD_YEARS: f32 = 2.0;

    pub const COLD_BELOW_TEMP: f32 = 0.2;
    pub const COLD_RANGE: f32 = 1.2;

    pub const HUNGRY_FOOD_PER_CAPITA: f32 = 4.0;
    pub const BIRTH_MIN_FOOD_DAYS: f32 = 12.0;

    pub const SWIM_COST: f32 = 0.5;
    pub const WORK_REST_BASE: f32 = 0.3;
    pub const WORK_REST_SCALE: f32 = 0.7;
    pub const WORK_HEALTH_BASE: f32 = 0.5;
    pub const WORK_HEALTH_SCALE: f32 = 0.5;
    pub const WORK_MOOD_BASE: f32 = 0.7;
    pub const WORK_MOOD_SCALE: f32 = 0.3;
    pub const SKILL_BASE: f32 = 0.5;
    pub const SKILL_GAIN: f32 = 0.01;
    pub const SPECIALIST_SKILL: f32 = 0.5;
    pub const SPECIALIST_BONUS: f32 = 1.25;
    pub const YOUNG_LEARN_RATE: f32 = 0.002;

    pub const GUARD_MITIGATION: f32 = 0.65;
    pub const WORK_EXPOSURE_BASE: f32 = 0.004;
    pub const WORK_EXPOSURE_PER_LAND_CELL: f32 = 0.003;
    pub const WORK_EXPOSURE_PER_WATER_DIST: f32 = 0.002;
    pub const SAFETY_EXPOSURE_SCALE: f32 = 6.0;
    pub const SAFETY_MIN: f32 = 0.2;

    pub const GUARD_SAFETY_PER_STRENGTH: f32 = 0.2;
    pub const GUARD_SAFETY_MAX: f32 = 0.2;
    pub const OPEN_INSULATION: f32 = 0.05;
    pub const OPEN_REST: f32 = 0.4;
    pub const LODGE_INSULATION: f32 = 0.85;
    pub const HUNGER_PER_DAY: f32 = 0.25;
    pub const EAT_BELOW: f32 = 0.7;
    pub const IDLE_FRACTION: f32 = 0.1;
    pub const TIRE_RATE: f32 = 0.20;
    pub const SHELTER_REST_RATE: f32 = 0.40;
    pub const RESTING_RECOVERY: f32 = 0.3;
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
    pub const LOW_MOOD_BELOW: f32 = 0.2;
    pub const FOOD_SPOILAGE: f32 = 0.003;

    pub const KIT_FRAILTY: f32 = 1.5;
    pub const STARVE_RATE: f32 = 0.02;
    pub const STARVE_BELOW: f32 = 0.15;
    pub const FREEZE_RATE: f32 = 0.015;
    pub const FREEZE_BELOW: f32 = 0.2;
    pub const EXHAUST_RATE: f32 = 0.01;
    pub const EXHAUST_BELOW: f32 = 0.05;
    pub const OLD_AGE_RATE: f32 = 0.0004;
    pub const OLD_AGE_YEARS: f32 = 8.0;
    pub const HEAL_RATE: f32 = 0.004;
    pub const HEAL_ABOVE: f32 = 0.5;

    pub const MAX_BIRTH_AGE_YEARS: f32 = 10.0;
    pub const JOIN_REQUEST_CHANCE_PER_DAY: f32 = 1.0 / 120.0;
    pub const JOIN_MAX_PENDING: usize = 3;
    pub const JOIN_ADULT_SHARE: f32 = 0.8;
    pub const BREED_MIN_HEALTH: f32 = 0.5;
    pub const BREED_MIN_FOOD: f32 = 0.4;
    pub const MIN_LITTER: u32 = 2;
    pub const LITTER_SPREAD: u32 = 3;
    pub const PAIR_MIN_HEALTH: f32 = 0.5;
    pub const WANDERER_CHANCE_PER_DAY: f32 = 0.04;
}

pub mod ecology {
    pub const SEASON_DAYS: f32 = 30.0;
    pub const FLOOD_DEPTH: f32 = 0.05;
    pub const DROWN_FLOOD_DAYS: f32 = 20.0;
    pub const DROWN_DEPTH: f32 = 0.15;
    pub const DROWN_RATE: f32 = 0.1;
    pub const WETLAND_SEASONS: f32 = 2.0;
    pub const WETLAND_MAX_DEPTH: f32 = 1.5;
    pub const SAFE_RANGE: f32 = 40.0;

    pub const START_FOREST_MIN: f32 = 0.6;
    pub const START_FOREST_SPREAD: f32 = 0.4;
    pub const SNAG_ROT_DAYS: f32 = 730.0;
    pub const MEADOW_REGROW_DAYS: f32 = 730.0;
    pub const FLOOD_DAYS_DRAIN_RATE: f32 = 2.0;
    pub const WETLAND_DRAIN_SEASONS: f32 = 2.0;

    pub const WILDLIFE_RATE: f32 = 0.05;
    pub const STAGNANT_FLOOD_DAYS: f32 = 120.0;
    pub const WATERFOWL_FOREST_SHYNESS: f32 = 0.5;
    pub const HERON_SCALE: f32 = 0.5;
    pub const MOSQUITO_BASE: f32 = 0.3;
    pub const MOSQUITO_STAGNATION: f32 = 0.7;
    pub const OTTER_SCALE: f32 = 0.6;

    pub const VEGETATION_MEADOW: f32 = 0.5;
    pub const VEGETATION_WETLAND: f32 = 0.5;
    pub const FOOD_FISH: f32 = 0.8;
    pub const FOOD_WATERFOWL: f32 = 0.6;
    pub const FOOD_FROGS: f32 = 0.2;
    pub const FOOD_FOREST: f32 = 0.5;
    pub const FOOD_MEADOW: f32 = 0.6;

    pub const DANGER_BASE: f32 = 0.05;
    pub const DANGER_DISTANCE: f32 = 0.95;
    pub const DANGER_PREDATOR_BASE: f32 = 0.5;
    pub const HIDEOUT_PROTECTION: f32 = 0.5;
}

pub mod water {
    pub const SEASON_DAYS: f64 = crate::calendar::SEASON_DAYS;
    pub const WET_EPS: f32 = 0.01;
    pub const TREE_VEGETATION: f32 = 0.5;
    pub const MAX_ICE: f32 = 1.0;
    pub const INFLOW_MULT: [f32; 4] = [3.0, 0.3, 0.8, 0.1];
    pub const RAIN_MULT: [f32; 4] = [1.2, 0.4, 1.5, 0.3];
    pub const EVAP_MULT: [f32; 4] = [1.0, 3.0, 0.8, 0.2];
    pub const RAIN: f32 = 0.002;
    pub const EVAPORATION: f32 = 0.002;
    pub const SEEPAGE: f32 = 0.002;
    pub const DAM_DECAY_PER_DAY: f32 = 0.002;
    pub const OVERTOP_EROSION: f32 = 1.0;
    pub const OVERTOP_DEPTH: f32 = 0.15;
    pub const ICE_GROWTH_PER_DAY: f32 = 0.1;
    pub const ICE_MELT_PER_DAY: f32 = 0.15;
    pub const RUNOFF_M3S_PER_KM2: f32 = 0.01;
    pub const WATER_PER_CATCHMENT_CELL: f32 = RUNOFF_M3S_PER_KM2 * 1.0e-6 * 86_400.0;
    pub const MANNING_CHANNEL: f32 = 0.04;
    pub const WEIR: f32 = 1.7;
}

pub mod buildings {
    pub const WET_DEPTH: f32 = 0.05;
    pub const MIN_FLOW: f32 = 0.01;
    pub const FULL_FLOW: f32 = 0.2;
    pub const BASE_CAPACITY: f32 = 100.0;
}
