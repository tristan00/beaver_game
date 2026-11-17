//! Beaver society layered on `colony::Colony` (owner: `society`): mood terms, join requests,
//! work-speed from mood. Births, ageing and skills stay in `colony.rs`.

use crate::colony::BeaverId;

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
}

impl Society {
    pub fn pending(&self) -> &[JoinRequest] {
        &self.requests
    }
    /// Answers a request. Accept adds a beaver (via `Colony::add_immigrants`); turn away does not.
    /// (`society` wires the colony side; the stub only removes the request.)
    pub fn answer(&mut self, id: RequestId, _a: Answer) -> Result<(), SocietyError> {
        let i = self.requests.iter().position(|r| r.id == id).ok_or(SocietyError::NoSuchRequest)?;
        self.requests.remove(i);
        Ok(())
    }
    /// Raises new requests when conditions allow. (`society` task.)
    pub fn step(&mut self, _dt_days: f32, _sim_time: f64) {
        let _ = self.next_id;
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
