//! Player commands and their validation/dispatch boundary.
use crate::{SimError, SimState, agents, buildings, colony, forestry, invalid, jobs, maintenance, sandbox, shipping, society, starts, water, work};


#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Regenerate { seed: u64 },
    BuildDam { cells: Vec<water::Cell>, height: f32 },
    OrderDam { cells: Vec<water::Cell>, height: f32 },
    OrderDamTo { cells: Vec<water::Cell>, crest: f32 },
    RepairDam { cell: water::Cell },
    RepairBuilding { building: buildings::BuildingId },
    ReinforceBuilding { building: buildings::BuildingId },
    Settle { cell: water::Cell },
    BreachDam { x: usize, y: usize },
    DigCanal { cells: Vec<water::Cell>, depth: f32 },
    Assign { who: Vec<colony::BeaverId>, to: Option<agents::Assignment> },
    PlaceSite { kind: buildings::BuildingKind, pos: (usize, usize) },
    FillSlot { building: buildings::BuildingId, slot: usize, who: colony::BeaverId },
    EmptySlot { building: buildings::BuildingId, slot: usize },
    AddWorker { building: buildings::BuildingId },
    RemoveWorker { building: buildings::BuildingId },
    SetPriority { building: buildings::BuildingId, priority: work::Priority },
    SetPaused { building: buildings::BuildingId, paused: bool },
    AdjustRole { role: jobs::RoleKind, delta: i32 },
    SetRoleOrder { order: Vec<jobs::RoleKind> },
    SetRoleWeight { role: jobs::RoleKind, weight: f32 },
    SetAutoStaff { on: bool },
    Unlock { who: colony::BeaverId },
    SetWorkArea { building: buildings::BuildingId, area: work::Area },
    SetHaul { order: work::HaulOrder },
    SetBalance { building: buildings::BuildingId, on: bool },
    SetBargeRoute { barge: buildings::BargeId, from: buildings::BuildingId, to: buildings::BuildingId, goods: Vec<buildings::Good> },
    ClearBargeRoute { barge: buildings::BargeId },
    AnswerJoin { id: society::RequestId, answer: society::Answer },
    OrderCanal { cells: Vec<water::Cell> },
    CancelSite { building: buildings::BuildingId },
    SetCutRule { building: buildings::BuildingId, rule: forestry::CutRule },
    SetPlantRule { building: buildings::BuildingId, rule: forestry::PlantRule },
}

/// Seconds each part of `SimState::step` has taken since the counters were last cleared.

impl SimState {
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        let logged = self.recorder.is_some().then(|| cmd.clone());
        let res = self.apply_inner(cmd);
        if let (Ok(()), Some(c)) = (&res, logged) {
            let step = self.steps;
            if let Some(r) = self.recorder.as_mut() {
                r.push((step, c));
            }
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
            Command::OrderDam { cells, height } => self.order_dam(&cells, height).map(|_| ()),
            Command::OrderDamTo { cells, crest } => self.order_dam_to(&cells, crest).map(|_| ()),
            Command::RepairDam { cell } => self.repair_dam(cell).map(|_| ()),
            Command::RepairBuilding { building } => maintenance::order(self, building, maintenance::Refit::Repair),
            Command::ReinforceBuilding { building } => maintenance::order(self, building, maintenance::Refit::Reinforce),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::Assign { who, to } => {
                if let Some(kit) = who.iter().find(|id| self.colony.beaver(**id).is_some_and(|b| b.stage == colony::Stage::Kit)) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is a kit and cannot work", kit.0)));
                }
                if let Some(id) = who.iter().find(|id| !self.agents.by_id.contains_key(id)) {
                    return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
                }
                for id in who {
                    jobs::order(self, id, to);
                    self.agents.assign(id, to);
                }
                Ok(())
            }
            Command::PlaceSite { kind, pos } => self.place_site(kind, pos).map(|_| ()),
            Command::FillSlot { building, slot, who } => {
                let stage = self.colony.beaver(who).map(|b| b.stage).ok_or(work::WorkError::NoSuchBeaver);
                stage
                    .and_then(|st| if st == colony::Stage::Kit { Err(work::WorkError::NotAdult) } else { Ok(()) })
                    .and_then(|()| self.work.place(building).ok_or(work::WorkError::NoSuchWorkplace))
                    .and_then(|p| match p.slots.get(slot) {
                        None => Err(work::WorkError::SlotOutOfRange),
                        Some(s) if s.worker.is_some() => Err(work::WorkError::SlotTaken),
                        Some(_) => Ok(()),
                    })
                    .map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                jobs::place_by_hand(self, building, slot, who)
            }
            Command::EmptySlot { building, slot } => {
                let who = self.work.place(building).and_then(|p| p.slots.get(slot)).and_then(|s| s.worker);
                self.work.unassign(building, slot).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                if let Some(w) = who {
                    self.work.managed.remove(&w);
                }
                if let Ok(p) = self.work.place_mut(building) {
                    p.max_workers = p.max_workers.min(p.filled());
                }
                Ok(())
            }
            Command::AddWorker { building } => jobs::add_worker(self, building),
            Command::RemoveWorker { building } => jobs::remove_worker(self, building),
            Command::SetPriority { building, priority } => jobs::set_priority(self, building, priority),
            Command::SetPaused { building, paused } => jobs::set_paused(self, building, paused),
            Command::AdjustRole { role, delta } => jobs::adjust_role(self, role, delta),
            Command::SetRoleWeight { role, weight } => {
                if !weight.is_finite() || !(0.1..=20.0).contains(&weight) {
                    return Err(SimError::InvalidCommand("role weight must be 0.1..20".into()));
                }
                self.work.role_weights[role as usize] = weight;
                Ok(())
            }
            Command::SetRoleOrder { order } => jobs::set_role_order(self, order),
            Command::SetAutoStaff { on } => {
                self.work.auto_organise = on;
                Ok(())
            }
            Command::Unlock { who } => jobs::unlock(self, who),
            Command::SetWorkArea { building, area } => {
                forestry::check_area(self, building, &area)?;
                self.work.set_area(building, area).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::OrderCanal { cells } => self.order_canal(&cells).map(|_| ()),
            Command::CancelSite { building } => self.cancel_site(building).map(|_| ()),
            Command::SetCutRule { building, rule } => forestry::set_cut_rule(self, building, rule),
            Command::SetPlantRule { building, rule } => forestry::set_plant_rule(self, building, rule),
            Command::SetBargeRoute { barge, from, to, goods } => shipping::set_route(self, barge, from, to, goods).map_err(SimError::InvalidCommand),
            Command::ClearBargeRoute { barge } => shipping::clear_route(self, barge).map_err(SimError::InvalidCommand),
            Command::SetBalance { building, on } => {
                let store = self.buildings.building(building).is_some_and(|b| b.is_complete() && agents::is_store(b.kind));
                if !store {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished store", building.0)));
                }
                if on {
                    self.work.balanced.insert(building);
                } else {
                    self.work.balanced.remove(&building);
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
