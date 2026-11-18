//! The one calendar every system shares: a 360-day year of four 90-day seasons, starting with
//! spring on day 0. Water, ecology, events, ageing, tech and the interface all read it from here.

pub const SEASON_DAYS: f64 = 90.0;
pub const YEAR_DAYS: f64 = 4.0 * SEASON_DAYS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    /// Season at `time_days` (0 = first day of spring, year 1).
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

/// A calendar date: `year` and `day` (within the season) are 1-based.
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

/// Day within the year, `0..YEAR_DAYS`.
pub fn day_of_year(time_days: f64) -> f64 {
    time_days.rem_euclid(YEAR_DAYS)
}

/// Fraction of the year elapsed, `0..1`.
pub fn year_phase(time_days: f64) -> f32 {
    (day_of_year(time_days) / YEAR_DAYS) as f32
}

/// Days from `time_days` until the next start of `season` (0 if it starts right now).
pub fn days_until(time_days: f64, season: Season) -> f64 {
    let start = season as usize as f64 * SEASON_DAYS;
    (start - day_of_year(time_days)).rem_euclid(YEAR_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_follow_each_other_over_a_year() {
        assert_eq!(Season::at(0.0), Season::Spring);
        assert_eq!(Season::at(89.9), Season::Spring);
        assert_eq!(Season::at(90.0), Season::Summer);
        assert_eq!(Season::at(270.0), Season::Winter);
        assert_eq!(Season::at(360.0), Season::Spring);
        assert_eq!(Season::Winter.next(), Season::Spring);
    }

    #[test]
    fn dates_are_one_based() {
        assert_eq!(Date::at(0.0), Date { year: 1, season: Season::Spring, day: 1 });
        assert_eq!(Date::at(91.5), Date { year: 1, season: Season::Summer, day: 2 });
        assert_eq!(Date::at(360.0 + 270.0), Date { year: 2, season: Season::Winter, day: 1 });
    }

    #[test]
    fn days_until_wraps_to_next_year() {
        assert_eq!(days_until(100.0, Season::Autumn), 80.0);
        assert_eq!(days_until(100.0, Season::Spring), 260.0);
        assert_eq!(days_until(90.0, Season::Summer), 0.0);
        assert!((year_phase(540.0) - 0.5).abs() < 1e-6);
    }
}
