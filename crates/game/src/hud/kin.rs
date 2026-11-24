//! Family ties for the beaver and lodge pages: who is whose parent, partner, brother or aunt,
//! which litter each beaver came in (its mother's first, second...), and when. Works for the
//! living and for those only remembered (see `sim::lives`).
use sim::colony::{BeaverId, Sex};
use sim::lives::What;
use sim::SimState;

pub fn name_of(s: &SimState, id: BeaverId) -> String {
    s.colony.beaver(id).map(|b| b.name.clone()).or_else(|| s.lives.of.get(&id).map(|l| l.name.clone())).unwrap_or_else(|| "Unknown".into())
}

/// A beaver's parents, whether it is still here or only remembered.
pub fn parents(s: &SimState, id: BeaverId) -> [Option<BeaverId>; 2] {
    match s.colony.beaver(id) {
        Some(b) => [b.mother, b.father],
        None => s.lives.of.get(&id).map_or([None, None], |l| [l.mother, l.father]),
    }
}

pub fn sex_of(s: &SimState, id: BeaverId) -> Option<Sex> {
    s.colony.beaver(id).map(|b| b.sex).or_else(|| s.lives.of.get(&id).map(|l| l.sex))
}

pub fn partner_of(s: &SimState, id: BeaverId) -> Option<BeaverId> {
    s.colony.beaver(id).and_then(|b| b.mate)
}

/// Every beaver the colony has known, living or remembered.
pub fn everyone(s: &SimState) -> Vec<BeaverId> {
    s.colony.beavers.iter().map(|b| b.id).chain(s.lives.of.keys().copied()).collect::<std::collections::BTreeSet<_>>().into_iter().collect()
}

/// Its kits, oldest first.
pub fn children(s: &SimState, all: &[BeaverId], id: BeaverId) -> Vec<BeaverId> {
    let mut v: Vec<BeaverId> = all.iter().copied().filter(|&c| parents(s, c).contains(&Some(id))).collect();
    v.sort_by(|a, b| born(s, *a).unwrap_or(f64::MAX).total_cmp(&born(s, *b).unwrap_or(f64::MAX)).then(a.cmp(b)));
    v
}

/// The day it was born, if the colony saw it or can tell from its age.
pub fn born(s: &SimState, id: BeaverId) -> Option<f64> {
    let noted = s.lives.of.get(&id).and_then(|l| l.entries.iter().find(|e| matches!(e.what, What::Born { .. }))).map(|e| e.day);
    noted.or_else(|| s.colony.beaver(id).map(|b| s.time_days - b.age_days as f64))
}

/// A birth year in words: "Y3", or before the colony began.
pub fn year_word(y: i64) -> String {
    if y >= 1 { format!("Y{y}") } else { "before Y1".into() }
}

/// The colony year it was born in, from 1 (0 or less: before the colony).
pub fn born_year(s: &SimState, id: BeaverId) -> Option<i64> {
    born(s, id).map(|d| (d / sim::calendar::YEAR_DAYS).floor() as i64 + 1)
}

/// Its litter: its mother, the year, and which of her litters it was (1 for her first). A
/// mother has at most one litter a year, so the year tells litters apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Litter {
    pub mother: BeaverId,
    pub year: i64,
    pub number: usize,
}

pub fn litter(s: &SimState, all: &[BeaverId], id: BeaverId) -> Option<Litter> {
    let mother = parents(s, id)[0]?;
    let year = born_year(s, id)?;
    let mut years: Vec<i64> = all.iter().filter(|&&c| parents(s, c)[0] == Some(mother)).filter_map(|&c| born_year(s, c)).collect();
    years.sort();
    years.dedup();
    let number = years.iter().position(|&y| y == year)? + 1;
    Some(Litter { mother, year, number })
}

/// Who is in a litter, in the order they were noted.
pub fn litter_mates(s: &SimState, all: &[BeaverId], l: Litter) -> Vec<BeaverId> {
    all.iter().copied().filter(|&c| parents(s, c)[0] == Some(l.mother) && born_year(s, c) == Some(l.year)).collect()
}

/// The kind of tie, for filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KinFilter {
    #[default]
    All,
    Partner,
    Parents,
    Kits,
    Siblings,
    Grand,
    Other,
}

impl KinFilter {
    pub const ALL: [KinFilter; 7] = [KinFilter::All, KinFilter::Partner, KinFilter::Parents, KinFilter::Kits, KinFilter::Siblings, KinFilter::Grand, KinFilter::Other];
    pub fn label(self) -> &'static str {
        match self {
            KinFilter::All => "All",
            KinFilter::Partner => "Partner",
            KinFilter::Parents => "Parents",
            KinFilter::Kits => "Kits",
            KinFilter::Siblings => "Siblings",
            KinFilter::Grand => "Grand",
            KinFilter::Other => "Other",
        }
    }
    /// The filter a tie falls under, by its order from `kinship`.
    pub fn of(order: u8) -> KinFilter {
        match order {
            0 => KinFilter::Partner,
            1 => KinFilter::Parents,
            2 => KinFilter::Kits,
            3 => KinFilter::Siblings,
            4 | 5 => KinFilter::Grand,
            _ => KinFilter::Other,
        }
    }
    pub fn takes(self, order: u8) -> bool {
        self == KinFilter::All || KinFilter::of(order) == self
    }
}

/// How `other` is related to `me`, closest tie first, in words that follow its sex; with an
/// order to list relatives in (nearest first).
pub fn kinship(s: &SimState, me: BeaverId, other: BeaverId) -> Option<(u8, &'static str)> {
    if me == other {
        return None;
    }
    let male = sex_of(s, other) == Some(Sex::Male);
    let pick = |m: &'static str, f: &'static str| if male { m } else { f };
    let mine = parents(s, me);
    let theirs = parents(s, other);
    let parent_of = |p: BeaverId, c: BeaverId| parents(s, c).contains(&Some(p));
    let shared = mine.iter().flatten().filter(|p| theirs.contains(&Some(**p))).count();
    let siblings = |a: BeaverId, b: BeaverId| a != b && parents(s, a).iter().flatten().any(|p| parents(s, b).contains(&Some(*p)));
    if partner_of(s, me) == Some(other) {
        return Some((0, "Partner"));
    }
    if parent_of(other, me) {
        return Some((1, pick("Father", "Mother")));
    }
    if parent_of(me, other) {
        return Some((2, pick("Son", "Daughter")));
    }
    if shared == 2 {
        return Some((3, pick("Brother", "Sister")));
    }
    if shared == 1 {
        return Some((3, pick("Half-brother", "Half-sister")));
    }
    if mine.iter().flatten().any(|p| parent_of(other, *p)) {
        return Some((4, pick("Grandfather", "Grandmother")));
    }
    if theirs.iter().flatten().any(|p| parent_of(me, *p)) {
        return Some((5, pick("Grandson", "Granddaughter")));
    }
    if mine.iter().flatten().any(|p| siblings(*p, other)) {
        return Some((6, pick("Uncle", "Aunt")));
    }
    if theirs.iter().flatten().any(|p| siblings(*p, me)) {
        return Some((7, pick("Nephew", "Niece")));
    }
    if mine.iter().flatten().any(|p| theirs.iter().flatten().any(|q| siblings(*p, *q))) {
        return Some((8, "Cousin"));
    }
    if let Some(m) = partner_of(s, me) {
        if parent_of(other, m) {
            return Some((9, pick("Father-in-law", "Mother-in-law")));
        }
        if siblings(m, other) {
            return Some((9, pick("Brother-in-law", "Sister-in-law")));
        }
    }
    if partner_of(s, other).is_some_and(|p| parent_of(me, p)) {
        return Some((10, pick("Son-in-law", "Daughter-in-law")));
    }
    None
}
