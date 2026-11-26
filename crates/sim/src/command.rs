//! Player commands and their validation/dispatch boundary.
use crate::{SimError, SimState, agents, buildings, canals, colony, dams, gates, invalid, jobs, maintenance, sandbox, society, starts, water, work};

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
    SetDamUpkeep { dam: buildings::BuildingId, on: bool },
    RepairBuilding { building: buildings::BuildingId },
    ReinforceBuilding { building: buildings::BuildingId },
    /// Face a dam or building with pebbles so it wears and erodes more slowly.
    PebbleBuilding { building: buildings::BuildingId },
    /// Switch one beaver slot on or off.
    SetSlotOff { building: buildings::BuildingId, slot: usize, off: bool },
    Settle { cell: water::Cell },
    BreachDam { x: usize, y: usize },
    DigCanal { cells: Vec<water::Cell>, depth: f32 },
    /// Draw a new canal from water along these cells; diggers then dig every cell.
    PlaceCanal { cells: Vec<water::Cell> },
    /// Grow a canal; `cells` narrows Deepen and Widen to those cells, or is the route Extend adds.
    GrowCanal { canal: buildings::BuildingId, growth: canals::CanalGrowth, cells: Vec<water::Cell> },
    SetCanalUpkeep { canal: buildings::BuildingId, on: bool },
    /// Make a dam cell a gate.
    AddDamGate { dam: buildings::BuildingId, cell: water::Cell },
    /// Ask for a gate setting on a dam or canal; a builder makes the change.
    SetGate { building: buildings::BuildingId, cell: water::Cell, setting: gates::GateSetting },
    /// Upgrade a built gate into a lock (needs a finished Raft landing); a builder visits to build it.
    UpgradeLock { building: buildings::BuildingId, cell: water::Cell },
    /// Take a gate (and any lock on it) off a dam or canal; what was brought for it goes back.
    RemoveGate { building: buildings::BuildingId, cell: water::Cell },
    /// Give these beavers an order now, dropping what they were doing and any queued orders.
    Assign { who: Vec<colony::BeaverId>, to: Option<agents::Assignment> },
    /// Add an order to the end of each beaver's queue; one with no order starts it now.
    Queue { who: Vec<colony::BeaverId>, order: agents::Order },
    PlaceSite { kind: buildings::BuildingKind, pos: (usize, usize) },
    FillSlot { building: buildings::BuildingId, slot: usize, who: colony::BeaverId },
    EmptySlot { building: buildings::BuildingId, slot: usize },
    AddBeaver { building: buildings::BuildingId },
    RemoveBeaver { building: buildings::BuildingId },
    SetPriority { building: buildings::BuildingId, priority: work::Priority },
    SetPaused { building: buildings::BuildingId, paused: bool },
    AdjustRole { role: jobs::Job, delta: i32 },
    SetWorkArea { building: buildings::BuildingId, area: work::Area },
    SetHaul { order: work::HaulOrder },
    SetBalance { building: buildings::BuildingId, on: bool },
    /// Have haulers fill this store to the brim from the others (or stop).
    FillStore { store: buildings::BuildingId, on: bool },
    AnswerJoin { id: society::RequestId, answer: society::Answer },
    CancelSite { building: buildings::BuildingId },
    /// Stop a repair, reinforcing or pebble facing, leaving the building itself standing.
    CancelMaintenance { building: buildings::BuildingId },
    /// Keep a role's beavers within `cells` of water (None: no limit).
    SetShoreLimit { role: jobs::Job, cells: Option<u16> },
}

impl SimState {
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        // Preserve synchronous input feedback on a presentation snapshot. Only
        // the beaver advances time; successful local commands are replayed there
        // in order and acknowledged before their predictions are retired.
        let predicted = self.client_commands.is_some().then(|| cmd.clone());
        let events = predicted.as_ref().map(|_| self.colony.events.clone());

        let diagnostic = crate::trace::enabled().then(|| format!("{cmd:?}"));
        let logged = self.recorder.is_some().then(|| cmd.clone());
        let res = self.apply_inner(cmd);
        // Beavers and families a command brought in (the founders, a newcomer) get their names.
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
        match cmd {
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
            Command::SetDamUpkeep { dam, on } => dams::set_upkeep(self, dam, on),
            Command::RepairBuilding { building } => maintenance::order(self, building, maintenance::Refit::Repair),
            Command::ReinforceBuilding { building } => maintenance::order(self, building, maintenance::Refit::Reinforce),
            Command::PebbleBuilding { building } => maintenance::order(self, building, maintenance::Refit::Facing),
            Command::SetSlotOff { building, slot, off } => jobs::set_slot_off(self, building, slot, off),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::PlaceCanal { cells } => canals::place(self, &cells).map(|_| ()),
            Command::GrowCanal { canal, growth, cells } => canals::grow(self, canal, growth, &cells),
            Command::SetCanalUpkeep { canal, on } => canals::set_upkeep(self, canal, on),
            Command::AddDamGate { dam, cell } => dams::add_gate(self, dam, cell),
            Command::SetGate { building, cell, setting } => gates::set_gate(self, building, cell, setting),
            Command::UpgradeLock { building, cell } => crate::locks::order(self, building, cell),
            Command::RemoveGate { building, cell } => crate::gates::remove(self, building, cell),
            Command::Assign { who, to } => {
                if let Some(kit) = who.iter().find(|id| to.is_some() && !matches!(to, Some(agents::Assignment::Go(_))) && self.colony.beaver(**id).is_some_and(|b| !b.can_work())) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is too young to work", kit.0)));
                }
                if let Some(id) = who.iter().find(|id| !self.agents.by_id.contains_key(id)) {
                    return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
                }
                let mut orders = Vec::new();
                let mut destinations = std::collections::BTreeSet::new();
                for id in who {
                    let assignment = if let Some(agents::Assignment::Go(target)) = to {
                        if self.nav.surface_at(target).is_none() {
                            return Err(SimError::InvalidCommand("Move destination is outside the map".into()));
                        }
                        let from = self.agents.get(id).unwrap().cell;
                        let radius = (destinations.len() as usize + 1).min(8);
                        let Some(cell) = agents::go_cell(self, from, target, radius, &destinations) else {
                            return Err(SimError::InvalidCommand(format!("No reachable destination for beaver {}", id.0)));
                        };
                        destinations.insert(cell);
                        Some(agents::Assignment::Go(cell))
                    } else { to };
                    orders.push((id, assignment));
                }
                for (id, assignment) in orders {
                    jobs::order(self, id, assignment);
                    self.agents.assign(id, assignment);
                    if let Some(ag) = self.agents.by_id.get_mut(&id) {
                        ag.queue.clear();
                    }
                }
                Ok(())
            }
            Command::Queue { who, order } => {
                if let Some(kit) = who.iter().find(|id| !matches!(order, agents::Order::Do(agents::Assignment::Go(_))) && self.colony.beaver(**id).is_some_and(|b| !b.can_work())) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is too young to work", kit.0)));
                }
                if let Some(id) = who.iter().find(|id| !self.agents.by_id.contains_key(id)) {
                    return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
                }
                if let agents::Order::Do(agents::Assignment::Go(target)) = order {
                    if self.nav.surface_at(target).is_none() {
                        return Err(SimError::InvalidCommand("Move destination is outside the map".into()));
                    }
                }
                let mut refused = None;
                for id in who {
                    let mut ag = self.agents.by_id.remove(&id).expect("checked above");
                    if ag.assignment.is_some() || !ag.queue.is_empty() {
                        ag.queue.push_back(order);
                    } else if !agents::start_order(self, &mut ag, order) {
                        refused = Some(id);
                    }
                    self.agents.by_id.insert(id, ag);
                }
                match refused {
                    Some(id) => Err(SimError::InvalidCommand(format!("beaver {} cannot carry out {order:?}", id.0))),
                    None => Ok(()),
                }
            }
            Command::PlaceSite { kind, pos } => self.place_site(kind, pos).map(|_| ()),
            Command::FillSlot { building, slot, who } => {
                let able = self.colony.beaver(who).map(|b| b.can_work()).ok_or(work::WorkError::NoSuchBeaver);
                able
                    .and_then(|ok| if ok { Ok(()) } else { Err(work::WorkError::NotAdult) })
                    .and_then(|()| self.work.place(building).ok_or(work::WorkError::NoSuchWorkplace))
                    .and_then(|p| match p.slots.get(slot) {
                        None if slot == p.slots.len() && p.open_ended() => Ok(()),
                        None => Err(work::WorkError::SlotOutOfRange),
                        Some(s) if s.beaver.is_some() => Err(work::WorkError::SlotTaken),
                        Some(_) => Ok(()),
                    })
                    .map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                jobs::place_by_hand(self, building, slot, who)?;
                if let Some(ag) = self.agents.by_id.get_mut(&who) {
                    ag.queue.clear();
                }
                Ok(())
            }
            Command::EmptySlot { building, slot } => {
                self.work.unassign(building, slot).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                if let Ok(p) = self.work.place_mut(building) {
                    p.max_beavers = p.max_beavers.min(p.filled());
                }
                Ok(())
            }
            Command::AddBeaver { building } => jobs::add_beaver(self, building),
            Command::RemoveBeaver { building } => jobs::remove_beaver(self, building),
            Command::SetPriority { building, priority } => jobs::set_priority(self, building, priority),
            Command::SetPaused { building, paused } => jobs::set_paused(self, building, paused),
            Command::AdjustRole { role, delta } => jobs::adjust_role(self, role, delta),
            Command::SetShoreLimit { role, cells } => {
                match cells {
                    Some(n) => self.work.shore_limits.insert(role, n),
                    None => self.work.shore_limits.remove(&role),
                };
                Ok(())
            }
            Command::SetWorkArea { building, area } => {
                self.work.set_area(building, area).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::CancelSite { building } => self.cancel_site(building).map(|_| ()),
            Command::CancelMaintenance { building } => crate::maintenance::cancel(self, building).map(|_| ()),
            Command::SetBalance { building, on } => {
                let haul_mound = self.buildings.building(building).is_some_and(|b| b.is_complete() && b.kind.balances());
                if !haul_mound {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished haul mound", building.0)));
                }
                if on {
                    self.work.balanced.insert(building);
                } else {
                    self.work.balanced.remove(&building);
                }
                Ok(())
            }
            Command::FillStore { store, on } => {
                let ok = self.buildings.building(store).is_some_and(|b| b.is_complete() && agents::is_store(b.kind));
                if !ok {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished store", store.0)));
                }
                if on {
                    self.work.fills.insert(store);
                } else {
                    self.work.fills.remove(&store);
                }
                Ok(())
            }
            Command::SetHaul { order } => {
                let store = |id| {
                    self.buildings.building(id).is_some_and(|b| b.is_complete() && agents::accepts(b.kind, order.good))
                };
                if order.from == order.to || !store(order.from) || !store(order.to) {
                    return Err(SimError::InvalidCommand(format!("haul needs two distinct complete stores accepting the good: {order:?}")));
                }
                self.work.hauls.push(order);
                Ok(())
            }
            Command::AnswerJoin { id, answer } => {
                self.society.answer(id, answer, &mut self.colony).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
        }
    }
}
