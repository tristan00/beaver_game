//! The single switch for out-of-scope systems (owner: `switch`).
//! `Features::scoped()` turns every system in `System::OUT_OF_SCOPE` off in every start. The code
//! stays compiled; the sim must not fire them and view-models must not list them.

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
    pub fn enabled(&self, _system: System) -> bool {
        !self.disable_out_of_scope
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::scoped()
    }
}
