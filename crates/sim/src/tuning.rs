pub mod colony {
    pub const MAX_IMMIGRANTS_PER_YEAR: u32 = 2;
    pub const KIT_DAYS: f32 = crate::calendar::YEAR_DAYS as f32;
    pub const ADULT_DAYS: f32 = 2.0 * crate::calendar::YEAR_DAYS as f32;
    pub const LODGE_CAPACITY: usize = 12;
    pub const LODGE_WOOD_COST: f32 = 8.0;
    pub const MIN_LODGE_SPACING: f32 = 15.0;
    pub const MAX_WATER_SCAN: f32 = 10.0;
    pub const WORK_FRACTION: f32 = 0.6;
    pub const CACHE_LIFETIME_DAYS: f64 = 30.0;
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
    pub const NEW_LODGE_CONDITION: f32 = 0.7;
    pub const START_SOCIAL: f32 = 0.7;
    pub const START_MOOD: f32 = 0.9;
    pub const START_SKILL_MAX: f32 = 0.1;
    pub const LIFESPAN_MIN_YEARS: f32 = 10.0;
    pub const LIFESPAN_SPREAD_YEARS: f32 = 4.0;
    pub const IMMIGRANT_MIN_AGE_YEARS: f32 = 2.0;
    pub const IMMIGRANT_AGE_SPREAD_YEARS: f32 = 2.0;
    pub const DEFAULT_PRIORITIES: [crate::colony::Job; 8] = {
        use crate::colony::Job::*;
        [Forager, Forager, Logger, Builder, Hauler, Guard, Crafter, Scout]
    };

    pub const COLD_BELOW_TEMP: f32 = 0.2;
    pub const COLD_RANGE: f32 = 1.2;
    pub const FORAGE_SEASON_BASE: f32 = 0.75;
    pub const FORAGE_SEASON_TEMP: f32 = 0.25;
    pub const FORAGE_SEASON_MIN: f32 = 0.5;

    pub const HUNGRY_FOOD_PER_CAPITA: f32 = 4.0;
    pub const BIRTH_MIN_FOOD_DAYS: f32 = 12.0;
    pub const BIRTH_MAX_CROWDING: f32 = 1.0;
    pub const REPAIR_BELOW_CONDITION: f32 = 0.6;
    pub const REPAIR_MIN_WOOD: f32 = 2.0;

    pub const WORK_SITE_DISTANCE: f32 = 3.0;
    pub const SWIM_COST: f32 = 0.5;
    pub const TRAVEL_COST_PENALTY: f32 = 0.02;
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
    pub const FORAGE_CROWDING_POPULATION: f32 = 100.0;
    pub const HAUL_BOOST_PER_EFFORT: f32 = 0.15;
    pub const HAUL_BOOST_MAX: f32 = 0.6;
    pub const TOOLS_BOOST: f32 = 0.3;
    pub const SCOUT_BONUS: f32 = 0.1;
    pub const SCOUT_WORK_PER_BONUS: f32 = 20.0;
    pub const SCOUT_BONUS_MAX_STEPS: f32 = 2.0;
    pub const FOOD_CAP_BASE: f32 = 10.0;
    pub const FOOD_CAP_PER_BEAVER: f32 = 25.0;
    pub const WOOD_CAP_BASE: f32 = 60.0;
    pub const WOOD_CAP_PER_BEAVER: f32 = 10.0;
    pub const FORAGE_YIELD: f32 = 4.0;
    pub const LOG_YIELD: f32 = 1.2;
    pub const REPAIR_YIELD: f32 = 0.04;
    pub const TOOLS_YIELD: f32 = 0.05;
    pub const REPAIR_WOOD_PER_CONDITION: f32 = 4.0;

    pub const GUARD_MITIGATION: f32 = 0.65;
    pub const CANAL_MITIGATION: f32 = 0.03;
    pub const CANAL_MITIGATION_MAX_DUG: f32 = 10.0;
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
    pub const TOOLS_WEAR: f32 = 0.003;
    pub const LODGE_WEAR: f32 = 0.002;
    pub const DRY_HOME_WEAR: f32 = 0.03;
    pub const LODGE_COLD_WEAR: f32 = 2.0;

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
    pub const LODGE_SITE_RADII: [f32; 6] = [15.0, 20.0, 25.0, 30.0, 40.0, 50.0];
    pub const LODGE_SITE_MAX_WATER_DIST: f32 = 3.0;
}

pub mod ecology {
    pub const SEASON_DAYS: f32 = 30.0;
    pub const FLOOD_DEPTH: f32 = 0.05;
    pub const DROWN_FLOOD_DAYS: f32 = 20.0;
    pub const DROWN_DEPTH: f32 = 0.15;
    pub const DROWN_RATE: f32 = 0.1;
    pub const WETLAND_SEASONS: f32 = 2.0;
    pub const WETLAND_MAX_DEPTH: f32 = 1.5;
    pub const CUT_COOLDOWN_DAYS: f32 = 90.0;
    pub const SAFE_RANGE: f32 = 40.0;
    pub const PREDATOR_RANGE: f32 = 30.0;

    pub const START_FOREST_MIN: f32 = 0.6;
    pub const START_FOREST_SPREAD: f32 = 0.4;
    pub const FOREST_GROWTH_BASE: f32 = 0.0005;
    pub const FOREST_GROWTH_SELF: f32 = 0.01;
    pub const FOREST_GROWTH_SPREAD: f32 = 0.01;
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

    use crate::ecology::PredatorKind::{self, *};
    pub const START_PREDATORS: [PredatorKind; 6] = [Wolf, Wolf, Lynx, Bear, Eagle, Eagle];
    pub const PREDATOR_MAX_WADE_DEPTH: f32 = 0.3;
    pub fn predator_speed(kind: PredatorKind) -> f32 {
        match kind { Wolf => 2.5, Lynx => 1.5, Bear => 1.0, Eagle => 4.0 }
    }
    pub fn predator_threat(kind: PredatorKind) -> f32 {
        match kind { Wolf => 1.0, Lynx => 0.7, Bear => 0.9, Eagle => 0.5 }
    }
    pub const DANGER_BASE: f32 = 0.05;
    pub const DANGER_DISTANCE: f32 = 0.95;
    pub const DANGER_PREDATOR_BASE: f32 = 0.5;
    pub const DANGER_PREDATOR_SCALE: f32 = 0.5;
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
    pub const BANKFULL_PER_MEAN: f32 = 25.0;
    pub const BANK_WIDTH: (f32, f32) = (3.0, 0.5);
    pub const BANK_DEPTH: (f32, f32) = (0.35, 0.4);
    pub const WETTED_WIDTH_EXP: f32 = 0.26;
    pub const MANNING_CHANNEL: f32 = 0.04;
    pub const MANNING_OVERLAND: f32 = 0.08;
    pub const WEIR: f32 = 1.7;
    pub const RELEASE_DAYS: f32 = 0.1;
}

pub mod buildings {
    pub const WET_DEPTH: f32 = 0.05;
    pub const MIN_FLOW: f32 = 0.01;
    pub const FULL_FLOW: f32 = 0.2;
    pub const BASE_CAPACITY: f32 = 100.0;
    pub const WAREHOUSE_CAPACITY: f32 = 150.0;
    pub const DOCK_CAPACITY: f32 = 60.0;
    pub const FOOD_CACHE_CAPACITY: f32 = 100.0;
    pub const BARGE_CAPACITY: f32 = 20.0;
    pub const BARGE_SPEED: f32 = 4.0;
    pub const CRAFT_BATCHES_PER_DAY: f32 = 2.0;
    pub const WHEEL_MAX_BOOST: f32 = 1.0;
    pub const MILL_BATCHES_PER_DAY: f32 = 6.0;
    pub const STOREHOUSE_CAPACITY: f32 = 400.0;

    pub fn batches_per_day(kind: BuildingKind) -> f32 {
        match kind {
            BuildingKind::Workshop => CRAFT_BATCHES_PER_DAY,
            BuildingKind::Mill => MILL_BATCHES_PER_DAY,
            _ => 0.0,
        }
    }
    pub fn store_capacity(kind: BuildingKind) -> f32 {
        match kind {
            BuildingKind::Warehouse => WAREHOUSE_CAPACITY,
            BuildingKind::Dock => DOCK_CAPACITY,
            BuildingKind::FoodCache => FOOD_CACHE_CAPACITY,
            BuildingKind::Storehouse => STOREHOUSE_CAPACITY,
            _ => 0.0,
        }
    }

    use crate::buildings::{Amount, BuildingKind, Good::*};

    pub fn cost_and_work(kind: BuildingKind) -> (&'static [Amount], f32) {
        use BuildingKind::*;
        match kind {
            Dam => (&[(Sticks,30.0),(Clay,4.0)],7.0),
            Lodge => (&[(Sticks,36.0),(Clay,16.0)],10.0),
            Burrow => (&[],5.0),
            FoodCache => (&[(Sticks,12.0),(Clay,5.0)],3.5),
            ScentMound => (&[(Clay,5.0),(Sticks,3.0)],1.5),
            GuardPost => (&[(Sticks,16.0),(Clay,3.0)],3.0),
            Canal => (&[(Clay,1.5)],0.8),
            Lock => (&[(Sticks,22.0),(Clay,10.0)],6.0),
            WaterWheel => (&[(Sticks,26.0),(Clay,4.0)],7.0),
            Workshop => (&[(Sticks,18.0),(Clay,6.0)],5.0),
            Warehouse => (&[(Sticks,32.0),(Clay,10.0)],8.0),
            Dock => (&[(Sticks,16.0),(Clay,4.0)],4.0),
            Barge => (&[(Sticks,24.0),(Clay,4.0)],5.0),
            LoggingCamp => (&[(Sticks,8.0)],2.0),
            ForagerHut => (&[(Sticks,6.0),(Clay,3.0)],2.0),
            MudPit => (&[(Sticks,6.0)],2.0),
            Mill => (&[(Sticks,50.0),(Clay,14.0)],12.0),
            Storehouse => (&[(Sticks,58.0),(Clay,16.0)],14.0),
            ForesterLodge => (&[(Sticks,10.0),(Clay,3.0)],3.0),
            TreeNursery => (&[(Sticks,12.0),(Clay,6.0)],4.0),
        }
    }


}

pub mod region {
    pub const WARY_BELOW: f32 = 0.3;
    pub const MAX_POPULATION: f32 = 40.0;
    pub const MAX_STORE: f32 = 500.0;
    pub const OFFER_LIFETIME_DAYS: f64 = 15.0;
    pub const MAX_OPEN_OFFERS: usize = 3;

    pub const MIN_NEIGHBOURS: u32 = 3;
    pub const NEIGHBOUR_SPREAD: u32 = 3;
    pub const WARY_CHANCE: f32 = 0.2;
    pub const WARY_FRIENDLINESS_MIN: f32 = 0.05;
    pub const WARY_FRIENDLINESS_SPREAD: f32 = 0.2;
    pub const FRIENDLINESS_MIN: f32 = 0.5;
    pub const FRIENDLINESS_SPREAD: f32 = 0.5;
    pub const START_POPULATION_MIN: f32 = 6.0;
    pub const START_POPULATION_SPREAD: f32 = 14.0;
    pub const REGION_EXTENT: f32 = 100.0;
    pub const START_FOOD_PER_HEAD_MIN: f32 = 2.0;
    pub const START_FOOD_PER_HEAD_SPREAD: f32 = 4.0;
    pub const START_WOOD_PER_HEAD_MIN: f32 = 1.0;
    pub const START_WOOD_PER_HEAD_SPREAD: f32 = 2.0;

    pub const BASE_FOOD_GAIN: f32 = 0.04;
    pub const BASE_WOOD_GAIN: f32 = 0.015;
    pub const FOOD_USE: f32 = 0.05;
    pub const WOOD_USE: f32 = 0.02;
    use crate::region::Specialty;
    pub fn specialty_gain(specialty: Specialty) -> (f32, f32) {
        match specialty {
            Specialty::Fishing => (0.06, 0.0),
            Specialty::Farming => (0.08, 0.0),
            Specialty::Lumber => (0.0, 0.07),
            Specialty::Crafts => (0.0, 0.04),
        }
    }
    pub const RESERVE_PER_HEAD: f32 = 3.0;
    pub const GROWTH_RATE: f32 = 0.01;
    pub const DECLINE_RATE: f32 = 0.01;
    pub const MIN_POPULATION: f32 = 2.0;

    pub const CROWDED_SHARE: f32 = 0.8;
    pub const MIGRATION_CHANCE: f32 = 0.1;
    pub const MIN_OFFER_SURPLUS: f32 = 4.0;
    pub const OFFER_CHANCE: f32 = 0.15;
    pub const OFFER_SHARE_MIN: f32 = 0.2;
    pub const OFFER_SHARE_SPREAD: f32 = 0.3;
    pub const MIN_OFFER: f32 = 2.0;
    pub const PRICE_MIN: f32 = 0.7;
    pub const PRICE_SPREAD: f32 = 0.6;
}

pub mod events {
    pub const DEFAULT_DAYS_PER_SEASON: u32 = crate::calendar::SEASON_DAYS as u32;
    pub const CHOICE_EXPIRY_DAYS: f64 = 5.0;
    pub const DROUGHT_INFLOW: f32 = 0.4;

    pub const FLOOD_CHANCE: f32 = 0.02;
    pub const FLOOD_SPRING_MULT: f32 = 2.0;
    pub const FLOOD_SUMMER_MULT: f32 = 0.7;
    pub const FLOOD_WATER_BASE: f32 = 0.5;
    pub const FLOOD_MIN_WATER_LEVEL: f32 = 0.2;
    pub const DROUGHT_CHANCE: f32 = 0.012;
    pub const DROUGHT_WATER_BASE: f32 = 1.5;
    pub const HARD_FREEZE_CHANCE: f32 = 0.03;
    pub const FIRE_CHANCE: f32 = 0.012;
    pub const FIRE_DROUGHT_MULT: f32 = 3.0;
    pub const FIRE_MIN_FOREST: f32 = 0.2;
    pub const DISEASE_CHANCE: f32 = 0.01;
    pub const DISEASE_MOSQUITO_BASE: f32 = 0.5;
    pub const DISEASE_MOSQUITO_SCALE: f32 = 2.0;
    pub const DISEASE_CROWD_POPULATION: f32 = 10.0;
    pub const DISEASE_CROWD_MAX: f32 = 2.0;
    pub const DISEASE_MIN_POPULATION: u32 = 6;
    pub const PREDATOR_RAID_CHANCE: f32 = 0.04;
    pub const WANDERING_BEAVER_CHANCE: f32 = 0.03;
    pub const WETLAND_ARRIVAL_CHANCE: f32 = 0.03;
    pub const WETLAND_ARRIVAL_MIN_WETLAND: f32 = 0.1;

    pub const FLOOD_WOOD_LOSS: (f32, f32) = (2.0, 6.0);
    pub const FLOOD_FOOD_LOSS: (f32, f32) = (1.0, 4.0);
    pub const DROUGHT_SEASONS: (f32, f32) = (2.0, 4.0);
    pub const HARD_FREEZE_FOOD_LOSS: (f32, f32) = (2.0, 5.0);
    pub const HARD_FREEZE_ILLNESS: f32 = 0.1;
    pub const FIRE_WOOD_LOSS: (f32, f32) = (4.0, 10.0);
    pub const FIRE_FIGHT_LOSS_SHARE: f32 = 0.3;
    pub const FIRE_FIGHT_ILLNESS: f32 = 0.05;
    pub const FIRE_BURN_FOOD_SHARE: f32 = 0.3;
    pub const DISEASE_ILLNESS: (f32, f32) = (0.1, 0.3);
    pub const DISEASE_DEATH_SHARE: (f32, f32) = (0.0, 0.08);
    pub const RAID_DEATH_SHARE: (f32, f32) = (0.03, 0.12);
    pub const RAID_MIN_DEATHS: u32 = 1;
    pub const RAID_FIGHT_DEATH_DIVISOR: u32 = 2;
    pub const RAID_FIGHT_ILLNESS: f32 = 0.05;
    pub const RAID_HIDE_FOOD_LOSS: (f32, f32) = (2.0, 6.0);
    pub const WANDERERS_MIN: u32 = 1;
    pub const WANDERERS_SPREAD: f32 = 2.0;
    pub const WANDERER_FOOD_COST: f32 = 2.0;
    pub const WETLAND_ARRIVAL_FOOD: (f32, f32) = (3.0, 8.0);
}
