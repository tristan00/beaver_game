use std::collections::BTreeMap;

use crate::buildings::{Amount, BuildingId, Good};
use crate::water::Cell;
use crate::SimState;

const EPS: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Spot {
    pub building: BuildingId,
    pub cell: Cell,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Supplies {
    pub at: BTreeMap<Spot, Vec<Amount>>,
    pub labour: BTreeMap<Spot, f32>,
}

pub fn labour(sim: &SimState, s: Spot) -> f32 {
    sim.supplies.labour.get(&s).copied().unwrap_or(0.0)
}

pub fn set_labour(sim: &mut SimState, s: Spot, amount: f32) {
    if amount > EPS {
        sim.supplies.labour.insert(s, amount);
    } else {
        sim.supplies.labour.remove(&s);
    }
}

pub fn mend_spot(sim: &SimState, building: BuildingId) -> Option<Spot> {
    sim.buildings.building(building).map(|b| Spot { building, cell: b.pos })
}

fn is_gate_spot(sim: &SimState, s: Spot) -> bool {
    sim.buildings.building(s.building).is_some_and(|b| b.dam.is_some() || b.canal.is_some())
}

fn gate_at(sim: &SimState, s: Spot) -> Option<(bool, bool)> {
    let b = sim.buildings.building(s.building)?;
    let g = b.gates().iter().find(|g| g.cell == s.cell)?;
    let wanted = b.canal.is_none() || g.pending.is_some();
    Some((g.built, wanted))
}

fn add(out: &mut Vec<Amount>, good: Good, amount: f32) {
    match out.iter_mut().find(|a| a.0 == good) {
        Some(a) => a.1 += amount,
        None => out.push((good, amount)),
    }
}

pub fn needs(sim: &SimState, s: Spot) -> Vec<Amount> {
    let mut out = Vec::new();
    if is_gate_spot(sim, s) {
        if let Some((built, wanted)) = gate_at(sim, s) {
            if !built && wanted {
                for &(g, a) in &crate::content::content().works.gate {
                    add(&mut out, g, a);
                }
            }
            if crate::locks::pending(sim, s.cell) {
                for &(g, a) in &crate::content::content().works.lock {
                    add(&mut out, g, a);
                }
            }
        }
    } else if let Some(b) = sim.buildings.building(s.building).filter(|b| b.pos == s.cell && crate::maintenance::needs_mending(sim, b)) {
        let (rates, _) = crate::maintenance::mend_rates(b);
        let left = (1.0 - b.condition).max(0.0);
        for (g, a) in rates {
            add(&mut out, g, a * left);
        }
    }
    out
}

pub fn delivered(sim: &SimState, s: Spot, good: Good) -> f32 {
    sim.supplies.at.get(&s).and_then(|v| v.iter().find(|a| a.0 == good)).map_or(0.0, |a| a.1)
}

pub fn missing(sim: &SimState, s: Spot, good: Good) -> f32 {
    needs(sim, s).iter().find(|a| a.0 == good).map_or(0.0, |a| (a.1 - delivered(sim, s, good)).max(0.0))
}

pub fn has(sim: &SimState, s: Spot, amounts: &[Amount]) -> bool {
    amounts.iter().all(|&(g, a)| delivered(sim, s, g) + EPS >= a)
}

pub fn deposit(sim: &mut SimState, s: Spot, good: Good, amount: f32) -> f32 {
    let put = amount.min(missing(sim, s, good)).max(0.0);
    if put > 0.0 {
        let v = sim.supplies.at.entry(s).or_default();
        add(v, good, put);
    }
    put
}

pub fn take(sim: &mut SimState, s: Spot, amounts: &[Amount]) -> bool {
    if !has(sim, s, amounts) {
        return false;
    }
    if let Some(v) = sim.supplies.at.get_mut(&s) {
        for &(g, a) in amounts {
            if let Some(x) = v.iter_mut().find(|x| x.0 == g) {
                x.1 = (x.1 - a).max(0.0);
            }
        }
        v.retain(|x| x.1 > EPS);
        if v.is_empty() {
            sim.supplies.at.remove(&s);
        }
    }
    true
}

pub fn tidy(sim: &mut SimState) {
    let worked: Vec<Spot> = sim.supplies.labour.keys().copied().collect();
    for s in worked {
        let wanted = crate::gates::gate_of(sim, s.building, s.cell).is_some_and(|g| crate::gates::piece(sim, s.building, &g).is_some());
        if !wanted {
            sim.supplies.labour.remove(&s);
        }
    }
    let spots: Vec<Spot> = sim.supplies.at.keys().copied().collect();
    for s in spots {
        let need = needs(sim, s);
        let Some(v) = sim.supplies.at.get(&s).cloned() else { continue };
        let mut kept = Vec::new();
        for (g, a) in v {
            let keep = need.iter().find(|n| n.0 == g).map_or(0.0, |n| n.1).min(a);
            let back = a - keep;
            if back > EPS {
                sim.refund_or_drop(s.cell, g, back);
            }
            if keep > EPS {
                kept.push((g, keep));
            }
        }
        if kept.is_empty() {
            sim.supplies.at.remove(&s);
        } else {
            sim.supplies.at.insert(s, kept);
        }
    }
}
