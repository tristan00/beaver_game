
include!(concat!(env!("OUT_DIR"), "/calendar_constants.rs"));
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    pub fn at(time_days: f64) -> Season {
        Season::ALL[((time_days / SEASON_DAYS).floor() as i64).rem_euclid(4) as usize]
    }

    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "Spring",
            Season::Summer => "Summer",
            Season::Autumn => "Autumn",
            Season::Winter => "Winter",
        }
    }

    pub fn next(self) -> Season {
        Season::ALL[(self as usize + 1) % 4]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u32,
    pub season: Season,
    pub day: u32,
}

impl Date {
    pub fn at(time_days: f64) -> Date {
        let whole = time_days.max(0.0).floor() as u64;
        let dps = SEASON_DAYS as u64;
        let season_idx = whole / dps;
        Date { year: (season_idx / 4) as u32 + 1, season: Season::ALL[(season_idx % 4) as usize], day: (whole % dps) as u32 + 1 }
    }
}

pub fn day_of_year(time_days: f64) -> f64 {
    time_days.rem_euclid(YEAR_DAYS)
}

pub fn year_phase(time_days: f64) -> f32 {
    (day_of_year(time_days) / YEAR_DAYS) as f32
}

/// Food vegetation uses the colony's calendar. Spring greens emerge over 15
/// days; growth winds down through the last half of autumn and stops in winter.
pub fn vegetation_growth(time_days: f64) -> f32 {
    let day = day_of_year(time_days);
    let smooth = |t: f64| { let t=t.clamp(0.0,1.0); (t*t*(3.0-2.0*t)) as f32 };
    if day < SPRING_GROWTH_DAYS { smooth(day / SPRING_GROWTH_DAYS) }
    else if day < AUTUMN_GROWTH_START { 1.0 }
    else if day < WINTER_GROWTH_START { 1.0-smooth((day-AUTUMN_GROWTH_START)/(WINTER_GROWTH_START-AUTUMN_GROWTH_START)) }
    else { 0.0 }
}

pub fn days_until(time_days: f64, season: Season) -> f64 {
    let start = season as usize as f64 * SEASON_DAYS;
    (start - day_of_year(time_days)).rem_euclid(YEAR_DAYS)
}

#[cfg(test)]
mod one_calendar {
    use super::*;

    /// Every system counts the same year: four seasons of `SEASON_DAYS`. A second season
    /// length (ecology once kept its own 30-day one) or a day count left from a shorter year
    /// makes things happen three times too fast without anything looking wrong.
    #[test]
    fn every_system_reads_the_same_seasons_and_year() {
        use crate::tuning::{colony, ecology};
        assert_eq!(YEAR_DAYS, 4.0 * SEASON_DAYS);
        assert_eq!(ecology::SEASON_DAYS as f64, SEASON_DAYS, "ecology runs on its own season length");
        assert_eq!(colony::KIT_DAYS as f64, YEAR_DAYS);
        assert_eq!(crate::hud::WINTER_RESERVE_DAYS as f64, SEASON_DAYS);
        // Mating falls in winter, births (after gestation) in spring, leaves turn in autumn.
        let winter = 3.0 * SEASON_DAYS..YEAR_DAYS;
        assert!(winter.contains(&colony::MATING_FROM_DAY) && winter.contains(&(colony::MATING_TO_DAY - 1e-6)));
        let born = (colony::MATING_FROM_DAY + colony::GESTATION_DAYS).rem_euclid(YEAR_DAYS);
        assert!(Season::at(born) == Season::Spring, "births fall in {:?}", Season::at(born));
        assert_eq!(Season::at(AUTUMN_GROWTH_START), Season::Autumn);
        assert_eq!(Season::at(WINTER_GROWTH_START), Season::Winter);
    }
}
