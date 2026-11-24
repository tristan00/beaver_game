//! The weather: the air's temperature, rain or snow, wind and cloud at any moment, drawn from
//! the real climate of the place a map is taken from (its monthly means, see `maps::Climate`).
//! Days swing about those means the way they do there: cold snaps and thaws lasting days, wet
//! spells and dry ones, rain coming as a shower of some hours, now and then a storm.
//!
//! It is a pure function of the world's seed and the time, so it needs no saving, does not
//! depend on frame rate or solver substeps, and every system reads the same sky.
//!
//! The game year starts with spring in mid-April, when the ice goes out on these northern
//! waters; a game month is 30 days.
use crate::maps::Climate;

/// What the sky is doing, as the player is told it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Clear,
    Cloudy,
    Rain,
    Storm,
    Snow,
    Blizzard,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Clear => "Clear",
            Kind::Cloudy => "Cloudy",
            Kind::Rain => "Rain",
            Kind::Storm => "Storm",
            Kind::Snow => "Snow",
            Kind::Blizzard => "Blizzard",
        }
    }
}

/// The weather at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Now {
    /// Air temperature, °C.
    pub temp_c: f32,
    /// How cold it feels out in the open, wind chill included, °C.
    pub feels_c: f32,
    /// Rain and snow (as water) falling now, metres a day.
    pub rain: f32,
    pub snow: f32,
    /// 0 calm to 1 a gale.
    pub wind: f32,
    /// 0 clear sky to 1 overcast.
    pub cloud: f32,
    pub kind: Kind,
    /// The last few days' air, which the ground and still water follow: frozen ground below 0.
    pub ground_c: f32,
}

impl Now {
    /// Ground too hard to dig mud or clay from.
    pub fn ground_frozen(&self) -> bool {
        self.ground_c < FROZEN_GROUND_C
    }

    /// Weather to stay in the lodge through.
    pub fn bitter(&self) -> bool {
        self.feels_c < HOLE_UP_FEELS_C || self.kind == Kind::Blizzard
    }
}

/// How cold `celsius` is for a beaver, 0 (none) to 1 (as cold as it gets).
pub fn cold(celsius: f32) -> f32 {
    use crate::tuning::colony::{COLD_FROM_C, COLD_SPAN_C};
    ((COLD_FROM_C - celsius) / COLD_SPAN_C).clamp(0.0, 1.0)
}

/// Ground below this (the last days' air) is frozen hard.
pub const FROZEN_GROUND_C: f32 = -3.0;
/// Out in the open colder than this (wind chill included), beavers keep to their lodges.
pub const HOLE_UP_FEELS_C: f32 = -18.0;
/// Beavers come out again once it feels warmer than this.
pub const COME_OUT_FEELS_C: f32 = -14.0;

/// A temperate continental climate (central Ontario) for worlds not taken from a real place.
const DEFAULT: Climate = Climate {
    temp_c: [-10.8, -9.8, -4.6, 2.4, 10.7, 16.0, 18.6, 17.8, 13.9, 7.0, 0.3, -6.4],
    precip_mm: [72.0, 52.0, 63.0, 83.0, 83.0, 90.0, 85.0, 88.0, 92.0, 94.0, 87.0, 73.0],
    snow_mm: [52.0, 39.0, 35.0, 25.0, 3.0, 0.0, 0.0, 0.0, 0.0, 6.0, 35.0, 52.0],
    evap_mm: [11.0, 17.0, 37.0, 63.0, 102.0, 116.0, 123.0, 102.0, 71.0, 41.0, 20.0, 12.0],
    runoff_mm: [40.0, 29.0, 60.0, 155.0, 84.0, 39.0, 22.0, 14.0, 12.0, 29.0, 44.0, 40.0],
    ice_days: Some((338, 113)),
};

/// One world's sky: its seed and its place's climate.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    seed: u64,
    pub climate: &'static Climate,
}

impl Default for Sky {
    fn default() -> Self {
        Sky { seed: 0, climate: &DEFAULT }
    }
}

impl PartialEq for Sky {
    fn eq(&self, o: &Self) -> bool {
        self.seed == o.seed && std::ptr::eq(self.climate, o.climate)
    }
}

/// Each map's climate by its seed. A map without its own climate in its header takes the
/// nearest one that has one: the same lakes and forests, a few kilometres off.
fn climates() -> &'static [(u64, Climate)] {
    static CACHE: std::sync::OnceLock<Vec<(u64, Climate)>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let infos = crate::maps::infos();
        let with: Vec<&crate::maps::MapInfo> = infos.iter().filter(|i| i.climate.is_some()).collect();
        infos
            .iter()
            .filter_map(|i| {
                let c = i.climate.clone().or_else(|| {
                    let d = |o: &crate::maps::MapInfo| (o.lat - i.lat).powi(2) + ((o.lon - i.lon) * i.lat.to_radians().cos()).powi(2);
                    with.iter().min_by(|a, b| d(a).total_cmp(&d(b))).and_then(|o| o.climate.clone())
                })?;
                Some((i.seed(), c))
            })
            .collect()
    })
}

impl Sky {
    /// The sky over the world with this seed: its map's climate, or a temperate default.
    pub fn for_seed(seed: u64) -> Sky {
        let climate = climates().iter().find(|(s, _)| *s == seed).map_or(&DEFAULT, |(_, c)| c);
        Sky { seed, climate }
    }

    /// The weather at `time` (game days).
    pub fn at(&self, time: f64) -> Now {
        let temp_c = self.temp(time);
        let (rain_total, cloud, storm) = self.precip(time);
        let snowing = temp_c < SNOW_BELOW_C;
        let (rain, snow) = if snowing { (0.0, rain_total) } else { (rain_total, 0.0) };
        let gust = noise(self.seed ^ 0x5717, time, 2.0) * 0.5 + 0.5;
        let wind = (0.12 + 0.3 * gust + if storm { 0.5 } else { 0.0 } + 0.15 * cloud).min(1.0);
        // Wind chill: about a degree for every tenth of a gale, more the colder it is.
        let feels_c = temp_c - wind * (4.0 + (-temp_c).max(0.0) * 0.35);
        let mm = rain_total * 1000.0;
        let kind = match (snowing, mm) {
            (true, m) if m > 0.5 && (storm || wind > 0.7) => Kind::Blizzard,
            (true, m) if m > 0.5 => Kind::Snow,
            (false, m) if m > 0.5 && storm => Kind::Storm,
            (false, m) if m > 0.5 => Kind::Rain,
            _ if cloud > 0.55 => Kind::Cloudy,
            _ => Kind::Clear,
        };
        Now { temp_c, feels_c, rain, snow, wind, cloud, kind, ground_c: self.ground(time) }
    }

    /// How much more (or less) the streams bring than the year's mean: the month's runoff,
    /// raised after the last days' rain.
    pub fn inflow(&self, time: f64) -> f32 {
        let c = self.climate;
        let month = self.usual_inflow(time);
        let day = time.floor();
        let mut recent = 0.0;
        let mut expected = 0.0;
        for k in 0..6 {
            let d = day - k as f64;
            let w = RUNOFF_MEMORY.powi(k);
            if self.temp(d + 0.5) >= SNOW_BELOW_C {
                recent += w * self.wet_amount_mm(d as i64);
            }
            let p = monthly(&c.precip_mm, d + 0.5);
            let liquid = (1.0 - monthly(&c.snow_mm, d + 0.5) / p.max(1.0)).clamp(0.0, 1.0);
            expected += w * p / MONTH_DAYS as f32 * liquid;
        }
        let storm = if expected > 0.3 { (0.55 + 0.45 * recent / expected).clamp(0.35, 3.5) } else { 1.0 };
        month * storm
    }

    /// The streams as they usually run this time of year, against the year's mean.
    pub fn usual_inflow(&self, time: f64) -> f32 {
        let c = self.climate;
        monthly(&c.runoff_mm, time) / (c.runoff_mm.iter().sum::<f32>() / 12.0).max(1e-3)
    }

    /// Open water lost to the air, as a share of `tuning::water::EVAPORATION` a day: the month's
    /// evaporation, less under cloud.
    pub fn evaporation(&self, time: f64) -> f32 {
        let per_day_m = monthly(&self.climate.evap_mm, time) / MONTH_DAYS as f32 / 1000.0;
        let (_, cloud, _) = self.precip(time);
        per_day_m / crate::tuning::water::EVAPORATION * (1.2 - 0.6 * cloud)
    }

    /// The air temperature: the month's mean plus the day's swing from it.
    fn temp(&self, time: f64) -> f32 {
        let c = self.climate;
        let mean = monthly(&c.temp_c, time);
        // Spread about the mean: wider in winter, when arctic air and thaws take turns.
        let sigma = (2.6 + 0.3 * (6.0 - mean).max(0.0)).min(8.5);
        let swing = (0.75 * noise(self.seed ^ 0x7e41, time, 4.0) + 0.85 * noise(self.seed ^ 0x3c9d, time, 11.0)) / 0.62;
        mean + sigma * swing.clamp(-2.6, 2.6)
    }

    /// The last week's daily means, the latest counting most.
    fn ground(&self, time: f64) -> f32 {
        let day = time.floor();
        let (mut sum, mut weight) = (0.0, 0.0);
        let today = (time - day) as f32;
        for k in 0..8 {
            let w = GROUND_MEMORY.powi(k) * if k == 0 { today.max(0.05) } else { 1.0 };
            sum += w * self.temp(day - k as f64 + 0.5);
            weight += w;
        }
        sum / weight
    }

    /// The share of days with rain or snow this time of year.
    fn wet_share(&self, day: i64) -> f32 {
        let p = monthly(&self.climate.precip_mm, day as f64 + 0.5);
        (p / (p + 80.0)).clamp(0.12, 0.6)
    }

    /// Whether a day is wet: a chain from a week before, a wet day likelier after a wet one.
    fn wet(&self, day: i64) -> bool {
        let mut wet = unit(self.seed, day - 8, 1) < self.wet_share(day - 8);
        for d in day - 7..=day {
            let p = self.wet_share(d);
            let dry_to_wet = p * (1.0 - WET_PERSISTENCE);
            wet = unit(self.seed, d, 1) < if wet { dry_to_wet + WET_PERSISTENCE } else { dry_to_wet };
        }
        wet
    }

    /// The rain or snow (as water) a day brings, mm: none on a dry day; on a wet one, the
    /// month's total shared out over its wet days, most days light and a few heavy.
    fn wet_amount_mm(&self, day: i64) -> f32 {
        if !self.wet(day) {
            return 0.0;
        }
        let p = monthly(&self.climate.precip_mm, day as f64 + 0.5) / MONTH_DAYS as f32;
        let mean_wet = p / self.wet_share(day);
        mean_wet * (-(1.0 - unit(self.seed, day, 2) * 0.995).ln()).min(6.0)
    }

    /// Rain or snow falling now (m/day), the cloud (0 to 1) and whether it is a storm. A wet
    /// day's water falls over a spell of six to eighteen hours, which can run past midnight.
    fn precip(&self, time: f64) -> (f32, f32, bool) {
        let day = time.floor() as i64;
        let (mut rate, mut cloud, mut storm) = (0.0f32, 0.0f32, false);
        for d in [day - 1, day] {
            let amount = self.wet_amount_mm(d);
            if amount <= 0.0 {
                continue;
            }
            let start = d as f64 + unit(self.seed, d, 3) as f64 * 0.6;
            let len = 0.25 + 0.5 * unit(self.seed, d, 4) as f64;
            let x = (time - start) / len;
            // Cloud gathers a few hours before and lingers after.
            let near = 1.0 - ((x - 0.5).abs() - 0.5).max(0.0) / 0.4;
            cloud = cloud.max(near.clamp(0.0, 1.0) as f32);
            if (0.0..1.0).contains(&x) {
                let shape = (std::f64::consts::PI * x).sin().powi(2) as f32 * 2.0;
                rate += amount / len as f32 * shape / 1000.0;
                let mean_wet = monthly(&self.climate.precip_mm, d as f64 + 0.5) / MONTH_DAYS as f32 / self.wet_share(d);
                storm |= amount > STORM_TIMES_MEAN * mean_wet && amount > STORM_MIN_MM;
            }
        }
        let drift = noise(self.seed ^ 0xc10d, time, 1.5) * 0.5 + 0.5;
        (rate, cloud.max(0.6 * drift * drift), storm)
    }
}

const MONTH_DAYS: f64 = crate::calendar::SEASON_DAYS / 3.0;
/// Where in the real year the game year starts, in months from the first of January: the
/// middle of April.
pub const START_MONTH: f64 = 3.5;
/// Rain or snow: snow below this.
const SNOW_BELOW_C: f32 = 0.5;
/// How much likelier a wet day is after a wet one.
const WET_PERSISTENCE: f32 = 0.35;
/// A wet day bringing this many times the usual, and at least `STORM_MIN_MM`, is a storm.
const STORM_TIMES_MEAN: f32 = 2.2;
const STORM_MIN_MM: f32 = 12.0;
/// How quickly the streams forget rain, and the ground the air, a day.
const RUNOFF_MEMORY: f32 = 0.6;
const GROUND_MEMORY: f32 = 0.7;

/// A monthly value (January first) at a game time, eased from one month's middle to the next.
pub fn monthly(values: &[f32; 12], time: f64) -> f32 {
    let x = crate::calendar::day_of_year(time) / MONTH_DAYS + START_MONTH - 0.5;
    let a = x.floor();
    let t = (x - a) as f32;
    let t = t * t * (3.0 - 2.0 * t);
    let v = values[(a as i64).rem_euclid(12) as usize];
    v + (values[(a as i64 + 1).rem_euclid(12) as usize] - v) * t
}

fn hash(seed: u64, i: i64, salt: u64) -> u64 {
    let mut z = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A number in [0, 1) for this day and purpose.
fn unit(seed: u64, day: i64, salt: u64) -> f32 {
    (hash(seed, day, salt) >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth noise in [-1, 1], changing over about `period` days.
fn noise(seed: u64, time: f64, period: f64) -> f32 {
    let x = time / period;
    let a = x.floor();
    let t = (x - a) as f32;
    let t = t * t * (3.0 - 2.0 * t);
    let k = |i: i64| unit(seed, i, 7) * 2.0 - 1.0;
    let (v0, v1) = (k(a as i64), k(a as i64 + 1));
    v0 + (v1 - v0) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn year(sky: &Sky, from: f64) -> Vec<Now> {
        (0..360 * 24).map(|h| sky.at(from + h as f64 / 24.0)).collect()
    }

    /// Over years, each month comes out near the place's own temperature and rain, while the
    /// days vary: rain on some, dry spells, cold snaps and thaws.
    #[test]
    fn the_weather_follows_the_places_climate_and_varies() {
        for id in ["algonquin_mizzy", "agder_tarn", "voyageurs_forks", "bavarian_lusen"] {
            let info = crate::maps::infos().iter().find(|i| i.id == id).unwrap();
            let sky = Sky::for_seed(info.seed());
            let c = info.climate.as_ref().unwrap();
            assert_eq!(sky.climate, c, "{id} reads its own climate");
            let (mut temp, mut rain) = ([0.0f64; 12], [0.0f64; 12]);
            let years = 6;
            for y in 0..years {
                for (h, now) in year(&sky, y as f64 * 360.0).iter().enumerate() {
                    let day = h / 24;
                    let month = (day as f64 / 30.0 + START_MONTH).floor() as usize % 12;
                    temp[month] += now.temp_c as f64 / (30.0 * 24.0 * years as f64);
                    rain[month] += (now.rain + now.snow) as f64 * 1000.0 / 24.0 / years as f64;
                }
            }
            for m in 0..12 {
                assert!((temp[m] as f32 - c.temp_c[m]).abs() < 3.0, "{id} month {m}: {:.1} °C against {:.1}", temp[m], c.temp_c[m]);
                assert!((rain[m] as f32 - c.precip_mm[m]).abs() < c.precip_mm[m] * 0.45 + 10.0, "{id} month {m}: {:.0} mm against {:.0}", rain[m], c.precip_mm[m]);
            }
            let days: Vec<Now> = (0..360).map(|d| sky.at(d as f64 + 0.5)).collect();
            let wet = (0..360).filter(|&d| sky.wet_amount_mm(d) > 0.0).count();
            assert!((40..240).contains(&wet), "{id}: {wet} wet days a year");
            let winter: Vec<f32> = days[270..360].iter().map(|n| n.temp_c).collect();
            let (lo, hi) = winter.iter().fold((f32::MAX, f32::MIN), |(a, b), &t| (a.min(t), b.max(t)));
            assert!(hi - lo > 12.0, "{id}: winter only ranges {lo:.0}..{hi:.0} °C");
        }
    }

    #[test]
    fn maps_without_a_climate_borrow_the_nearest() {
        let infos = crate::maps::infos();
        let old = infos.iter().find(|i| i.id == "algonquin").unwrap();
        assert!(old.climate.is_none());
        let near = infos.iter().find(|i| i.id == "algonquin_mizzy").unwrap();
        assert_eq!(Sky::for_seed(old.seed()).climate, near.climate.as_ref().unwrap());
    }

    #[test]
    fn the_same_moment_has_the_same_weather() {
        let sky = Sky::for_seed(12);
        assert_eq!(sky.at(100.3), sky.at(100.3));
        assert!(sky.inflow(50.0) > 0.0 && sky.evaporation(50.0) > 0.0);
    }
}
