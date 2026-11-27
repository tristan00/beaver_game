pub mod colony {
    include!(concat!(env!("OUT_DIR"), "/colony_constants.rs"));
}

pub mod ecology {
    include!(concat!(env!("OUT_DIR"), "/ecology_constants.rs"));
}

pub mod water {
    pub const SEASON_DAYS: f64 = crate::calendar::SEASON_DAYS;
    pub const WET_EPS: f32 = 0.01;
    pub const TREE_VEGETATION: f32 = 0.5;
    pub const MAX_ICE: f32 = 0.8;
    pub const ICE_COVER_M: f32 = 0.03;
    pub const ICE_WALK_M: f32 = 0.04;
    pub const ICE_STEFAN: f32 = 0.000_37;
    pub const ICE_SEED_M: f32 = 0.02;
    pub const ICE_MELT_PER_DEGREE_DAY: f32 = 0.009;
    pub const ICE_OPEN_SPEED: f32 = 0.35;
    pub const ICE_FLOW_MELT: f32 = 0.05;
    pub const RAIN: f32 = 0.002;
    pub const EVAPORATION: f32 = 0.002;
    pub const SEEPAGE: f32 = 0.002;
    pub const DAM_DECAY_PER_DAY: f32 = 0.001;
    pub const OVERTOP_EROSION: f32 = 0.5;
    pub const OVERTOP_DEPTH: f32 = 0.15;
    pub const ICE_GROWTH_PER_DAY: f32 = 0.1;
    pub const ICE_MELT_PER_DAY: f32 = 0.15;
    pub const RUNOFF_M3S_PER_KM2: f32 = 0.01;
    pub const WATER_PER_CATCHMENT_CELL: f32 = RUNOFF_M3S_PER_KM2 * 1.0e-6 * 86_400.0;
    pub const MANNING_CHANNEL: f32 = 0.04;
    pub const WEIR: f32 = 1.7;
}

pub mod buildings {
    include!(concat!(env!("OUT_DIR"), "/buildings_constants.rs"));
}
