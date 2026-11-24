use crate::weather::Sky;

/// The weather the water runs under: the place's own (`Seasonal`, the game's), or one held
/// fixed for the river sandbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Forcing {
    /// Metres of rain falling a day (snow lies where it falls), and the evaporation multiplier.
    pub rain: f32,
    pub evaporation: f32,
    /// Catchment response, multiplied by each source's own seasonality.
    pub inflow: f32,
}

/// The rain, evaporation and streams at `time`: the place's weather (see `weather`), or a
/// sandbox's fixed weather over the place's seasons.
pub fn forcing(time: f64, weather: Weather, sky: &Sky) -> Forcing {
    let evaporation = sky.evaporation(time);
    let base = |rain: f32, runoff: f32, evap: f32| Forcing { rain, evaporation: evaporation * evap, inflow: sky.usual_inflow(time) * runoff };
    match weather {
        Weather::Seasonal => Forcing { rain: sky.at(time).rain, evaporation, inflow: sky.inflow(time) },
        Weather::Clear => base(0.0, 1.0, 1.0),
        Weather::Rain => base(0.012, 1.6, 0.5),
        Weather::Storm => base(0.060, 3.0, 0.3),
        Weather::Drought => base(0.0, 0.12, 1.5),
    }
}
