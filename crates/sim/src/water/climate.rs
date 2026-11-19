use super::{EVAP_MULT, INFLOW_MULT, RAIN_MULT, SEASON_DAYS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weather {
    #[default]
    Seasonal,
    Clear,
    Rain,
    Storm,
    Drought,
}

impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Self::Seasonal => "seasonal",
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Storm => "storm",
            Self::Drought => "drought",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "seasonal" => Some(Self::Seasonal),
            "clear" => Some(Self::Clear),
            "rain" => Some(Self::Rain),
            "storm" => Some(Self::Storm),
            "drought" => Some(Self::Drought),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Forcing {
    /// Metres of precipitation per day and seasonal evaporation multiplier.
    pub rain: f32,
    pub evaporation: f32,
    /// Catchment response, multiplied by each source's own seasonality.
    pub inflow: f32,
}

fn seasonal(time: f64, values: [f32; 4]) -> f32 {
    let phase = time / SEASON_DAYS - 0.5;
    let a = phase.floor() as i64;
    let t = (phase - phase.floor()) as f32;
    let t = t * t * (3.0 - 2.0 * t);
    let v = values[a.rem_euclid(4) as usize];
    v + (values[(a + 1).rem_euclid(4) as usize] - v) * t
}

// Repeatable weather is independent of frame rate and solver substep size. A rainy day
// in each six-day cycle has gentle onset/end; upstream runoff peaks three hours later.
fn rain_pulse(time: f64) -> f32 {
    let t = time.rem_euclid(6.0) - 2.0;
    if !(0.0..1.0).contains(&t) { return 0.0; }
    (std::f64::consts::PI * t).sin().powi(2) as f32
}

pub fn forcing(time: f64, weather: Weather, base_rain: f32) -> Forcing {
    let (rain, runoff) = match weather {
        Weather::Seasonal => (base_rain * 12.0 * rain_pulse(time), 1.0 + 0.8 * rain_pulse(time - 0.125)),
        Weather::Clear => (0.0, 1.0),
        Weather::Rain => (0.012, 1.6),
        Weather::Storm => (0.060, 3.0),
        Weather::Drought => (0.0, 0.12),
    };
    Forcing {
        rain: rain * seasonal(time, RAIN_MULT),
        evaporation: seasonal(time, EVAP_MULT),
        inflow: seasonal(time, INFLOW_MULT) * runoff,
    }
}
