use crate::buildings::BuildingKind;
use crate::colony::Job;
use crate::events::EventKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum System {
    Predators,
    Floods,
    Droughts,
    Guard,
    Rivals,
    Trade,
    Scouting,
    HardFreeze,
    Fire,
    Disease,
}

impl System {
    pub const OUT_OF_SCOPE: [System; 10] = [
        System::Predators,
        System::Floods,
        System::Droughts,
        System::Guard,
        System::Rivals,
        System::Trade,
        System::Scouting,
        System::HardFreeze,
        System::Fire,
        System::Disease,
    ];
}

impl System {
    pub fn of_event(kind: EventKind) -> Option<System> {
        match kind {
            EventKind::Flood => Some(System::Floods),
            EventKind::Drought => Some(System::Droughts),
            EventKind::PredatorRaid => Some(System::Predators),
            EventKind::HardFreeze => Some(System::HardFreeze),
            EventKind::Fire => Some(System::Fire),
            EventKind::Disease => Some(System::Disease),
            EventKind::WanderingBeaver | EventKind::WetlandArrival => None,
        }
    }
    pub fn of_job(job: Job) -> Option<System> {
        match job {
            Job::Guard => Some(System::Guard),
            Job::Scout => Some(System::Scouting),
            _ => None,
        }
    }
    pub fn of_building(kind: BuildingKind) -> Option<System> {
        match kind {
            BuildingKind::GuardPost => Some(System::Guard),
            BuildingKind::ScentMound => Some(System::Predators),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    pub disable_out_of_scope: bool,
}

impl Features {
    pub const fn scoped() -> Self {
        Self { disable_out_of_scope: true }
    }
    pub const fn everything() -> Self {
        Self { disable_out_of_scope: false }
    }
    pub fn enabled(&self, system: System) -> bool {
        !(self.disable_out_of_scope && System::OUT_OF_SCOPE.contains(&system))
    }
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

