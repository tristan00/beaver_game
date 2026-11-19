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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimState;

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn default_start_fires_only_arrivals_over_eight_seasons() {
        for seed in 1..=3 {
            let mut s = SimState::new(seed);
            assert_eq!(s.features, Features::scoped());
            let c = s.world.valleys[s.world.start.0 as usize].center;
            let land = crate::WorldLand { world: &s.world, depth: &s.water.depth };
            s.colony.found((c.0 as f32, c.1 as f32), &land).expect("found");
            let days = 8.0 * crate::events::DEFAULT_DAYS_PER_SEASON as f32;
            let mut t = 0.0;
            while t < days {
                s.step(0.25).unwrap();
                t += 0.25;
            }
            for r in s.events.log() {
                assert!(matches!(r.kind, EventKind::WanderingBeaver | EventKind::WetlandArrival), "{:?} fired", r.kind);
            }
            for k in [EventKind::Flood, EventKind::Drought, EventKind::HardFreeze, EventKind::Fire, EventKind::Disease, EventKind::PredatorRaid] {
                assert!(!Features::scoped().allows_event(k), "{k:?} not switched off");
            }
        }
    }
}
