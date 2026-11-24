include!(concat!(env!("OUT_DIR"), "/society_constants.rs"));
use crate::colony::Colony;
use crate::tuning::colony::{JOIN_ADULT_SHARE, JOIN_MAX_PENDING, JOIN_REQUESTS_PER_YEAR};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct RequestId(pub u64);

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinRequest {
    pub id: RequestId,
    pub name: String,
    pub adult: bool,
    pub at: crate::water::Cell,
    pub arrived_day: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Answer {
    Accept,
    TurnAway,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SocietyError {
    NoSuchRequest,
    Colony(String),
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Society {
    pub requests: Vec<JoinRequest>,
    next_id: u64,
    rng: u64,
    /// The year the last newcomer came asking: one a year at most. Not saved; a reload only
    /// risks one more that year.
    #[serde(skip)]
    asked_year: Option<i64>,
}

impl Society {
    pub fn pending(&self) -> &[JoinRequest] {
        &self.requests
    }

    pub fn answer(&mut self, id: RequestId, a: Answer, colony: &mut Colony) -> Result<(), SocietyError> {
        let i = self.requests.iter().position(|r| r.id == id).ok_or(SocietyError::NoSuchRequest)?;
        if a == Answer::Accept {
            colony.admit_wanderer(self.requests[i].adult).map_err(|e| SocietyError::Colony(e.to_string()))?;
        }
        self.requests.remove(i);
        Ok(())
    }

    pub fn raise(&mut self, colony: &Colony, adult: bool, sim_time: f64) -> Result<RequestId, SocietyError> {
        let at = colony.lodges.first().map(|l| (l.pos.0.round() as usize, l.pos.1.round() as usize));
        let at = at.ok_or_else(|| SocietyError::Colony("colony has no lodge".into()))?;
        let id = RequestId(self.next_id);
        self.next_id += 1;
        let name = format!("Wanderer {}", id.0);
        self.requests.push(JoinRequest { id, name, adult, at, arrived_day: sim_time });
        Ok(id)
    }

    pub fn step(&mut self, dt_days: f32, sim_time: f64, colony: &Colony) {
        self.requests.retain(|r| sim_time - r.arrived_day < JOIN_EXPIRES_DAYS);
        if colony.migration_remaining() == 0 {
            self.requests.clear();
            return;
        }
        if !colony.is_founded() || colony.is_collapsed() || self.requests.len() >= JOIN_MAX_PENDING {
            return;
        }
        if colony.food_per_capita() < JOIN_MIN_FOOD_PER_HEAD {
            return;
        }
        // A single newcomer at a time, and at most one a year.
        let year = (sim_time / crate::calendar::YEAR_DAYS).floor() as i64;
        if self.asked_year == Some(year) {
            return;
        }
        if self.unit() < JOIN_REQUESTS_PER_YEAR / crate::calendar::YEAR_DAYS as f32 * dt_days {
            let adult = self.unit() < JOIN_ADULT_SHARE;
            if self.raise(colony, adult, sim_time).is_ok() {
                self.asked_year = Some(year);
            }
        }
    }

    fn unit(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn work_speed(mood: f32) -> f32 {
    use crate::tuning::colony::{WORK_MOOD_BASE, WORK_MOOD_SCALE};
    WORK_MOOD_BASE + WORK_MOOD_SCALE * mood.clamp(0.0, 1.0)
}

