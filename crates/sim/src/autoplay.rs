//! Scripted players for audits and screenshots. They act only through what a player can do:
//! `Command`s (the same ones the HUD sends) and the alerts' own fixes (`hud::apply_fix`), so a run
//! shows what the game allows, not what the code could do behind the player's back.
//!
//! `Player::act` is meant to be called once a sim day; it records what it did and what it saw.
use crate::buildings::{BuildingId, BuildingKind, Recipe};
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
    recipe_set: Vec<BuildingId>,
}

impl Player {
    pub fn new(style: Style, camp: Cell, dam: Option<(Vec<Cell>, f32)>) -> Player {
        Player { style, camp, dam, placed: Vec::new(), log: Vec::new(), recipe_set: Vec::new() }
    }

    fn note(&mut self, s: &SimState, line: String) {
        self.log.push(format!("day {:>5.1}: {line}", s.time_days));
    }

    fn has(&self, kind: BuildingKind) -> bool {
        self.placed.iter().any(|(_, k, r)| *k == kind && r.is_ok())
    }

    /// Places `kind` on the nearest cell to the camp the sim accepts and a beaver can reach.
    pub fn place(&mut self, s: &mut SimState, kind: BuildingKind) -> Option<BuildingId> {
        let res = place_near(s, kind, self.camp);
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
        // The camp: shelter, store and the first dam, from the bundle.
        if self.placed.is_empty() {
            self.place(s, BuildingKind::Burrow);
            self.place(s, BuildingKind::Warehouse);
            if let Some((cells, h)) = self.dam.clone() {
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
            }
        }
        // Producers as soon as research allows them.
        for kind in [BuildingKind::ForagerHut, BuildingKind::LoggingCamp, BuildingKind::MudPit, BuildingKind::Workshop, BuildingKind::QuarryCamp] {
            if s.tech.allows(kind) && !self.has(kind) && !self.placed.iter().any(|(d, k, _)| *k == kind && s.time_days - d < 10.0) {
                self.place(s, kind);
            }
        }
        // A second home once the family outgrows the burrow's comfort.
        if s.tech.allows(BuildingKind::Lodge) && s.colony.population() >= 6 && !self.has(BuildingKind::Lodge) {
            self.place(s, BuildingKind::Lodge);
        }
        // Finished workshops make rope (stores and the water wheel need it).
        let benches: Vec<BuildingId> = s
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Workshop && b.is_complete() && !self.recipe_set.contains(&b.id))
            .map(|b| b.id)
            .collect();
        for b in benches {
            let r = s.apply(Command::SetRecipe { building: b, recipe: Some(Recipe::Rope) });
            self.recipe_set.push(b);
            self.note(s, format!("workshop makes rope: {r:?}"));
        }
        if self.style == Style::IgnoresAlerts {
            return;
        }
        // The top alert's fix, as its button would do it.
        let Some(p) = hud::problems(s).into_iter().next() else { return };
        match p.fix {
            Fix::Place(kind) => {
                if !self.placed.iter().any(|(d, k, _)| *k == kind && s.time_days - d < 10.0) {
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

/// The nearest cell to `around` where `kind` can be placed and the first adult can walk beside.
pub fn place_near(s: &mut SimState, kind: BuildingKind, around: Cell) -> Result<BuildingId, String> {
    let walker = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult).map(|b| b.id);
    let from = walker.and_then(|w| s.agents.get(w)).map(|a| a.cell);
    let mut last = String::from("no cell tried");
    for r in 2..30usize {
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
        while s.time_days < 5.0 {
            p.act(&mut s);
            for _ in 0..10 {
                s.step(0.1).unwrap();
            }
        }
        for k in [BuildingKind::Burrow, BuildingKind::Warehouse, BuildingKind::Dam] {
            assert!(p.has(k), "{k:?} placed: {:?}", p.log);
        }
    }
}
