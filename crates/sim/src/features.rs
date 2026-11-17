//! The single switch for out-of-scope systems (owner: `switch`).
//! `Features::scoped()` turns every system in `System::OUT_OF_SCOPE` off in every start. The code
//! stays compiled; the sim must not fire them and view-models must not list them.

use crate::buildings::BuildingKind;
use crate::colony::Job;
use crate::events::EventKind;

/// A system that can be switched off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum System {
    Predators,
    Floods,
    Droughts,
    Guard,
    Rivals,
    Trade,
    Scouting,
}

impl System {
    pub const OUT_OF_SCOPE: [System; 7] = [
        System::Predators,
        System::Floods,
        System::Droughts,
        System::Guard,
        System::Rivals,
        System::Trade,
        System::Scouting,
    ];
}

impl System {
    /// The system an event belongs to, if it is switchable.
    pub fn of_event(kind: EventKind) -> Option<System> {
        match kind {
            EventKind::Flood => Some(System::Floods),
            EventKind::Drought => Some(System::Droughts),
            EventKind::PredatorRaid => Some(System::Predators),
            _ => None,
        }
    }
    /// The system a job belongs to, if it is switchable.
    pub fn of_job(job: Job) -> Option<System> {
        match job {
            Job::Guard => Some(System::Guard),
            Job::Scout => Some(System::Scouting),
            _ => None,
        }
    }
    /// The system a building belongs to, if it is switchable.
    pub fn of_building(kind: BuildingKind) -> Option<System> {
        match kind {
            BuildingKind::GuardPost => Some(System::Guard),
            BuildingKind::ScentMound => Some(System::Predators),
            _ => None,
        }
    }
}

/// Feature configuration carried by `SimState::features`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// The one switch: when true every `System::OUT_OF_SCOPE` system is disabled.
    pub disable_out_of_scope: bool,
}

impl Features {
    /// Shipping configuration: out-of-scope systems off.
    pub const fn scoped() -> Self {
        Self { disable_out_of_scope: true }
    }
    /// Everything on (legacy behaviour, used by old tests).
    pub const fn everything() -> Self {
        Self { disable_out_of_scope: false }
    }
    /// Whether `system` may fire / be shown.
    pub fn enabled(&self, system: System) -> bool {
        !(self.disable_out_of_scope && System::OUT_OF_SCOPE.contains(&system))
    }
    /// `true` when `system` is `None` (not switchable) or enabled.
    pub fn allows(&self, system: Option<System>) -> bool {
        system.is_none_or(|s| self.enabled(s))
    }
    pub fn allows_event(&self, kind: EventKind) -> bool {
        self.allows(System::of_event(kind))
    }
    pub fn allows_job(&self, job: Job) -> bool {
        self.allows(System::of_job(job))
    }
    pub fn allows_building(&self, kind: BuildingKind) -> bool {
        self.allows(System::of_building(kind))
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::scoped()
    }
}
