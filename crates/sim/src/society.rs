//! Beaver society layered on `colony::Colony` (owner: `society`): mood terms, join requests,
//! work-speed from mood. Births, ageing and skills stay in `colony.rs`.

use crate::colony::{BeaverId, Colony};
use crate::tuning::colony::{JOIN_ADULT_SHARE, JOIN_MAX_PENDING, JOIN_REQUEST_CHANCE_PER_DAY};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId(pub u64);

/// A wandering beaver asking to join; needs the player's answer.
#[derive(Clone, Debug, PartialEq)]
pub struct JoinRequest {
    pub id: RequestId,
    pub name: String,
    pub adult: bool,
    /// Map cell where the stranger waits (events and alerts point here).
    pub at: crate::water::Cell,
    pub arrived_day: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Accept,
    TurnAway,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocietyError {
    NoSuchRequest,
    /// The colony could not take the beaver in (the message says why); the request stays open.
    Colony(String),
}

/// One term of a mood or work-speed breakdown, shown on hover.
#[derive(Clone, Debug, PartialEq)]
pub struct Factor {
    pub label: &'static str,
    pub value: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Society {
    pub requests: Vec<JoinRequest>,
    next_id: u64,
    rng: u64,
}

impl Society {
    pub fn pending(&self) -> &[JoinRequest] {
        &self.requests
    }

    /// Answers a request. Accept adds the beaver to a family via `Colony::admit_wanderer` (on failure the
    /// request stays open and the error is returned); turn away removes the request and adds nobody.
    pub fn answer(&mut self, id: RequestId, a: Answer, colony: &mut Colony) -> Result<(), SocietyError> {
        let i = self.requests.iter().position(|r| r.id == id).ok_or(SocietyError::NoSuchRequest)?;
        if a == Answer::Accept {
            colony.admit_wanderer(self.requests[i].adult).map_err(|e| SocietyError::Colony(e.to_string()))?;
        }
        self.requests.remove(i);
        Ok(())
    }

    /// Raises a request now (used by `step`; also lets callers stage one). Errors if the colony has no lodge to
    /// point at.
    pub fn raise(&mut self, colony: &Colony, adult: bool, sim_time: f64) -> Result<RequestId, SocietyError> {
        let at = colony.lodges.first().map(|l| (l.pos.0.round() as usize, l.pos.1.round() as usize));
        let at = at.ok_or_else(|| SocietyError::Colony("colony has no lodge".into()))?;
        let id = RequestId(self.next_id);
        self.next_id += 1;
        let name = format!("Wanderer {}", id.0);
        self.requests.push(JoinRequest { id, name, adult, at, arrived_day: sim_time });
        Ok(id)
    }

    /// Wanderers arrive now and then while a colony is founded and fewer than `JOIN_MAX_PENDING` wait.
    pub fn step(&mut self, dt_days: f32, sim_time: f64, colony: &Colony) {
        if !colony.is_founded() || colony.is_collapsed() || self.requests.len() >= JOIN_MAX_PENDING {
            return;
        }
        if self.unit() < JOIN_REQUEST_CHANCE_PER_DAY * dt_days {
            let adult = self.unit() < JOIN_ADULT_SHARE;
            let _ = self.raise(colony, adult, sim_time);
        }
    }

    /// splitmix64, uniform in `0..1`; the stream depends only on how often it was asked.
    fn unit(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Mood split into terms whose sum is `Beaver::mood`. (`society` task.)
pub fn mood_factors(_colony: &crate::colony::Colony, _who: BeaverId) -> Vec<Factor> {
    Vec::new()
}

/// Work-speed multiplier from mood (below 1 when mood is low).
pub fn work_speed(mood: f32) -> f32 {
    0.5 + 0.5 * mood.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::RiverLand;

    fn founded() -> Colony {
        let mut c = Colony::new(1);
        c.found((0.0, 5.0), &RiverLand::default()).unwrap();
        c
    }

    #[test]
    fn join_request_accept_adds_beaver_and_turn_away_does_not() {
        let mut c = founded();
        let mut s = Society::default();
        let (a, b) = (s.raise(&c, true, 1.0).unwrap(), s.raise(&c, false, 1.0).unwrap());
        let n = c.population();
        s.answer(b, Answer::TurnAway, &mut c).unwrap();
        assert_eq!((c.population(), s.pending().len()), (n, 1));
        s.answer(a, Answer::Accept, &mut c).unwrap();
        assert_eq!((c.population(), s.pending().len()), (n + 1, 0));
        let newcomer = c.beavers.last().unwrap();
        assert!(c.family(newcomer.family).is_some() && newcomer.lodge.is_some());
        assert_eq!(s.answer(a, Answer::Accept, &mut c), Err(SocietyError::NoSuchRequest));
        assert_eq!(c.population(), n + 1);
    }

    #[test]
    fn requests_arrive_during_steps_and_are_capped() {
        let c = founded();
        let mut s = Society::default();
        for d in 0..2000 {
            s.step(1.0, d as f64, &c);
        }
        assert_eq!(s.pending().len(), JOIN_MAX_PENDING);
        let mut unfounded = Society::default();
        for d in 0..2000 {
            unfounded.step(1.0, d as f64, &Colony::new(1));
        }
        assert!(unfounded.pending().is_empty());
    }
}
