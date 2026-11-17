//! Every balance parameter of the simulation, grouped by system.
//! Each module re-exports (or imports) its group; structural constants such as grid sizes
//! and solver numerics stay in their modules.

pub mod colony {
    pub const KIT_DAYS: f32 = 365.0;
    pub const ADULT_DAYS: f32 = 730.0;
    pub const LODGE_CAPACITY: usize = 12;
    pub const LODGE_WOOD_COST: f32 = 8.0;
    pub const MIN_LODGE_SPACING: f32 = 5.0;
    pub const MAX_WATER_SCAN: f32 = 10.0;
    pub const WORK_FRACTION: f32 = 0.6;
    pub const CACHE_LIFETIME_DAYS: f64 = 30.0;
    /// Days of continuously bad mood after which an adult or yearling leaves the colony.
    pub const DEPARTURE_BAD_MOOD_DAYS: f32 = 30.0;
    pub const SPRING_START: f64 = 60.0;
    pub const SPRING_END: f64 = 150.0;
    pub const BIRTH_FROM_DAY: f64 = 75.0;
}

pub mod ecology {
    /// Days per season; wetland formation is counted in seasons.
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
}

pub mod water {
    /// Days in one season; a year is four seasons: spring, summer, autumn, winter.
    pub const SEASON_DAYS: f64 = 30.0;
    /// Depth below which a cell counts as dry (for ice formation).
    pub const WET_EPS: f32 = 0.01;
    /// Vegetation density above which a cell counts as holding trees.
    pub const TREE_VEGETATION: f32 = 0.5;
    /// Maximum ice thickness.
    pub const MAX_ICE: f32 = 1.0;
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
    pub const FOOD_CACHE_CAPACITY: f32 = 100.0;
    /// Barge cargo limit and speed (cells/day).
    pub const BARGE_CAPACITY: f32 = 20.0;
    pub const BARGE_SPEED: f32 = 4.0;
    /// Workshop batches per day at base speed.
    pub const CRAFT_BATCHES_PER_DAY: f32 = 2.0;
}

pub mod region {
    /// Friendliness below this refuses contact.
    pub const WARY_BELOW: f32 = 0.3;
    pub const MAX_POPULATION: f32 = 40.0;
    pub const MAX_STORE: f32 = 500.0;
    pub const OFFER_LIFETIME_DAYS: f64 = 15.0;
    pub const MAX_OPEN_OFFERS: usize = 3;
}

pub mod events {
    /// Default calendar length of a season, in days.
    pub const DEFAULT_DAYS_PER_SEASON: u32 = 10;
    /// Days an unresolved choice waits before its default (first) option is applied.
    pub const CHOICE_EXPIRY_DAYS: f64 = 5.0;
    /// Water inflow multiplier while a drought lasts.
    pub const DROUGHT_INFLOW: f32 = 0.4;
}
pub mod tech {
    /// Branch experience needed for a tier-1 tech.
    pub const TIER1_XP: f32 = 3.0;
    /// Branch experience needed for a tier-2 tech.
    pub const TIER2_XP: f32 = 15.0;
    /// Forest Lore: storage capacity multiplier.
    pub const STORAGE_BONUS: f32 = 1.25;
    /// Master Builders: build work multiplier.
    pub const BUILD_WORK_BONUS: f32 = 0.75;
    /// Fine Tools: workshop crafting speed multiplier.
    pub const CRAFT_SPEED_BONUS: f32 = 1.5;
}
