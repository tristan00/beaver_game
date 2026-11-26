//! Player commands and their validation/dispatch boundary.
use crate::{SimError, SimState, agents, buildings, canals, colony, dams, gates, invalid, jobs, maintenance, sandbox, society, starts, water};

/// How a dam grows when the player asks for more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DamGrowth {
    Raise,
    Widen,
    Extend,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    Regenerate { seed: u64 },
    BuildDam { cells: Vec<water::Cell>, height: f32 },
    /// Draw a new dam along these cells; beavers then build every cell.
    PlaceDam { cells: Vec<water::Cell> },
    /// Grow a dam; `cells` narrows a raise to those cells.
    GrowDam { dam: buildings::BuildingId, growth: DamGrowth, cells: Vec<water::Cell> },
    /// Toggle mender upkeep on a dam or canal.
    SetUpkeep { building: buildings::BuildingId, on: bool },
    /// Order a repair, mud reinforcement, or pebble facing (`Some`), or cancel one (`None`).
    Refit { building: buildings::BuildingId, refit: Option<maintenance::Refit> },
    /// Switch one beaver slot on or off.
    SetSlotOff { building: buildings::BuildingId, slot: usize, off: bool },
    Settle { cell: water::Cell },
    BreachDam { x: usize, y: usize },
    DigCanal { cells: Vec<water::Cell>, depth: f32 },
    /// Draw a new canal from water along these cells; diggers then dig every cell.
    PlaceCanal { cells: Vec<water::Cell> },
    /// Grow a canal; `cells` narrows Deepen and Widen to those cells, or is the route Extend adds.
    GrowCanal { canal: buildings::BuildingId, growth: canals::CanalGrowth, cells: Vec<water::Cell> },
    /// Make a dam cell a gate.
    AddDamGate { dam: buildings::BuildingId, cell: water::Cell },
    /// Ask for a gate setting on a dam or canal; a builder makes the change.
    SetGate { building: buildings::BuildingId, cell: water::Cell, setting: gates::GateSetting },
    /// Upgrade a built gate into a lock (needs a finished Raft landing); a builder visits to build it.
    UpgradeLock { building: buildings::BuildingId, cell: water::Cell },
    /// Take a gate (and any lock on it) off a dam or canal; what was brought for it goes back.
    RemoveGate { building: buildings::BuildingId, cell: water::Cell },
    /// Give these beavers an order now (`queue: false`, clearing any queued orders) or append it
    /// to their queue (`queue: true`, starting immediately if free).
    Assign { who: Vec<colony::BeaverId>, to: Option<agents::Assignment>, #[serde(default)] queue: bool },
    PlaceSite { kind: buildings::BuildingKind, pos: (usize, usize) },
    /// Add (`delta > 0`) or remove (`delta < 0`) seated workers on a workplace.
    StaffBuilding { building: buildings::BuildingId, delta: i32 },
    SetPaused { building: buildings::BuildingId, paused: bool },
    AdjustRole { role: jobs::Job, delta: i32 },
    SetBalance { building: buildings::BuildingId, on: bool },
    /// Have haulers fill this store to the brim from the others (or stop).
    FillStore { store: buildings::BuildingId, on: bool },
    AnswerJoin { id: society::RequestId, answer: society::Answer },
    CancelSite { building: buildings::BuildingId },
    /// Keep a role's beavers within `cells` of water (None: no limit).
    SetShoreLimit { role: jobs::Job, cells: Option<u16> },
}

impl SimState {
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        let predicted = self.client_commands.is_some().then(|| cmd.clone());
        let events = predicted.as_ref().map(|_| self.colony.events.clone());

        let diagnostic = crate::trace::enabled().then(|| format!("{cmd:?}"));
        let logged = self.recorder.is_some().then(|| cmd.clone());
        let res = self.apply_inner(cmd);
        crate::names::step(self);
        if let Some(command) = diagnostic {
            crate::trace::event(format!("COMMAND step={} day={:.4} {command} RESULT={res:?}", self.steps, self.time_days));
        }
        if let (Ok(()), Some(c)) = (&res, logged) {
            let step = self.steps;
            if let Some(r) = self.recorder.as_mut() {
                r.push((step, c));
            }
        }
        if let Some(events) = events { self.colony.events = events; }
        if let (Ok(()), Some(command)) = (&res, predicted) {
            self.client_commands.get_or_insert_with(Vec::new).push(command);
        }
        res
    }

    fn apply_inner(&mut self, cmd: Command) -> Result<(), SimError> {
        if self.sandbox {
            return sandbox::apply(self, cmd);
        }
        let res = match cmd {
            Command::Regenerate { seed } => {
                *self = SimState::new(seed);
                Ok(())
            }
            Command::BuildDam { cells, height } => self.water.build_dam(&cells, height).map_err(invalid),
            Command::PlaceDam { cells } => dams::place(self, &cells).map(|_| ()),
            Command::GrowDam { dam, growth, cells } => {
                let plan = match growth {
                    DamGrowth::Raise => dams::plan_raise(self, dam, &cells),
                    DamGrowth::Widen => dams::plan_widen(self, dam),
                    DamGrowth::Extend if cells.is_empty() => dams::plan_extend(self, dam),
                    DamGrowth::Extend => dams::plan_extend_to(self, dam, &cells),
                };
                dams::apply_plan(self, dam, plan.map_err(SimError::InvalidCommand)?)
            }
            Command::SetUpkeep { building, on } => {
                let b = self.buildings.buildings.iter_mut().find(|b| b.id == building)
                    .ok_or_else(|| SimError::InvalidCommand("no such building".into()))?;
                if let Some(d) = b.dam.as_deref_mut() { d.upkeep = on; Ok(()) }
                else if let Some(k) = b.canal.as_deref_mut() { k.upkeep = on; Ok(()) }
                else { Err(SimError::InvalidCommand("not a dam or canal".into())) }
            }
            Command::Refit { building, refit: Some(refit) } => maintenance::order(self, building, refit),
            Command::Refit { building, refit: None } => maintenance::cancel(self, building).map(|_| ()),
            Command::SetSlotOff { building, slot, off } => jobs::set_slot_off(self, building, slot, off),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::PlaceCanal { cells } => canals::place(self, &cells).map(|_| ()),
            Command::GrowCanal { canal, growth, cells } => canals::grow(self, canal, growth, &cells),
            Command::AddDamGate { dam, cell } => dams::add_gate(self, dam, cell),
            Command::SetGate { building, cell, setting } => gates::set_gate(self, building, cell, setting),
            Command::UpgradeLock { building, cell } => crate::locks::order(self, building, cell),
            Command::RemoveGate { building, cell } => crate::gates::remove(self, building, cell),
            Command::Assign { who, to, queue } => apply_assign(self, who, to, queue),
            Command::PlaceSite { kind, pos } => self.place_site(kind, pos).map(|_| ()),
            Command::StaffBuilding { building, delta } => jobs::staff_building(self, building, delta),
            Command::SetPaused { building, paused } => jobs::set_paused(self, building, paused),
            Command::AdjustRole { role, delta } => jobs::adjust_role(self, role, delta),
            Command::SetShoreLimit { role, cells } => {
                match cells {
                    Some(n) => self.work.shore_limits.insert(role, n),
                    None => self.work.shore_limits.remove(&role),
                };
                Ok(())
            }
            Command::CancelSite { building } => self.cancel_site(building).map(|_| ()),
            Command::SetBalance { building, on } => {
                let haul_mound = self.buildings.building(building).is_some_and(|b| b.is_complete() && b.kind.balances());
                if !haul_mound {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished haul mound", building.0)));
                }
                if on { self.work.balanced.insert(building); } else { self.work.balanced.remove(&building); }
                Ok(())
            }
            Command::FillStore { store, on } => {
                let ok = self.buildings.building(store).is_some_and(|b| b.is_complete() && agents::is_store(b.kind));
                if !ok {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished store", store.0)));
                }
                if on { self.work.fills.insert(store); } else { self.work.fills.remove(&store); }
                Ok(())
            }
            Command::AnswerJoin { id, answer } => {
                self.society.answer(id, answer, &mut self.colony, &self.buildings.buildings).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
        };
        if res.is_ok() { agents::wake_idle(self); }
        res
    }
}

fn apply_assign(sim: &mut SimState, who: Vec<colony::BeaverId>, to: Option<agents::Assignment>, queue: bool) -> Result<(), SimError> {
    if let Some(kit) = who.iter().find(|id| to.is_some() && !matches!(to, Some(agents::Assignment::Go(_))) && sim.colony.beaver(**id).is_some_and(|b| !b.can_work())) {
        return Err(SimError::InvalidCommand(format!("beaver {} is too young to work", kit.0)));
    }
    if let Some(id) = who.iter().find(|id| !sim.agents.by_id.contains_key(id)) {
        return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
    }
    if let Some(agents::Assignment::Go(target)) = to && sim.nav.surface_at(target).is_none() {
        return Err(SimError::InvalidCommand("Move destination is outside the map".into()));
    }
    let Some(order) = to else {
        for id in who {
            sim.work.vacate(id);
            sim.agents.assign(id, None);
        }
        return Ok(());
    };
    let mut destinations = std::collections::BTreeSet::new();
    let mut start_now = Vec::new();
    let mut to_queue = Vec::new();
    for id in who {
        if queue && (jobs::is_busy(sim, id) || sim.agents.get(id).is_some_and(|a| !a.queue.is_empty())) {
            to_queue.push(id);
            continue;
        }
        let a = match order {
            agents::Assignment::Go(target) => {
                let from = sim.agents.get(id).unwrap().cell;
                let Some(cell) = agents::go_cell(sim, from, target, (destinations.len() + 1).min(8), &destinations) else {
                    return Err(SimError::InvalidCommand(format!("No reachable destination for beaver {}", id.0)));
                };
                destinations.insert(cell);
                agents::Assignment::Go(cell)
            }
            other => other,
        };
        start_now.push((id, a));
    }
    for &id in &to_queue {
        sim.agents.by_id.get_mut(&id).expect("checked above").queue.push_back(order);
    }
    let mut placed = to_queue.len();
    let mut last_err = None;
    for (id, a) in start_now {
        let mut ag = sim.agents.by_id.remove(&id).expect("checked above");
        let ok = agents::start_order(sim, &mut ag, a);
        if ok && !queue { ag.queue.clear(); }
        sim.agents.by_id.insert(id, ag);
        if ok {
            placed += 1;
        } else {
            last_err = Some(match a {
                agents::Assignment::Work(b) => jobs::workplace_error_reason(sim, b),
                _ => format!("beaver {} cannot carry out {a:?}", id.0),
            });
        }
    }
    if placed > 0 || last_err.is_none() { Ok(()) } else { Err(SimError::InvalidCommand(last_err.unwrap())) }
}
