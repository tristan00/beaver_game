//! Each beaver's life in the colony, kept for the player to read back: how it came (born,
//! with its parents, or joined as a newcomer), each job it took, each partner, each home,
//! growing up, and how it ended. Worked out by looking at the colony a few times a day and
//! noting what changed since the last look, so nothing else has to report to it. Kept for the
//! dead too, so family links still lead somewhere. Saved after the world (see `save`).
use std::collections::BTreeMap;

use crate::colony::{BeaverId, DeathCause, LodgeId, Sex, Stage};
use crate::jobs::RoleKind;
use crate::SimState;

/// Days between looks.
const LOOK_DAYS: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum What {
    /// Already in the colony when its records began (a founder, or a game from before them).
    Known,
    Born { mother: Option<BeaverId>, father: Option<BeaverId> },
    /// Came in as a newcomer.
    Joined,
    Grew(Stage),
    /// Took up a job; `None` is left free.
    Job(Option<RoleKind>),
    /// Paired with a partner; `None` is the partner lost.
    Partner(Option<BeaverId>),
    Moved(Option<LodgeId>),
    Died(DeathCause),
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub day: f64,
    pub what: What,
}

/// What was true of a beaver at the last look.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Seen {
    job: Option<RoleKind>,
    lodge: Option<LodgeId>,
    partner: Option<BeaverId>,
    stage: Stage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Life {
    pub name: String,
    /// Kept so family ties can still be named once it is gone.
    pub sex: Sex,
    pub mother: Option<BeaverId>,
    pub father: Option<BeaverId>,
    pub entries: Vec<Entry>,
    /// Still in the colony (false once dead or gone).
    pub here: bool,
    seen: Seen,
}

impl Life {
    /// When it came to the colony (born or joined), if the records saw it.
    pub fn arrived(&self) -> Option<f64> {
        self.entries.iter().find(|e| matches!(e.what, What::Born { .. } | What::Joined)).map(|e| e.day)
    }
    /// The day it took up with its current partner.
    pub fn partnered_since(&self) -> Option<f64> {
        self.entries.iter().rev().find(|e| matches!(e.what, What::Partner(Some(_)))).map(|e| e.day)
    }
    /// The day it moved into its current home.
    pub fn home_since(&self) -> Option<f64> {
        self.entries.iter().rev().find(|e| matches!(e.what, What::Moved(_) | What::Born { .. } | What::Joined | What::Known)).map(|e| e.day)
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Lives {
    pub of: BTreeMap<BeaverId, Life>,
    next_look: f64,
}

/// The job a beaver holds: its seat's role, or the role of the order it is on.
pub fn job_of(sim: &SimState, id: BeaverId) -> Option<RoleKind> {
    if let Some((_, _, role)) = sim.work.slot_of(id) {
        return Some(crate::jobs::role_kind(role));
    }
    sim.agents.get(id).and_then(|a| a.assignment).and_then(|a| crate::jobs::assignment_role(sim, a))
}

pub fn step(sim: &mut SimState) {
    if !sim.colony.is_founded() || sim.time_days < sim.lives.next_look {
        return;
    }
    let now = sim.time_days;
    let first = sim.lives.of.is_empty();
    sim.lives.next_look = now + LOOK_DAYS;
    let mut lives = std::mem::take(&mut sim.lives);
    for b in &sim.colony.beavers {
        let seen = Seen { job: job_of(sim, b.id), lodge: b.lodge, partner: b.mate, stage: b.stage };
        let Some(life) = lives.of.get_mut(&b.id) else {
            // A new face: born here if young enough to have been born since the last look,
            // else a newcomer, or (at the very first look) already here.
            let what = if first {
                What::Known
            } else if b.mother.is_some() && (b.age_days as f64) <= LOOK_DAYS + 1.0 {
                What::Born { mother: b.mother, father: b.father }
            } else {
                What::Joined
            };
            let mut entries = vec![Entry { day: now, what }];
            if seen.job.is_some() {
                entries.push(Entry { day: now, what: What::Job(seen.job) });
            }
            if seen.partner.is_some() {
                entries.push(Entry { day: now, what: What::Partner(seen.partner) });
            }
            lives.of.insert(b.id, Life { name: b.name.clone(), sex: b.sex, mother: b.mother, father: b.father, entries, here: true, seen });
            continue;
        };
        let was = life.seen;
        if seen.stage != was.stage {
            life.entries.push(Entry { day: now, what: What::Grew(seen.stage) });
        }
        if seen.job != was.job {
            life.entries.push(Entry { day: now, what: What::Job(seen.job) });
        }
        if seen.partner != was.partner {
            life.entries.push(Entry { day: now, what: What::Partner(seen.partner) });
        }
        if seen.lodge != was.lodge {
            life.entries.push(Entry { day: now, what: What::Moved(seen.lodge) });
        }
        life.seen = seen;
        life.name.clone_from(&b.name);
    }
    // Those no longer here: dead (a body lies somewhere), or gone off.
    for (id, life) in lives.of.iter_mut().filter(|(_, l)| l.here) {
        if sim.colony.beaver(*id).is_some() {
            continue;
        }
        life.here = false;
        let body = sim.agents.remains.bodies.iter().find(|b| b.id == *id).map(|b| b.cause);
        life.entries.push(Entry { day: now, what: body.map_or(What::Left, What::Died) });
    }
    sim.lives = lives;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_life_records_its_start_its_jobs_and_its_partner() {
        let (mut s, site) = crate::starts::new_game_on_map("voyageurs").unwrap();
        s.apply(crate::Command::Settle { cell: site.camp }).unwrap();
        crate::sandbox::run_days(&mut s, 1.0);
        let id = s.colony.beavers.iter().find(|b| b.can_work()).unwrap().id;
        let life = s.lives.of.get(&id).expect("every beaver has a life");
        assert!(matches!(life.entries[0].what, What::Known), "{:?}", life.entries);
        crate::jobs::adjust_role(&mut s, RoleKind::Forager, 1).unwrap();
        crate::sandbox::run_days(&mut s, 1.0);
        let who = s.colony.beavers.iter().find(|b| job_of(&s, b.id) == Some(RoleKind::Forager)).unwrap().id;
        assert!(s.lives.of[&who].entries.iter().any(|e| e.what == What::Job(Some(RoleKind::Forager))), "{:?}", s.lives.of[&who].entries);
        let paired = s.colony.beavers.iter().find(|b| b.mate.is_some()).map(|b| (b.id, b.mate));
        if let Some((a, mate)) = paired {
            assert!(s.lives.of[&a].entries.iter().any(|e| e.what == What::Partner(mate)));
        }
    }
}
