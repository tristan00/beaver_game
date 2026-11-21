//! Player commands and their validation/dispatch boundary.
use crate::{SimError, SimState, agents, buildings, colony, dams, forestry, invalid, jobs, maintenance, sandbox, society, starts, water, work};

/// How a dam grows when the player asks for more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamGrowth {
    Raise,
    Widen,
    Extend,
}


#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Regenerate { seed: u64 },
    BuildDam { cells: Vec<water::Cell>, height: f32 },
    /// Draw a new dam along these cells; beavers then build every cell.
    PlaceDam { cells: Vec<water::Cell> },
    /// Grow a dam; `cells` narrows a raise to those cells.
    GrowDam { dam: buildings::BuildingId, growth: DamGrowth, cells: Vec<water::Cell> },
    SetDamMaintain { dam: buildings::BuildingId, on: bool },
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
    AnswerJoin { id: society::RequestId, answer: society::Answer },
    CancelSite { building: buildings::BuildingId },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family() -> (SimState, Vec<colony::BeaverId>) {
        let mut s = SimState::new(1);
        s.colony.found((0.0, 3.0), &colony::RiverLand::default()).unwrap();
        s.nav = crate::nav::NavGrid::new(32, 32);
        let ids: Vec<_> = s.colony.beavers.iter().map(|b| b.id).collect();
        for (i, &id) in ids.iter().enumerate() {
            s.agents.spawn(id, (2, 2 + i));
            s.nav.reserve(id, (2, 2 + i));
        }
        (s, ids)
    }

    #[test]
    fn move_replaces_route_and_preserves_load() {
        let (mut s, ids) = family();
        let id = ids[0];
        let a = s.agents.by_id.get_mut(&id).unwrap();
        a.path = Some(crate::nav::Path { steps: vec![((3, 2), crate::nav::Gait::Walk)], cost: 1.0 });
        a.step_frac = 0.5;
        a.load = Some(agents::Load { good: buildings::Good::Sticks, amount: 2.0, booked: false });
        s.apply(Command::Assign { who: vec![id], to: Some(agents::Assignment::Go((2, 8))) }).unwrap();
        let a = s.agents.get(id).unwrap();
        assert!(a.path.is_none());
        assert_eq!(a.step_frac, 0.0);
        assert_eq!(a.load.unwrap().amount, 2.0);
        assert_eq!(a.assignment, Some(agents::Assignment::Go((2, 8))));
        s.apply(Command::Assign { who: vec![id], to: None }).unwrap();
        assert_eq!(s.agents.get(id).unwrap().assignment, None);
        assert!(s.agents.get(id).unwrap().load.is_some());
    }

    #[test]
    fn group_move_accepts_kits_and_chooses_distinct_destinations() {
        let (mut s, ids) = family();
        s.colony.beavers[0].stage = colony::Stage::Kit;
        s.apply(Command::Assign { who: ids.clone(), to: Some(agents::Assignment::Go((12, 12))) }).unwrap();
        let cells: std::collections::BTreeSet<_> = ids.iter().map(|id| match s.agents.get(*id).unwrap().assignment {
            Some(agents::Assignment::Go(c)) => c, _ => panic!("missing order"),
        }).collect();
        assert_eq!(cells.len(), ids.len());
        let prior = s.agents.get(ids[0]).unwrap().assignment;
        assert!(s.apply(Command::Assign { who: ids, to: Some(agents::Assignment::Go((99, 99))) }).is_err());
        assert_eq!(s.agents.get(s.colony.beavers[0].id).unwrap().assignment, prior);
    }
}

/// Seconds each part of `SimState::step` has taken since the counters were last cleared.

impl SimState {
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        // Preserve synchronous input feedback on a presentation snapshot. Only
        // the worker advances time; successful local commands are replayed there
        // in order and acknowledged before their predictions are retired.
        let predicted = self.client_commands.is_some().then(|| cmd.clone());
        let events = predicted.as_ref().map(|_| self.colony.events.clone());

        let diagnostic = crate::trace::enabled().then(|| format!("{cmd:?}"));
        let logged = self.recorder.is_some().then(|| cmd.clone());
        let res = self.apply_inner(cmd);
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
            Command::SetDamMaintain { dam, on } => dams::set_maintain(self, dam, on),
            Command::RepairBuilding { building } => maintenance::order(self, building, maintenance::Refit::Repair),
            Command::ReinforceBuilding { building } => maintenance::order(self, building, maintenance::Refit::Reinforce),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::Assign { who, to } => {
                if let Some(kit) = who.iter().find(|id| to.is_some() && !matches!(to, Some(agents::Assignment::Go(_))) && self.colony.beaver(**id).is_some_and(|b| b.stage == colony::Stage::Kit)) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is a kit and cannot work", kit.0)));
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
                        let mut cells: Vec<_> = (target.1.saturating_sub(radius)..=target.1 + radius)
                            .flat_map(|y| (target.0.saturating_sub(radius)..=target.0 + radius).map(move |x| (x, y)))
                            .filter(|c| !destinations.contains(c)).collect();
                        cells.sort_by_key(|c| (c.0.abs_diff(target.0).pow(2) + c.1.abs_diff(target.1).pow(2), *c));
                        let Some(cell) = cells.into_iter().find(|&c| {
                            self.nav.surface_at(c).is_some_and(|s| s != crate::nav::Surface::Solid)
                                && self.nav.may_reach(from, [c])
                        }) else {
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
            Command::CancelSite { building } => self.cancel_site(building).map(|_| ()),
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
