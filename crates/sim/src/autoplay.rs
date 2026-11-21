include!(concat!(env!("OUT_DIR"), "/autoplay_constants.rs"));
use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::resources::ResourceKind;
use crate::hud::{self, Fix};
use crate::water::Cell;
use crate::{Command, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Attentive,
    IgnoresAlerts,
    HandsOff,
}

#[derive(Clone)]
pub struct Player {
    pub style: Style,
    pub camp: Cell,
    pub dam: Option<(Vec<Cell>, f32)>,
    pub placed: Vec<(f64, BuildingKind, Result<BuildingId, String>)>,
    pub log: Vec<String>,

    food_relief: Vec<(crate::colony::BeaverId, crate::agents::Assignment)>,
}


impl Player {
    pub fn new(style: Style, camp: Cell, dam: Option<(Vec<Cell>, f32)>) -> Player {
        Player { style, camp, dam, placed: Vec::new(), log: Vec::new(), food_relief: Vec::new() }
    }

    fn note(&mut self, s: &SimState, line: String) {
        self.log.push(format!("day {:>5.1}: {line}", s.time_days));
    }

    fn has(&self, kind: BuildingKind) -> bool {
        self.placed.iter().any(|(_, k, r)| *k == kind && r.is_ok())
    }

    fn tried_lately(&self, s: &SimState, kind: BuildingKind) -> bool {
        self.placed.iter().any(|(d, k, _)| *k == kind && s.time_days - d < 10.0)
            || s.buildings.buildings.iter().any(|b| b.kind == kind && !b.is_complete())
            || !self.has_hands(s)
    }

    fn has_hands(&self, s: &SimState) -> bool {
        let workers = s.colony.beavers.iter().filter(|b| b.stage != crate::colony::Stage::Kit).count();
        let open = s.buildings.buildings.iter().filter(|b| !b.is_complete()).count();
        open < (workers / 2).max(1)
    }

    pub fn place(&mut self, s: &mut SimState, kind: BuildingKind) -> Option<BuildingId> {
        let around = s.buildings.buildings.iter().find(|b|b.kind==BuildingKind::Shelter && b.is_complete()).map_or(self.camp,|b|b.pos);
        self.place_at(s, kind, around)
    }

    pub fn place_at(&mut self, s: &mut SimState, kind: BuildingKind, around: Cell) -> Option<BuildingId> {
        let keep_clear: Vec<Cell> = if kind == BuildingKind::Dam { Vec::new() } else { self.dam.iter().flat_map(|(c, _)| c.iter().copied()).collect() };
        let res = place_best(s, kind, around, &keep_clear);
        self.placed.push((s.time_days, kind, res.clone()));
        match res {
            Ok(id) => {
                self.note(s, format!("placed {kind:?}"));
                Some(id)
            }
            Err(e) => {
                self.note(s, format!("could not place {kind:?}: {e}"));
                None
            }
        }
    }

    fn release_food_relief(&mut self, s: &mut SimState) {
        let days=if matches!(crate::calendar::Season::at(s.time_days),crate::calendar::Season::Autumn|crate::calendar::Season::Winter) {crate::hud::WINTER_RESERVE_DAYS} else {30.0};
        let target=(crate::hud::food_need_per_day(s)*days).min(s.buildings.stores.capacity(Good::Food)*0.9);
        if s.colony.food >= target {
            for (who, order) in std::mem::take(&mut self.food_relief) {
                if s.agents.get(who).is_some_and(|a| a.assignment == Some(order)) {
                    let _ = s.apply(Command::Unlock { who });
                }
            }
        }
    }

    fn return_new_workers(&mut self, s: &mut SimState, locked: &std::collections::HashSet<crate::colony::BeaverId>) {
        // Unlocking an area order cancels it before any gathering can happen.
        let hand_back: Vec<_> = s.colony.beavers.iter().filter(|b| !locked.contains(&b.id)
            && s.work.is_locked(b.id)).map(|b| b.id).collect();
        for who in hand_back {
            if let Some(order @ crate::agents::Assignment::Area(ResourceKind::Plant, _)) = s.agents.get(who).and_then(|a|a.assignment) {
                self.food_relief.push((who,order));
            } else { let _ = s.apply(Command::Unlock { who }); }
        }
    }

    fn apply_response(&mut self,s:&mut SimState,fix:Fix) {
        let locked:std::collections::HashSet<_>=s.colony.beavers.iter().filter(|b|s.work.is_locked(b.id)).map(|b|b.id).collect();
        if let Err(e)=hud::apply_fix(s,fix) {self.note(s,format!("fix {fix:?} refused: {e}"));}
        self.return_new_workers(s,&locked);
    }

    pub fn act(&mut self, s: &mut SimState) {
        if self.style == Style::HandsOff || !s.colony.is_founded() {
            return;
        }
        self.release_food_relief(s);
        let count = |s: &SimState, k: BuildingKind| s.buildings.buildings.iter().filter(|b| b.kind == k).count();
        if count(s, BuildingKind::Shelter) == 0 && !self.tried_lately(s, BuildingKind::Shelter) {
            self.place(s, BuildingKind::Shelter);
        }
        if !self.has(BuildingKind::Dam) && !self.tried_lately(s, BuildingKind::Dam) {
            let dam = self.dam.clone().or_else(|| crate::starts::dam_near(s, self.camp, 24));
            if let Some((cells, _)) = dam {
                match s.apply(Command::PlaceDam { cells }).map(|_| s.buildings.buildings.last().expect("placed dam").id) {
                    Ok(id) => {
                        self.placed.push((s.time_days, BuildingKind::Dam, Ok(id)));
                        self.note(s, "drew the dam".into());
                    }
                    Err(e) => {
                        self.placed.push((s.time_days, BuildingKind::Dam, Err(e.to_string())));
                        self.note(s, format!("dam refused: {e}"));
                    }
                }
            } else {
                self.placed.push((s.time_days, BuildingKind::Dam, Err("no dam site in sight".into())));
            }
        }
        if count(s, BuildingKind::Warehouse) == 0 && s.has_building(BuildingKind::Shelter) && !self.tried_lately(s, BuildingKind::Warehouse) {
            self.place(s, BuildingKind::Warehouse);
        }
        let stores: Vec<BuildingId> = s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.is_complete() && b.kind == BuildingKind::Warehouse)
            .map(|b| b.id)
            .filter(|id| !s.work.balanced.contains(id))
            .collect();
        for b in stores {
            let r = s.apply(Command::SetBalance { building: b, on: true });
            self.note(s, format!("store {} balances: {r:?}", b.0));
        }
        let families = s.colony.beavers.iter().map(|b|b.family).collect::<std::collections::HashSet<_>>().len();
        let young_adults = s.colony.beavers.iter().any(|b|b.stage == crate::colony::Stage::Adult && b.mate.is_none());
        let wanted_homes = families + usize::from(young_adults || !s.society.pending().is_empty());
        if count(s, BuildingKind::Shelter) < wanted_homes && !self.tried_lately(s, BuildingKind::Shelter) {
            self.place(s, BuildingKind::Shelter);
        }

        if self.style == Style::IgnoresAlerts {
            return;
        }
        let Some(p) = hud::problems(s).into_iter().next() else { return };
        match p.fix {
            Fix::Place(kind) => {
                if !self.tried_lately(s, kind) {
                    self.place(s, kind);
                }
                if p.kind==hud::ProblemKind::WinterFood {
                    if let Some(fix)=p.alt_fix {self.apply_response(s,fix);}
                }
            }
            Fix::OpenJobs | Fix::Focus | Fix::OpenDam { .. } => {}
            fix => self.apply_response(s,fix),
        }
    }
}

pub fn place_best(s: &mut SimState, kind: BuildingKind, around: Cell, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 2..=29, keep_clear)
}


fn place_ring(s: &mut SimState, kind: BuildingKind, around: Cell, radii: std::ops::RangeInclusive<usize>, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    let walker = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult).map(|b| b.id);
    let from = walker.and_then(|w| s.agents.get(w)).map(|a| a.cell);
    let mut last = String::from("no cell tried");
    for r in radii {
        for y in around.1.saturating_sub(r)..=around.1 + r {
            for x in around.0.saturating_sub(r)..=around.0 + r {
                if x.abs_diff(around.0).max(y.abs_diff(around.1)) != r {
                    continue;
                }
                if let Err(e) = s.check_site(kind, (x, y)) {
                    last = e.to_string();
                    continue;
                }
                if crate::buildings::Buildings::footprint(kind, (x, y)).iter().any(|c| keep_clear.contains(c)) {
                    last = "on the planned dam".into();
                    continue;
                }
                if let (Some(w), Some(f)) = (walker, from) {
                    if s.nav.find_path(w, f, crate::nav::Goal::Beside((x, y))).is_none() {
                        last = "unreachable".into();
                        continue;
                    }
                }
                s.apply(Command::PlaceSite { kind, pos: (x, y) }).map_err(|e| e.to_string())?;
                let id = s.buildings.buildings.last().map(|b| b.id).ok_or("placed nothing")?;
                if let (Some(w), Some(f), Some(pile)) = (walker, from, s.work.supplies) {
                    if s.nav.find_path(w, f, crate::nav::Goal::Beside(pile)).is_none() {
                        let _ = s.apply(Command::CancelSite { building: id });
                        last = "would wall the family in".into();
                        continue;
                    }
                }
                return Ok(id);
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod relief_tests {
    use super::*;
    use crate::agents::Assignment;
    #[test]
    fn emergency_gathering_persists_and_releases_only_its_own_orders() {
        let mut s=SimState::new(1);
        s.colony.found((1.0,1.0),&crate::grid::Grid::new(4,4,0.0)).unwrap();
        let ids:Vec<_>=s.colony.beavers.iter().take(3).map(|b|b.id).collect();
        let gather=Assignment::Area(ResourceKind::Plant,crate::work::Area{min:(0,0),max:(3,3)});
        for &who in &ids {
            s.agents.spawn(who,(1,1));
            s.agents.assign(who,Some(gather));
            s.work.player_orders.insert(who);
        }
        let mut p=Player::new(Style::Attentive,(1,1),None);
        p.return_new_workers(&mut s,&[ids[1]].into_iter().collect());
        assert_eq!(p.food_relief.len(),2);
        s.colony.food=0.0;
        p.release_food_relief(&mut s);
        assert!(ids.iter().all(|&id|s.agents.get(id).unwrap().assignment==Some(gather)));
        s.time_days=190.0;s.colony.food=crate::hud::food_need_per_day(&s)*31.0;
        p.release_food_relief(&mut s);
        assert!(ids.iter().all(|&id|s.agents.get(id).unwrap().assignment==Some(gather)));
        s.time_days=0.0;
        // A later player order must survive the caretaker's cleanup.
        let override_order=Assignment::Go((3,3));
        s.agents.assign(ids[2],Some(override_order));
        s.colony.food=crate::hud::food_need_per_day(&s)*31.0;
        p.release_food_relief(&mut s);
        assert_eq!(s.agents.get(ids[0]).unwrap().assignment,None);
        assert_eq!(s.agents.get(ids[1]).unwrap().assignment,Some(gather));
        assert_eq!(s.agents.get(ids[2]).unwrap().assignment,Some(override_order));
    }
}
