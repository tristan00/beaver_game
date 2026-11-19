use crate::buildings::{BuildingId, BuildingKind, Good, Recipe};
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

pub struct Player {
    pub style: Style,
    pub camp: Cell,
    pub dam: Option<(Vec<Cell>, f32)>,
    pub placed: Vec<(f64, BuildingKind, Result<BuildingId, String>)>,
    pub log: Vec<String>,
    recipe: Option<(BuildingId, Recipe)>,
    next_explore: f64,
    far_dock: Option<BuildingId>,
    home_dock: Option<BuildingId>,
    routed: bool,
}

pub const FAR_DOCK_REACH: usize = 30;
pub const FAR_DEPOSIT: usize = 150;
pub const DEPOSIT_REACH: usize = 200;

impl Player {
    pub fn new(style: Style, camp: Cell, dam: Option<(Vec<Cell>, f32)>) -> Player {
        Player { style, camp, dam, placed: Vec::new(), log: Vec::new(), recipe: None, next_explore: 0.0, far_dock: None, home_dock: None, routed: false }
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
        let around = self.camp;
        self.place_at(s, kind, around)
    }

    pub fn place_at(&mut self, s: &mut SimState, kind: BuildingKind, around: Cell) -> Option<BuildingId> {
        let keep_clear: Vec<Cell> = if kind == BuildingKind::Dam { Vec::new() } else { self.dam.iter().flat_map(|(c, _)| c.iter().copied()).collect() };
        let res = if matches!(kind, BuildingKind::QuarryCamp | BuildingKind::MudPit) { place_deposit(s, kind, around, &keep_clear) } else { place_best(s, kind, around, &keep_clear) };
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

    pub fn act(&mut self, s: &mut SimState) {
        if self.style == Style::HandsOff || !s.colony.is_founded() {
            return;
        }
        if self.placed.is_empty() {
            self.place(s, BuildingKind::Burrow);
        }
        let huts = s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::ForagerHut).count();
        let hut_building = s.buildings.buildings.iter().any(|b| b.kind == BuildingKind::ForagerHut && !b.is_complete());
        let recent = self.placed.iter().any(|(d, k, _)| *k == BuildingKind::ForagerHut && s.time_days - d < 10.0);
        if s.tech.allows(BuildingKind::ForagerHut) && huts < 1 + s.colony.population() / 5 && !hut_building && !recent {
            self.place(s, BuildingKind::ForagerHut);
        }
        if !self.has(BuildingKind::Dam) && !self.tried_lately(s, BuildingKind::Dam) {
            let dam = self.dam.clone().or_else(|| crate::starts::dam_near(s, self.camp, 24));
            if let Some((cells, h)) = dam {
                match s.order_dam(&cells, h) {
                    Ok(id) => {
                        self.placed.push((s.time_days, BuildingKind::Dam, Ok(id)));
                        self.note(s, "ordered the dam".into());
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
        for kind in [BuildingKind::LoggingCamp, BuildingKind::Warehouse, BuildingKind::MudPit, BuildingKind::Workshop, BuildingKind::QuarryCamp] {
            if s.tech.allows(kind) && s.missing_prerequisites(kind).is_empty() && !self.has(kind) && !self.tried_lately(s, kind) {
                self.place(s, kind);
            }
        }
        self.explore(s);
        self.ship(s);
        self.tend_logging(s);
        let stores: Vec<BuildingId> = s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.is_complete() && matches!(b.kind, BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::FoodCache) || Some(b.id) == self.home_dock && b.is_complete())
            .map(|b| b.id)
            .filter(|id| !s.work.balanced.contains(id))
            .collect();
        for b in stores {
            let r = s.apply(Command::SetBalance { building: b, on: true });
            self.note(s, format!("store {} balances: {r:?}", b.0));
        }
        let pop = s.colony.population();
        let count = |s: &SimState, k: BuildingKind| s.buildings.buildings.iter().filter(|b| b.kind == k).count();
        if s.tech.allows(BuildingKind::Lodge) && pop >= 6 && count(s, BuildingKind::Lodge) < pop.div_ceil(8) && !self.tried_lately(s, BuildingKind::Lodge) {
            self.place(s, BuildingKind::Lodge);
        }
        if s.tech.allows(BuildingKind::LoggingCamp) && count(s, BuildingKind::LoggingCamp) < 1 + pop / 10 && !self.tried_lately(s, BuildingKind::LoggingCamp) {
            self.place(s, BuildingKind::LoggingCamp);
        }
        if let Some(bench) = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::Workshop && b.is_complete()).map(|b| b.id) {
            let short = |g: Good| s.work.places.iter().filter_map(|p| p.site.as_ref()).any(|st| st.missing(g) > 0.0) && s.buildings.stores.amount(g) < 1.0;
            let want = if short(Good::Tools) { Recipe::Tools } else if short(Good::Baskets) { Recipe::Baskets } else { Recipe::Rope };
            if self.recipe != Some((bench, want)) {
                let r = s.apply(Command::SetRecipe { building: bench, recipe: Some(want) });
                self.recipe = Some((bench, want));
                self.note(s, format!("workshop makes {want:?}: {r:?}"));
            }
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
            }
            Fix::OpenJobs | Fix::Focus => {}
            fix => {
                if let Err(e) = hud::apply_fix(s, fix) {
                    self.note(s, format!("fix {fix:?} refused: {e}"));
                }
            }
        }
    }
}

impl Player {
    fn explore(&mut self, s: &mut SimState) {
        if s.time_days < self.next_explore || s.colony.population() < 4 {
            return;
        }
        self.next_explore = s.time_days + 12.0;
        let seen = |k: ResourceKind| s.resources.items.iter().any(|r| r.kind == k && r.amount >= 20.0 && s.fog.explored(r.cell));
        if seen(ResourceKind::StoneOutcrop) && seen(ResourceKind::MudBank) {
            return;
        }
        let n = s.world.size as isize;
        let k = (s.time_days / 12.0) as usize;
        let mut target = None;
        'dirs: for i in 0..8 {
            let a = (k + i * 3) % 8;
            let (dx, dy) = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)][a];
            for r in (20..=40).step_by(4) {
                let (x, y) = (self.camp.0 as isize + dx * r, self.camp.1 as isize + dy * r);
                if x < 2 || y < 2 || x >= n - 2 || y >= n - 2 {
                    continue 'dirs;
                }
                let c = (x as usize, y as usize);
                if !s.fog.explored(c) && s.water.depth.get(c.0, c.1).is_some_and(|&d| d < 0.05) {
                    target = Some(c);
                    break 'dirs;
                }
            }
        }
        let Some(to) = target else { return };
        let who = s.colony.beavers.iter().filter(|b| b.stage == crate::colony::Stage::Adult).map(|b| b.id).find(|&b| s.work.slot_of(b).is_none());
        if let Some(who) = who {
            let r = s.apply(Command::Assign { who: vec![who], to: Some(crate::agents::Assignment::Go(to)) });
            self.note(s, format!("sent a scout toward {to:?}: {r:?}"));
        }
    }

    fn place_dock(&mut self, s: &mut SimState, spot: Option<Cell>) -> Option<BuildingId> {
        let r = match spot {
            Some(p) => s.apply(Command::PlaceSite { kind: BuildingKind::Dock, pos: p }).map_err(|e| e.to_string()).and_then(|_| s.buildings.buildings.last().map(|b| b.id).ok_or_else(|| "placed nothing".into())),
            None => Err("no dock spot on water a barge can reach".to_string()),
        };
        self.placed.push((s.time_days, BuildingKind::Dock, r.clone()));
        self.note(s, format!("dock: {r:?}"));
        r.ok()
    }

    fn tend_logging(&mut self, s: &mut SimState) {
        if (s.time_days as u64) % 5 != 0 {
            return;
        }
        let camps: Vec<(BuildingId, Cell, crate::work::Area)> = s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::LoggingCamp && b.is_complete())
            .filter_map(|b| Some((b.id, b.pos, s.work.place(b.id)?.area?)))
            .collect();
        for (id, pos, area) in camps {
            if crate::forestry::cuttable_in_area(s, id) >= 6 {
                continue;
            }
            let min_age = crate::forestry::cut_min_age(s, Some(id));
            let (w, h) = (area.max.0 - area.min.0, area.max.1 - area.min.1);
            let n = s.world.size;
            let mut best: Option<(usize, crate::work::Area)> = None;
            for cy in (pos.1.saturating_sub(24)..(pos.1 + 24).min(n)).step_by(3) {
                for cx in (pos.0.saturating_sub(24)..(pos.0 + 24).min(n)).step_by(3) {
                    if !s.fog.explored((cx, cy)) {
                        continue;
                    }
                    let a = crate::work::Area { min: (cx.saturating_sub(w / 2), cy.saturating_sub(h / 2)), max: ((cx + w / 2).min(n - 1), (cy + h / 2).min(n - 1)) };
                    let k = s.resources.items.iter().filter(|r| a.contains(r.cell) && crate::forestry::cuttable(r, min_age)).count();
                    if best.as_ref().is_none_or(|b| k > b.0) {
                        best = Some((k, a));
                    }
                }
            }
            if let Some((k, a)) = best.filter(|b| b.0 >= 12) {
                let r = s.apply(Command::SetWorkArea { building: id, area: a });
                self.note(s, format!("moved logging area to {:?} ({k} trees): {r:?}", a.min));
            }
        }
    }

    fn ship(&mut self, s: &mut SimState) {
        let home = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::Warehouse && b.is_complete()).cloned();
        let Some(home) = home else { return };
        if !s.tech.allows(BuildingKind::Dock) {
            return;
        }
        let home_spot = match self.home_dock.and_then(|d| s.buildings.building(d)) {
            Some(d) => Some(d.pos),
            None => dock_spot(s, home.pos, None, 14),
        };
        let Some(home_spot) = home_spot else { return };
        let home_berths = crate::shipping::berths_at(s, home_spot);
        if self.far_dock.is_none() {
            if self.placed.iter().any(|(d, k, _)| *k == BuildingKind::Dock && s.time_days - d < 10.0) {
                return;
            }
            let mut far: Vec<crate::buildings::Building> = s
                .buildings
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, BuildingKind::QuarryCamp | BuildingKind::MudPit) && b.is_complete())
                .filter(|b| crate::stock::gap(b, &home) > FAR_DEPOSIT)
                .cloned()
                .collect();
            far.sort_by_key(|b| std::cmp::Reverse(crate::stock::gap(b, &home)));
            let Some((camp, spot)) = far.iter().find_map(|b| dock_spot(s, b.pos, Some(&home_berths), FAR_DOCK_REACH).map(|p| (b.pos, p))) else {
                self.placed.push((s.time_days, BuildingKind::Dock, Err("no far camp on water joined to home".into())));
                return;
            };
            if self.home_dock.is_none() {
                self.home_dock = self.place_dock(s, Some(home_spot));
                if self.home_dock.is_none() {
                    return;
                }
            }
            self.note(s, format!("shipping from the camp at {camp:?}"));
            self.far_dock = self.place_dock(s, Some(spot));
            return;
        }
        let done = |id: Option<BuildingId>| id.and_then(|i| s.buildings.building(i)).is_some_and(|b| b.is_complete());
        let (Some(fd), Some(hd)) = (self.far_dock, self.home_dock) else { return };
        if !done(Some(fd)) || !done(Some(hd)) || !s.tech.allows(BuildingKind::Barge) {
            return;
        }
        if s.buildings.barges.is_empty() {
            if !self.has(BuildingKind::Barge) && !self.tried_lately(s, BuildingKind::Barge) {
                let berths = s.buildings.building(hd).map(|b| crate::shipping::berths_at(s, b.pos)).unwrap_or_default();
                let r = berths
                    .into_iter()
                    .find_map(|c| s.apply(Command::PlaceSite { kind: BuildingKind::Barge, pos: c }).ok())
                    .ok_or_else(|| "no berth takes a barge".to_string())
                    .and_then(|_| s.buildings.buildings.last().map(|b| b.id).ok_or_else(|| "placed nothing".to_string()));
                self.placed.push((s.time_days, BuildingKind::Barge, r.clone()));
                self.note(s, format!("barge: {r:?}"));
            }
            return;
        }
        if self.routed {
            return;
        }
        let barge = s.buildings.barges[0].id;
        match s.apply(Command::SetBargeRoute { barge, from: fd, to: hd, goods: vec![Good::Stone, Good::Mud] }) {
            Ok(()) => {
                self.routed = true;
                self.note(s, "barge route set: far dock to home dock".into());
            }
            Err(e) => {
                let e = e.to_string();
                self.note(s, format!("barge route refused: {e}"));
                let at = e.split("a dam at ").nth(1).and_then(|t| t.split(' ').next()).and_then(|t| {
                    let (x, y) = t.split_once(',')?;
                    Some((x.parse::<usize>().ok()?, y.parse::<usize>().ok()?))
                });
                if let Some((x, y)) = at {
                    if s.tech.allows(BuildingKind::Lock) && !self.tried_lately(s, BuildingKind::Lock) {
                        let r = [(x, y), (x.saturating_sub(1), y)]
                            .into_iter()
                            .find_map(|p| s.apply(Command::PlaceSite { kind: BuildingKind::Lock, pos: p }).ok().map(|_| p))
                            .ok_or_else(|| "no lock fits there".to_string())
                            .and_then(|_| s.buildings.buildings.last().map(|b| b.id).ok_or_else(|| "placed nothing".to_string()));
                        self.placed.push((s.time_days, BuildingKind::Lock, r.clone()));
                        self.note(s, format!("lock on the dam at {x},{y}: {r:?}"));
                    }
                }
            }
        }
    }
}

pub fn dock_spot(s: &SimState, near: Cell, reach: Option<&[Cell]>, radius: usize) -> Option<Cell> {
    let n = s.world.size;
    let mut cands: Vec<(usize, Cell)> = Vec::new();
    for y in near.1.saturating_sub(radius)..(near.1 + radius).min(n) {
        for x in near.0.saturating_sub(radius)..(near.0 + radius).min(n) {
            if s.check_site(BuildingKind::Dock, (x, y)).is_ok() && !crate::shipping::berths_at(s, (x, y)).is_empty() {
                cands.push((x.abs_diff(near.0) + y.abs_diff(near.1), (x, y)));
            }
        }
    }
    cands.sort();
    cands.into_iter().map(|c| c.1).find(|&p| reach.is_none_or(|r| crate::shipping::joined(s, &crate::shipping::berths_at(s, p), r)))
}

pub fn place_deposit(s: &mut SimState, kind: BuildingKind, around: Cell, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    let n = s.world.size;
    let r = DEPOSIT_REACH;
    let mut cands: Vec<(usize, usize, Cell)> = Vec::new();
    for y in (around.1.saturating_sub(r)..(around.1 + r).min(n)).step_by(2) {
        for x in (around.0.saturating_sub(r)..(around.0 + r).min(n)).step_by(2) {
            if !s.fog.explored((x, y)) || s.check_site(kind, (x, y)).is_err() {
                continue;
            }
            if let Some((_, k)) = area_yield(s, kind, (x, y)) {
                if k >= 3 {
                    cands.push((k, x.abs_diff(around.0) + y.abs_diff(around.1), (x, y)));
                }
            }
        }
    }
    cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, _, c) in cands.into_iter().take(12) {
        if let Ok(id) = place_near_within(s, kind, c, 1, keep_clear) {
            return Ok(id);
        }
    }
    Err("no deposit in sight".into())
}

pub fn area_yield(s: &SimState, kind: BuildingKind, pos: Cell) -> Option<(crate::resources::ResourceKind, usize)> {
    let role = crate::catalogue::def(kind).slots.iter().find_map(|r| if let crate::work::Role::Collect(k) = r { Some(*k) } else { None })?;
    let b = crate::buildings::Building { id: BuildingId(0), kind, pos, status: crate::buildings::Status::Complete, recipe: None, output: 0.0 };
    let area = crate::catalogue::work_area(&b)?;
    let n = s.resources.items.iter().filter(|r| r.kind == role && area.contains(r.cell)).count();
    Some((role, n))
}

pub fn place_best(s: &mut SimState, kind: BuildingKind, around: Cell, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    if area_yield(s, kind, around).is_none() {
        return place_ring(s, kind, around, 2..=29, keep_clear);
    }
    let mut cands: Vec<(usize, usize, Cell)> = Vec::new();
    for y in around.1.saturating_sub(20)..around.1 + 20 {
        for x in around.0.saturating_sub(20)..around.0 + 20 {
            if (x + y) % 2 == 1 || s.check_site(kind, (x, y)).is_err() {
                continue;
            }
            if let Some((_, n)) = area_yield(s, kind, (x, y)) {
                cands.push((n, x.abs_diff(around.0) + y.abs_diff(around.1), (x, y)));
            }
        }
    }
    cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, _, c) in cands.into_iter().take(12) {
        if let Ok(id) = place_near_within(s, kind, c, 1, keep_clear) {
            return Ok(id);
        }
    }
    place_ring(s, kind, around, 2..=29, keep_clear)
}

fn place_near_within(s: &mut SimState, kind: BuildingKind, around: Cell, max_r: usize, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 0..=max_r, keep_clear)
}

pub fn place_near(s: &mut SimState, kind: BuildingKind, around: Cell) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 2..=29, &[])
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
mod tests {
    use super::*;

    #[test]
    fn an_attentive_player_builds_the_camp_from_the_bundle() {
        let (mut s, dam) = crate::starts::new_game_with_dam(1).unwrap();
        s.apply(Command::Settle { cell: dam.camp }).unwrap();
        let mut p = Player::new(Style::Attentive, dam.camp, Some((dam.cells.clone(), dam.height)));
        while s.time_days < 60.0 {
            p.act(&mut s);
            for _ in 0..10 {
                s.step(0.1).unwrap();
            }
        }
        for k in [BuildingKind::Burrow, BuildingKind::ForagerHut, BuildingKind::Dam] {
            assert!(p.has(k), "{k:?} placed: {:?}", p.log);
        }
    }
}
