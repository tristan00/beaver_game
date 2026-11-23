use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::resources::ResourceKind;
use crate::hud::{self, Fix};
use crate::water::Cell;
use crate::{Command, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Style {
    Attentive,
    IgnoresAlerts,
    HandsOff,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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
        let workers = s.colony.beavers.iter().filter(|b| b.can_work()).count();
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
                    let _ = s.apply(Command::Assign { who: vec![who], to: None });
                }
            }
        }
    }

    /// Follows an alert's fix; foragers it sends out in an emergency are called back once
    /// food recovers.
    fn apply_response(&mut self,s:&mut SimState,fix:Fix) {
        let before:std::collections::HashMap<_,_>=s.agents.by_id.iter().map(|(&id,a)|(id,a.assignment)).collect();
        if let Err(e)=hud::apply_fix(s,fix) {self.note(s,format!("fix {fix:?} refused: {e}"));}
        for (&who,a) in &s.agents.by_id {
            if let Some(order @ crate::agents::Assignment::Area(ResourceKind::Plant, _)) = a.assignment {
                if before.get(&who).copied().flatten() != Some(order) && !self.food_relief.iter().any(|r| r.0 == who) {
                    self.food_relief.push((who, order));
                }
            }
        }
    }

    /// Staffs by hand with the player's own buttons: gatherers for whatever the stores are
    /// short of (at most half the colony), then every open seat that has work.
    fn staff(&mut self, s: &mut SimState) {
        use crate::jobs::RoleKind;
        let workers = s.colony.beavers.iter().filter(|b| b.can_work()).count();
        let mut room = workers.div_ceil(2).max(1);
        let roles = [(RoleKind::Forager, Good::Food, 0.5), (RoleKind::Logger, Good::Sticks, 0.34), (RoleKind::MudDigger, Good::Clay, 0.2)];
        let short = roles.map(|(_, good, _)| crate::jobs::on_hand(s, good) < crate::jobs::stock_target(s, good));
        // One gatherer for each thing the stores are short of, then more up to each share.
        let mut wants = [0usize; 3];
        for k in 0..3 {
            if short[k] && room > 0 {
                wants[k] = 1;
                room -= 1;
            }
        }
        for (k, &(_, _, share)) in roles.iter().enumerate() {
            if short[k] {
                let more = ((workers as f32 * share).ceil() as usize).saturating_sub(1).min(room);
                wants[k] += more;
                room -= more;
            }
        }
        for (k, (role, _, _)) in roles.into_iter().enumerate() {
            let relief = self.food_relief.iter().filter(|r| role == RoleKind::Forager && s.agents.get(r.0).is_some_and(|a| a.assignment == Some(r.1))).count();
            let out = crate::jobs::out_gathering(s, role).len().saturating_sub(relief);
            let want = wants[k];
            for _ in out..want {
                if s.apply(Command::AdjustRole { role, delta: 1 }).is_err() {
                    break;
                }
            }
            for _ in want..out {
                if s.apply(Command::AdjustRole { role, delta: -1 }).is_err() {
                    break;
                }
            }
        }
        // Builders waiting on a dam with no sticks, or on a stalled site, are called off.
        let idle_seats: Vec<BuildingId> = s
            .work
            .places
            .iter()
            .filter(|p| p.filled() > 0)
            .filter(|p| {
                let b = p.building;
                if crate::agents::is_site(s, p) {
                    crate::agents::site_stalled(s, p)
                } else if s.buildings.building(b).is_some_and(|x| matches!(x.kind, BuildingKind::Dam | BuildingKind::Canal)) {
                    !(crate::dams::has_work(s, b) || crate::canals::has_work(s, b)) || crate::jobs::on_hand(s, Good::Sticks) < STICKS_TO_BUILD
                } else {
                    false
                }
            })
            .map(|p| p.building)
            .collect();
        for b in idle_seats {
            while s.apply(Command::RemoveWorker { building: b }).is_ok() {}
        }
        let mut seats: Vec<(std::cmp::Reverse<crate::work::Priority>, bool, BuildingId)> = s
            .work
            .places
            .iter()
            .filter(|p| !p.paused)
            .filter(|p| {
                let site = crate::agents::is_site(s, p);
                let b = p.building;
                if site {
                    !crate::agents::site_stalled(s, p)
                } else if s.buildings.building(b).is_some_and(|x| matches!(x.kind, BuildingKind::Dam | BuildingKind::Canal)) {
                    // Dam and canal work burns sticks: no builders while there are none to build with.
                    (crate::dams::has_work(s, b) || crate::canals::has_work(s, b)) && crate::jobs::on_hand(s, Good::Sticks) >= STICKS_TO_BUILD
                } else {
                    s.buildings.building(b).is_some_and(|x| x.operational())
                }
            })
            .map(|p| (std::cmp::Reverse(p.priority), !crate::agents::is_site(s, p), p.building))
            .collect();
        seats.sort();
        for (_, _, b) in seats {
            while s.apply(Command::AddWorker { building: b }).is_ok() {}
        }
        // Nobody stands about: whoever is still free gathers what the stores are shortest of.
        loop {
            let free = s.colony.beavers.iter().any(|b| b.can_work() && !crate::jobs::is_busy(s, b.id));
            let need = roles
                .iter()
                .filter(|(_, good, _)| crate::jobs::on_hand(s, *good) < crate::jobs::stock_target(s, *good))
                .min_by(|a, b| {
                    let left = |g: Good| crate::jobs::on_hand(s, g) / crate::jobs::stock_target(s, g).max(1.0);
                    left(a.1).total_cmp(&left(b.1))
                })
                .map(|r| r.0);
            match need {
                Some(role) if free && s.apply(Command::AdjustRole { role, delta: 1 }).is_ok() => {}
                _ => break,
            }
        }
    }

    pub fn act(&mut self, s: &mut SimState) {
        if self.style == Style::HandsOff || !s.colony.is_founded() {
            return;
        }
        self.release_food_relief(s);
        self.staff(s);
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
        if count(s, BuildingKind::Larder) == 0 && s.has_building(BuildingKind::Warehouse) && !self.tried_lately(s, BuildingKind::Larder) {
            self.place(s, BuildingKind::Larder);
        }
        if count(s, BuildingKind::Forester) == 0 && s.has_building(BuildingKind::Larder) && !self.tried_lately(s, BuildingKind::Forester) {
            self.place(s, BuildingKind::Forester);
        }
        if count(s, BuildingKind::Logistics) == 0 && s.has_building(BuildingKind::Larder) && !self.tried_lately(s, BuildingKind::Logistics) {
            self.place(s, BuildingKind::Logistics);
        }
        let stores: Vec<BuildingId> = s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.is_complete() && b.kind == BuildingKind::Logistics)
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

/// Sticks the bot wants in store before it puts builders on a dam or canal.
const STICKS_TO_BUILD: f32 = 10.0;

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
        }
        let mut p=Player::new(Style::Attentive,(1,1),None);
        p.food_relief=vec![(ids[0],gather),(ids[2],gather)];
        s.colony.food=0.0;
        p.release_food_relief(&mut s);
        assert!(ids.iter().all(|&id|s.agents.get(id).unwrap().assignment==Some(gather)), "kept while food is short");
        // A later player order must survive the caretaker's cleanup.
        let override_order=Assignment::Go((3,3));
        s.agents.assign(ids[2],Some(override_order));
        s.colony.food=crate::hud::food_need_per_day(&s)*31.0;
        p.release_food_relief(&mut s);
        assert_eq!(s.agents.get(ids[0]).unwrap().assignment,None);
        assert_eq!(s.agents.get(ids[1]).unwrap().assignment,Some(gather));
        assert_eq!(s.agents.get(ids[2]).unwrap().assignment,Some(override_order));
        assert!(p.food_relief.is_empty());
    }
}
