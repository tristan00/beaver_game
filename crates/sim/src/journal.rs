//! A very verbose record of the sim for play sessions, written to the session trace (see
//! `trace`): every colony event with its context, every task change, wolf moves, building
//! changes, and regular polls of every beaver and of the whole map. Nothing is written, and
//! nothing is computed, unless a session trace is open.
use crate::agents::{Assignment, Task};
use crate::buildings::{BuildingId, Good};
use crate::colony::{Beaver, BeaverId, Event, Job};
use crate::jobs::RoleKind;
use crate::work::Role;
use crate::resources::ResourceKind;
use crate::{trace, SimState};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// How often every beaver is written out, in days: every game hour.
const BEAVER_POLL_DAYS: f64 = 1.0 / 24.0;
/// How often the map state is written out, in days.
const MAP_POLL_DAYS: f64 = 0.25;

/// What a task line is made of: the task's variant, whether indoors, and the good carried. The
/// line is written out only when this changes, so most steps compare a few words, not strings.
type TaskSig = (std::mem::Discriminant<Task>, bool, Option<Good>);
/// What a job line is made of.
type JobSig = (Option<Job>, Option<Job>, Option<(BuildingId, usize, Role)>, Option<Assignment>, Option<RoleKind>);

#[derive(Clone, Debug, Default)]
pub struct Journal {
    started: bool,
    /// Where this step's colony events start: the log's length and drop count when the step
    /// began. The interface drains the log between steps, so only counts within a step hold.
    seen: usize,
    dropped: u64,
    buildings: BTreeMap<BuildingId, String>,
    tasks: HashMap<BeaverId, (TaskSig, String)>,
    jobs: HashMap<BeaverId, (JobSig, String)>,
    alerts: String,
    wolf: String,
    next_beavers: f64,
    next_map: f64,
    /// Reused for each building's line, so an unchanged building allocates nothing.
    line: String,
}

/// Everything a beaver's record holds that matters for why it lives or dies.
pub fn describe(b: &Beaver) -> String {
    let mut out = String::new();
    describe_into(&mut out, b);
    out
}

fn describe_into(out: &mut String, b: &Beaver) {
    let n = &b.needs;
    let h = b.harm();
    let _ = write!(out,
        "id={} name={:?} sex={:?} age_days={:.1} stage={:?} health={:.3} harm[starve={:.3} cold={:.3} injury={:.3} exhaust={:.3} old={:.3}] needs[food={:.3} rest={:.3} warmth={:.3} safety={:.3} social={:.3}] mood={:.3} low_mood_days={:.1} resting={} job={:?} current_job={:?} lodge={:?} family={:?} mate={:?} pregnant={} pos=({:.1},{:.1})",
        b.id.0, b.name, b.sex, b.age_days, b.stage, b.health, h[0], h[1], h[2], h[3], h[4],
        n.food, n.rest, n.warmth, n.safety, n.social, b.mood, b.low_mood_days(), b.resting, b.job, b.current_job,
        b.lodge.map(|l| l.0), b.family.0, b.mate.map(|m| m.0), b.pregnancy.is_some(), b.pos.0, b.pos.1
    );
}

/// The first word of a debug print: the enum variant, without its fields.
fn variant(s: &str) -> &str {
    s.split(|c: char| c == ' ' || c == '{' || c == '(').next().unwrap_or(s)
}

fn agent_context(sim: &SimState, id: BeaverId) -> String {
    let mut out = String::new();
    agent_context_into(&mut out, sim, id);
    out
}

fn agent_context_into(out: &mut String, sim: &SimState, id: BeaverId) {
    let Some(a) = sim.agents.get(id) else { out.push_str("agent=none"); return };
    let surface = sim.nav.surface_at(a.cell);
    let depth = sim.water.depth.get(a.cell.0, a.cell.1).copied().unwrap_or(0.0);
    let _ = write!(out,
        "cell=({},{}) surface={:?} depth={:.2} inside={} gait={:?} task={:?} assignment={:?} load={:?} stuck_ticks={} moving={} wolf_dist=",
        a.cell.0, a.cell.1, surface, depth, a.inside, a.gait, a.task, a.assignment,
        a.load.map(|l| (l.good, (l.amount * 100.0).round() / 100.0)), a.stuck_ticks,
        a.path.as_ref().is_some_and(|p| !p.steps.is_empty())
    );
    match &sim.predators.wolf {
        Some(w) => {
            let d = ((w.pos.0 - a.cell.0 as f32 - 0.5).powi(2) + (w.pos.1 - a.cell.1 as f32 - 0.5).powi(2)).sqrt();
            let _ = write!(out, "{d:.1}");
        }
        None => out.push_str("none"),
    }
}

fn stamp(sim: &SimState) -> String {
    format!("day={:.4} step={}", sim.time_days, sim.steps)
}

/// Write what changed since the last call. `phase` names the part of the step just done.
pub fn scan(sim: &mut SimState, phase: &str) {
    if !trace::enabled() {
        return;
    }
    let mut j = std::mem::take(&mut sim.journal);
    if !j.started {
        j.started = true;
        trace::event(format!(
            "SIM_START {} seed={} instance={} size={} founded={} population={}",
            stamp(sim), sim.seed, sim.instance, sim.world.size, sim.colony.is_founded(), sim.colony.population()
        ));
    }
    events(sim, &mut j, phase);
    tasks(sim, &mut j);
    jobs(sim, &mut j);
    wolf(sim, &mut j);
    buildings(sim, &mut j);
    sim.journal = j;
}

/// Events added between steps (by commands) are written at the next scan; if the interface
/// drained the log since, start again from what is left.
pub fn begin_step(sim: &mut SimState) {
    if !trace::enabled() {
        return;
    }
    let j = &mut sim.journal;
    let pushed_out = (sim.colony.events_dropped - j.dropped) as usize;
    if sim.colony.events.len() + pushed_out < j.seen {
        j.seen = sim.colony.events.len();
        j.dropped = sim.colony.events_dropped;
    }
}

fn events(sim: &SimState, j: &mut Journal, phase: &str) {
    let len = sim.colony.events.len();
    let pushed_out = (sim.colony.events_dropped - j.dropped) as usize;
    for i in j.seen.saturating_sub(pushed_out)..len {
        let Some(e) = sim.colony.events.get(i) else { continue };
        let who = match &e.event {
            Event::Born { id, .. } | Event::Matured { id, .. } | Event::Died { id, .. } | Event::Departed { id } | Event::Immigrated { id } | Event::LeftHome { id, .. } => Some(*id),
            _ => None,
        };
        let mut line = format!("EVENT {} at_day={:.4} phase={phase} {:?}", stamp(sim), e.day, e.event);
        if let Some(id) = who {
            match sim.colony.beaver(id) {
                Some(b) => { let _ = write!(line, " | {}", describe(b)); }
                None => line.push_str(" | beaver=gone"),
            }
            let _ = write!(line, " | {}", agent_context(sim, id));
        }
        if matches!(e.event, Event::Died { .. }) {
            let _ = write!(line, " | {}", colony_brief(sim));
        }
        trace::event(line);
    }
    j.seen = len;
    j.dropped = sim.colony.events_dropped;
}

fn tasks(sim: &SimState, j: &mut Journal) {
    for (id, a) in &sim.agents.by_id {
        let sig = (std::mem::discriminant(&a.task), a.inside, a.load.map(|l| l.good));
        if j.tasks.get(id).is_some_and(|(was, _)| *was == sig) {
            continue;
        }
        let now = format!("{} inside={} load={:?}", variant(&format!("{:?}", a.task)), a.inside, a.load.map(|l| l.good));
        let before = j.tasks.insert(*id, (sig, now.clone())).map(|(_, line)| line);
        if before.as_ref() != Some(&now) {
            let before = before.unwrap_or_else(|| "new".into());
            trace::event(format!("TASK {} beaver={} {} -> {} | {}", stamp(sim), id.0, before, now, agent_context(sim, *id)));
        }
    }
    // Every agent is in the record by now, so it holds more only when some have gone.
    if j.tasks.len() > sim.agents.by_id.len() {
        let gone: Vec<BeaverId> = j.tasks.keys().filter(|id| !sim.agents.by_id.contains_key(id)).copied().collect();
        for id in gone {
            j.tasks.remove(&id);
            trace::event(format!("AGENT_GONE {} beaver={}", stamp(sim), id.0));
        }
    }
}

/// The job a beaver holds: its colony job, the work slot it fills, and any standing order.
fn jobs(sim: &SimState, j: &mut Journal) {
    for b in &sim.colony.beavers {
        let slot = sim.work.slot_of(b.id);
        let order = sim.agents.get(b.id).and_then(|a| a.assignment);
        let role = crate::jobs::role_of(sim, b.id);
        let sig = (b.job, b.current_job, slot, order, role);
        if j.jobs.get(&b.id).is_some_and(|(was, _)| *was == sig) {
            continue;
        }
        let slot = slot.map(|(building, i, role)| format!("{}#{i}:{role:?}", building.0));
        let now = format!("job={:?} current_job={:?} slot={slot:?} order={order:?} role={role:?}", b.job, b.current_job);
        let before = j.jobs.insert(b.id, (sig, now.clone())).map(|(_, line)| line);
        if before.as_ref() != Some(&now) {
            let before = before.unwrap_or_else(|| "new".into());
            trace::event(format!("JOB {} beaver={} {before} -> {now}", stamp(sim), b.id.0));
        }
    }
    // Every beaver is in the record by now, so it holds more only when some have gone.
    if j.jobs.len() > sim.colony.beavers.len() {
        j.jobs.retain(|id, _| sim.colony.beaver(*id).is_some());
    }
}

fn wolf(sim: &SimState, j: &mut Journal) {
    if let Some(w) = &sim.predators.wolf {
        // Every step it is on the map: where, doing what, and the beavers it could see.
        let near = sim.agents.by_id.values().filter(|a| !a.inside)
            .map(|a| (((a.cell.0 as f32 + 0.5 - w.pos.0).powi(2) + (a.cell.1 as f32 + 0.5 - w.pos.1).powi(2)).sqrt(), a))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let near = near.map_or("none".to_string(), |(d, a)| format!("beaver={} dist={d:.1} surface={:?}", a.id.0, sim.nav.surface_at(a.cell)));
        trace::event(format!("WOLF_STEP {} state={:?} pos=({:.2},{:.2}) heading={:.2} pace={:.1} route_left={} rest_until={:.2} leave_by={:.2} nearest_outside={near}",
            stamp(sim), w.state, w.pos.0, w.pos.1, w.heading, w.pace, w.route_left(), w.rest_until(), w.leave_by));
    }
    let now = match &sim.predators.wolf {
        Some(w) => format!("{:?}", w.state),
        None => "absent".into(),
    };
    if j.wolf != now {
        let detail = sim.predators.wolf.as_ref().map_or(String::new(), |w| {
            format!(" pos=({:.1},{:.1}) pace={:.1} arrived={:.2} leave_by={:.2}", w.pos.0, w.pos.1, w.pace, w.arrived, w.leave_by)
        });
        trace::event(format!("WOLF {} {} -> {}{}", stamp(sim), if j.wolf.is_empty() { "start" } else { &j.wolf }, now, detail));
        j.wolf = now;
    }
}

fn building_line(sim: &SimState, b: &crate::buildings::Building) -> String {
    let mut out = String::new();
    building_line_into(&mut out, sim, b);
    out
}

fn building_line_into(out: &mut String, sim: &SimState, b: &crate::buildings::Building) {
    let stranded = sim.buildings.stranded(b, &sim.water.depth);
    let _ = write!(out,
        "{:?} pos=({},{}) status={:?} condition={:.1} reinforced={} stranded={}",
        b.kind, b.pos.0, b.pos.1, b.status, b.condition, b.reinforced, stranded
    );
    if let Some(d) = &b.dam {
        let (mut sum, mut low, mut n) = (0.0f32, f32::MAX, 0);
        for c in &d.cells {
            let h = sim.water.dam_height.get(c.cell.0, c.cell.1).copied().unwrap_or(0.0);
            let frac = if c.target > 0.0 { h / c.target } else { 1.0 };
            sum += frac;
            low = low.min(frac);
            n += 1;
        }
        let _ = write!(out, " dam_cells={} rows={} built_mean={:.2} built_min={:.2}", n, d.rows, sum / n.max(1) as f32, if n == 0 { 0.0 } else { low });
    }
}

/// Each building's line is written into one reused buffer and compared with the last one
/// written, so only new, changed and gone buildings allocate.
fn buildings(sim: &SimState, j: &mut Journal) {
    let mut line = std::mem::take(&mut j.line);
    // The id, the line before (none for a new building) and the line now, written in id order.
    let mut changed: Vec<(BuildingId, Option<String>, String)> = Vec::new();
    for b in &sim.buildings.buildings {
        line.clear();
        building_line_into(&mut line, sim, b);
        match j.buildings.get_mut(&b.id) {
            Some(old) if *old == line => {}
            Some(old) => {
                let was = std::mem::replace(old, line.clone());
                changed.push((b.id, Some(was), line.clone()));
            }
            None => {
                j.buildings.insert(b.id, line.clone());
                changed.push((b.id, None, line.clone()));
            }
        }
    }
    changed.sort_by_key(|c| c.0);
    for (id, old, line) in changed {
        match old {
            None => trace::event(format!("BUILDING_NEW {} id={} {line}", stamp(sim), id.0)),
            Some(old) => trace::event(format!("BUILDING_CHANGE {} id={} {old} -> {line}", stamp(sim), id.0)),
        }
    }
    // Every building is in the record by now, so it holds more only when some have gone.
    if j.buildings.len() > sim.buildings.buildings.len() {
        let live: std::collections::BTreeSet<BuildingId> = sim.buildings.buildings.iter().map(|b| b.id).collect();
        let gone: Vec<BuildingId> = j.buildings.keys().filter(|id| !live.contains(id)).copied().collect();
        for id in gone {
            if let Some(was) = j.buildings.remove(&id) {
                trace::event(format!("BUILDING_GONE {} id={} was {was}", stamp(sim), id.0));
            }
        }
    }
    j.line = line;
}

fn colony_brief(sim: &SimState) -> String {
    let stores = &sim.buildings.stores;
    format!(
        "colony[pop={} season={:?} food={:.1} food_need_per_day={:.2} food_days_left={:.1} sticks={:.1} clay={:.1} pebbles={:.1} saplings={:.1} lodges={}]",
        sim.colony.population(), crate::calendar::Season::at(sim.time_days), stores.amount(Good::Food),
        crate::hud::food_need_per_day(sim), crate::hud::food_days_left(sim), stores.amount(Good::Sticks),
        stores.amount(Good::Clay), stores.amount(Good::Pebbles), stores.amount(Good::Saplings), sim.colony.lodges.len()
    )
}

/// Write every beaver, and the whole map, when their times come.
pub fn poll(sim: &mut SimState) {
    if !trace::enabled() || !sim.journal.started {
        return;
    }
    if sim.time_days >= sim.journal.next_beavers {
        sim.journal.next_beavers = sim.time_days + BEAVER_POLL_DAYS;
        let mut line = String::new();
        for b in &sim.colony.beavers {
            line.clear();
            let _ = write!(line, "POLL_BEAVER {} ", stamp(sim));
            describe_into(&mut line, b);
            line.push_str(" | ");
            agent_context_into(&mut line, sim, b.id);
            trace::event(&line);
        }
        let alerts = format!("{:?}", crate::hud::problems(sim));
        if alerts != sim.journal.alerts {
            trace::event(format!("ALERTS {} {} -> {alerts}", stamp(sim), sim.journal.alerts));
            sim.journal.alerts = alerts;
        }
    }
    if sim.time_days >= sim.journal.next_map {
        sim.journal.next_map = sim.time_days + MAP_POLL_DAYS;
        map(sim);
    }
}
fn map(sim: &SimState) {
    let s = stamp(sim);
    trace::event(format!("POLL_COLONY {s} {} births={} deaths={} departures={} immigrants={} peak={} families={} spoiled={:.1} eaten={:?}",
        colony_brief(sim), sim.colony.stats.births, sim.colony.stats.deaths, sim.colony.stats.departures,
        sim.colony.stats.immigrants, sim.colony.stats.peak_population, sim.colony.families.len(), sim.colony.spoiled, sim.colony.eaten));
    let stores = &sim.buildings.stores;
    let mut goods = String::new();
    for g in Good::ALL {
        let _ = write!(goods, " {g:?}={:.1}/{:.0}", stores.amount(g), stores.capacity(g));
    }
    trace::event(format!("POLL_STORES {s}{goods}"));
    // Row by row, as the grids are laid out; ice and velocity share the depth grid's size.
    let (ice, velocity) = (sim.water.ice.as_slice(), sim.water.velocity.as_slice());
    let (mut wet, mut sum, mut deepest, mut iced, mut fast) = (0usize, 0.0f64, 0.0f32, 0usize, 0.0f32);
    for (i, &depth) in sim.water.depth.as_slice().iter().enumerate() {
        if depth > crate::buildings::WET_DEPTH {
            wet += 1;
            sum += depth as f64;
            deepest = deepest.max(depth);
            if ice.get(i).is_some_and(|&i| i > crate::tuning::water::ICE_COVER_M) {
                iced += 1;
            }
            let (vx, vy) = velocity.get(i).copied().unwrap_or((0.0, 0.0));
            fast = fast.max(vx.hypot(vy));
        }
    }
    trace::event(format!("POLL_WATER {s} wet_cells={wet} mean_depth={:.3} max_depth={deepest:.2} iced_cells={iced} max_velocity={fast:.3}", if wet > 0 { sum / wet as f64 } else { 0.0 }));
    // Tallied per kind (there are a handful), then written in the order of the kinds' names.
    let mut kinds: Vec<(ResourceKind, (usize, f32, usize))> = Vec::new();
    for r in sim.resources.items() {
        let e = match kinds.iter().position(|(k, _)| *k == r.kind) {
            Some(i) => &mut kinds[i].1,
            None => {
                kinds.push((r.kind, Default::default()));
                &mut kinds.last_mut().expect("just pushed").1
            }
        };
        e.0 += 1;
        e.1 += if r.kind == ResourceKind::Felled { r.amount + r.branches + r.bark } else { r.amount };
        e.2 += r.wet as usize;
    }
    let mut kinds: Vec<(String, (usize, f32, usize))> = kinds.into_iter().map(|(k, e)| (format!("{k:?}"), e)).collect();
    kinds.sort_by(|a, b| a.0.cmp(&b.0));
    let mut res = String::new();
    for (k, (n, amount, wet)) in kinds {
        let _ = write!(res, " {k}[n={n} amount={amount:.0} wet={wet}]");
    }
    trace::event(format!("POLL_RESOURCES {s}{res}"));
    for b in &sim.buildings.buildings {
        trace::event(format!("POLL_BUILDING {s} id={} {}", b.id.0, building_line(sim, b)));
    }
    for l in &sim.colony.lodges {
        let residents: Vec<u64> = sim.colony.beavers.iter().filter(|b| b.lodge == Some(l.id)).map(|b| b.id.0).collect();
        let cell = (l.pos.0.max(0.0) as usize, l.pos.1.max(0.0) as usize);
        let depth = sim.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0);
        trace::event(format!("POLL_LODGE {s} id={} family={} pos=({:.1},{:.1}) condition={:.2} depth={depth:.2} residents={residents:?}", l.id.0, l.family.0, l.pos.0, l.pos.1, l.condition));
    }
    match &sim.predators.wolf {
        Some(w) => trace::event(format!("POLL_WOLF {s} state={:?} pos=({:.1},{:.1}) pace={:.1} arrived={:.2} leave_by={:.2}", w.state, w.pos.0, w.pos.1, w.pace, w.arrived, w.leave_by)),
        None => trace::event(format!("POLL_WOLF {s} absent")),
    }
    let t = sim.step_timings;
    trace::event(format!("POLL_PERF {s} steps={} water_s={:.1} colony_s={:.1} agents_s={:.1} buildings_s={:.1} other_s={:.1} ecology_s={:.1} shipping_s={:.1} fog_s={:.1}", t.steps, t.water_s, t.colony_s, t.agents_s, t.buildings_s, t.other_s, t.ecology_s, t.shipping_s, t.fog_s));
}
