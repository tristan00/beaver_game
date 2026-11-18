//! Scripted players for audits and screenshots. They act only through what a player can do:
//! `Command`s (the same ones the HUD sends) and the alerts' own fixes (`hud::apply_fix`), so a run
//! shows what the game allows, not what the code could do behind the player's back.
//!
//! `Player::act` is meant to be called once a sim day; it records what it did and what it saw.
use crate::buildings::{BuildingId, BuildingKind, Good, Recipe};
use crate::resources::ResourceKind;
use crate::hud::{self, Fix};
use crate::water::Cell;
use crate::{Command, SimState};

/// How a scripted player plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Builds the camp (burrow, warehouse, dam) at once, adds producers as they unlock and acts
    /// on the top alert every day.
    Attentive,
    /// Builds like `Attentive` but never touches an alert.
    IgnoresAlerts,
    /// Only settles; the colony is left to auto-staffing.
    HandsOff,
}

/// A scripted player and what it did.
pub struct Player {
    pub style: Style,
    pub camp: Cell,
    pub dam: Option<(Vec<Cell>, f32)>,
    /// What it placed, with the day.
    pub placed: Vec<(f64, BuildingKind, Result<BuildingId, String>)>,
    /// One line per notable action or refusal.
    pub log: Vec<String>,
    recipe: Option<(BuildingId, Recipe)>,
    next_explore: f64,
    /// Water haulage: a dock at the far deposit, a dock at home, a barge between them.
    far_dock: Option<BuildingId>,
    home_dock: Option<BuildingId>,
    routed: bool,
}

/// A deposit further than this (cells) from the home store is shipped home by barge.
pub const FAR_DEPOSIT: usize = 16;
/// Gathering camps for concentrated deposits look this far from the camp (explored ground only).
pub const DEPOSIT_REACH: usize = 45;

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

    /// Tried in the last ten days, or a site of that kind is still being built: a careful player
    /// finishes one before starting another.
    fn tried_lately(&self, s: &SimState, kind: BuildingKind) -> bool {
        self.placed.iter().any(|(d, k, _)| *k == kind && s.time_days - d < 10.0)
            || s.buildings.buildings.iter().any(|b| b.kind == kind && !b.is_complete())
            || !self.has_hands(s)
    }

    /// Whether the colony has hands for another site: one open site per two working beavers.
    fn has_hands(&self, s: &SimState) -> bool {
        let workers = s.colony.beavers.iter().filter(|b| b.stage != crate::colony::Stage::Kit).count();
        let open = s.buildings.buildings.iter().filter(|b| !b.is_complete()).count();
        open < (workers / 2).max(1)
    }

    /// Places `kind` on the nearest cell to the camp the sim accepts and a beaver can reach.
    pub fn place(&mut self, s: &mut SimState, kind: BuildingKind) -> Option<BuildingId> {
        let around = self.camp;
        self.place_at(s, kind, around)
    }

    /// Places `kind` as near `around` as the sim allows (gathering camps where their area is richest).
    pub fn place_at(&mut self, s: &mut SimState, kind: BuildingKind, around: Cell) -> Option<BuildingId> {
        let res = if matches!(kind, BuildingKind::QuarryCamp | BuildingKind::MudPit) { place_deposit(s, kind, around) } else { place_best(s, kind, around) };
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

    /// One day's decisions.
    pub fn act(&mut self, s: &mut SimState) {
        if self.style == Style::HandsOff || !s.colony.is_founded() {
            return;
        }
        // The camp, from the bundle: shelter, food, the first dam, then the trades and a store
        // (one site per two working beavers).
        if self.placed.is_empty() {
            self.place(s, BuildingKind::Burrow);
        }
        // Food first: a forager hut as soon as the burrow opens foraging, and one per five beavers
        // after that (food never waits for free hands).
        let huts = s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::ForagerHut).count();
        let hut_building = s.buildings.buildings.iter().any(|b| b.kind == BuildingKind::ForagerHut && !b.is_complete());
        let recent = self.placed.iter().any(|(d, k, _)| *k == BuildingKind::ForagerHut && s.time_days - d < 10.0);
        if s.tech.allows(BuildingKind::ForagerHut) && huts < 1 + s.colony.population() / 5 && !hut_building && !recent {
            self.place(s, BuildingKind::ForagerHut);
        }
        // The first dam: the designed one, or the best the family can see from its camp.
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
        // Producers as soon as research allows them.
        for kind in [BuildingKind::LoggingCamp, BuildingKind::Warehouse, BuildingKind::MudPit, BuildingKind::Workshop, BuildingKind::QuarryCamp] {
            if s.tech.allows(kind) && s.missing_prerequisites(kind).is_empty() && !self.has(kind) && !self.tried_lately(s, kind) {
                self.place(s, kind);
            }
        }
        self.explore(s);
        self.ship(s);
        // Homes and wood keep pace with the colony: a lodge per eight beavers, a logging camp per ten.
        let pop = s.colony.population();
        let count = |s: &SimState, k: BuildingKind| s.buildings.buildings.iter().filter(|b| b.kind == k).count();
        if s.tech.allows(BuildingKind::Lodge) && pop >= 6 && count(s, BuildingKind::Lodge) < pop.div_ceil(8) && !self.tried_lately(s, BuildingKind::Lodge) {
            self.place(s, BuildingKind::Lodge);
        }
        if s.tech.allows(BuildingKind::LoggingCamp) && count(s, BuildingKind::LoggingCamp) < 1 + pop / 10 && !self.tried_lately(s, BuildingKind::LoggingCamp) {
            self.place(s, BuildingKind::LoggingCamp);
        }
        // The workshop makes what a site waits for (baskets for a barge, tools for a lock),
        // else rope (stores and the water wheel need it).
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
        // The top alert's fix, as its button would do it.
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
    /// Sends a free adult to look over unexplored ground when no stone or clay is in sight.
    fn explore(&mut self, s: &mut SimState) {
        if s.time_days < self.next_explore || s.colony.population() < 4 {
            return;
        }
        self.next_explore = s.time_days + 12.0;
        let seen = |k: ResourceKind| s.resources.items.iter().any(|r| r.kind == k && r.amount >= 20.0 && s.fog.explored(r.cell));
        if seen(ResourceKind::StoneOutcrop) && seen(ResourceKind::MudBank) {
            return;
        }
        // The nearest unexplored dry cell in one of eight directions, 20 to 40 cells out.
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

    /// Ships a far deposit home: a dock by the quarry or clay pit, a dock by the home store, a barge
    /// running between them (and a lock on any dam in the way).
    fn ship(&mut self, s: &mut SimState) {
        let home = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::Warehouse && b.is_complete()).cloned();
        let Some(home) = home else { return };
        let far = s
            .buildings
            .buildings
            .iter()
            .filter(|b| matches!(b.kind, BuildingKind::QuarryCamp | BuildingKind::MudPit) && b.is_complete())
            .filter(|b| crate::stock::gap(b, &home) > FAR_DEPOSIT)
            .max_by_key(|b| crate::stock::gap(b, &home))
            .cloned();
        let Some(far) = far else { return };
        if !s.tech.allows(BuildingKind::Dock) {
            return;
        }
        if self.home_dock.is_none() && !self.tried_lately(s, BuildingKind::Dock) {
            self.home_dock = self.place_at(s, BuildingKind::Dock, home.pos);
        }
        if self.far_dock.is_none() && self.home_dock.is_some() && !self.placed.iter().any(|(d, k, _)| *k == BuildingKind::Dock && s.time_days - d < 1.0) {
            self.far_dock = self.place_at(s, BuildingKind::Dock, far.pos);
        }
        let done = |id: Option<BuildingId>| id.and_then(|i| s.buildings.building(i)).is_some_and(|b| b.is_complete());
        let (Some(fd), Some(hd)) = (self.far_dock, self.home_dock) else { return };
        if !done(Some(fd)) || !done(Some(hd)) || !s.tech.allows(BuildingKind::Barge) {
            return;
        }
        if s.buildings.barges.is_empty() {
            if !self.has(BuildingKind::Barge) && !self.tried_lately(s, BuildingKind::Barge) {
                let at = s.buildings.building(hd).map(|b| b.pos).unwrap_or(home.pos);
                let r = place_ring(s, BuildingKind::Barge, at, 0..=3);
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
                // "a dam at x,y is in the way": build a lock on it.
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

/// Places a quarry or clay pit at the richest deposit in sight: explored cells within
/// `DEPOSIT_REACH` of `around` whose work area holds the most, nearer winning ties.
pub fn place_deposit(s: &mut SimState, kind: BuildingKind, around: Cell) -> Result<BuildingId, String> {
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
        if let Ok(id) = place_near_within(s, kind, c, 1) {
            return Ok(id);
        }
    }
    Err("no deposit in sight".into())
}

/// What a gathering camp at `pos` would find in its work area (`None` for other buildings).
pub fn area_yield(s: &SimState, kind: BuildingKind, pos: Cell) -> Option<(crate::resources::ResourceKind, usize)> {
    let role = crate::catalogue::def(kind).slots.iter().find_map(|r| if let crate::work::Role::Collect(k) = r { Some(*k) } else { None })?;
    let b = crate::buildings::Building { id: BuildingId(0), kind, pos, status: crate::buildings::Status::Complete, recipe: None, output: 0.0 };
    let area = crate::catalogue::work_area(&b)?;
    let n = s.resources.items.iter().filter(|r| r.kind == role && area.contains(r.cell)).count();
    Some((role, n))
}

/// Places a gathering camp where its work area holds the most of what it gathers (within 20 cells
/// of `around`, nearer wins ties); other buildings go to the nearest suitable cell.
pub fn place_best(s: &mut SimState, kind: BuildingKind, around: Cell) -> Result<BuildingId, String> {
    if area_yield(s, kind, around).is_none() {
        return place_near(s, kind, around);
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
        if let Ok(id) = place_near_within(s, kind, c, 1) {
            return Ok(id);
        }
    }
    place_near(s, kind, around)
}

fn place_near_within(s: &mut SimState, kind: BuildingKind, around: Cell, max_r: usize) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 0..=max_r)
}

/// The nearest cell to `around` where `kind` can be placed and the first adult can walk beside.
pub fn place_near(s: &mut SimState, kind: BuildingKind, around: Cell) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 2..=29)
}

fn place_ring(s: &mut SimState, kind: BuildingKind, around: Cell, radii: std::ops::RangeInclusive<usize>) -> Result<BuildingId, String> {
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
                if let (Some(w), Some(f)) = (walker, from) {
                    if s.nav.find_path(w, f, crate::nav::Goal::Beside((x, y))).is_none() {
                        last = "unreachable".into();
                        continue;
                    }
                }
                s.apply(Command::PlaceSite { kind, pos: (x, y) }).map_err(|e| e.to_string())?;
                let id = s.buildings.buildings.last().map(|b| b.id).ok_or("placed nothing")?;
                // A careful player does not wall the family in: undo a site that cuts the first
                // adult off from the supply pile.
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
