//! Events and alerts that point to places (owner: `events_ui`). Ten kinds, each with a map
//! location: birth, coming of age, death, new family, site finished (what happened) and hunger,
//! no shelter, low mood, too few workers, join request (what needs attention). Pure view-model:
//! `Notices` turns colony log entries into short-lived toasts, `alerts` reads standing problems off
//! the state, `toasts` lists what to draw; clicking a toast is `Do::Jump`, which flies the camera.

use std::collections::HashSet;

use sim::buildings::{BuildingId, Status};
use sim::colony::{BeaverId, DeathCause, Event, LoggedEvent, Stage};
use sim::society::RequestId;
use sim::water::Cell;
use sim::{Command, SimState};

use super::icons::Icon;
use super::screens::{Action, Do, Tone};

/// Days an event toast stays up.
pub const NOTICE_DAYS: f64 = 4.0;
/// Most toasts on the map at once.
pub const MAX_TOASTS: usize = 4;
/// A need below this counts as a problem worth an alert.
pub const HUNGRY_BELOW: f32 = 0.3;
pub const LOW_MOOD_BELOW: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertKind {
    Birth,
    ComingOfAge,
    Death,
    NewFamily,
    SiteFinished,
    Hunger,
    NoShelter,
    LowMood,
    TooFewWorkers,
    JoinRequest,
}

impl AlertKind {
    pub const ALL: [AlertKind; 10] = [
        AlertKind::Birth,
        AlertKind::ComingOfAge,
        AlertKind::Death,
        AlertKind::NewFamily,
        AlertKind::SiteFinished,
        AlertKind::Hunger,
        AlertKind::NoShelter,
        AlertKind::LowMood,
        AlertKind::TooFewWorkers,
        AlertKind::JoinRequest,
    ];
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Birth => "Birth",
            AlertKind::ComingOfAge => "Coming of age",
            AlertKind::Death => "Death",
            AlertKind::NewFamily => "New family",
            AlertKind::SiteFinished => "Site finished",
            AlertKind::Hunger => "Hunger",
            AlertKind::NoShelter => "No shelter",
            AlertKind::LowMood => "Low mood",
            AlertKind::TooFewWorkers => "Too few workers",
            AlertKind::JoinRequest => "Join request",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            AlertKind::Birth | AlertKind::ComingOfAge | AlertKind::NewFamily => Icon::Social,
            AlertKind::Death | AlertKind::LowMood => Icon::Mood,
            AlertKind::SiteFinished => Icon::Build,
            AlertKind::Hunger => Icon::Food,
            AlertKind::NoShelter => Icon::Shelter,
            AlertKind::TooFewWorkers => Icon::Builder,
            AlertKind::JoinRequest => Icon::Accept,
        }
    }
    pub fn tone(self) -> Tone {
        match self {
            AlertKind::Birth | AlertKind::ComingOfAge | AlertKind::NewFamily | AlertKind::SiteFinished => Tone::Good,
            AlertKind::Death | AlertKind::Hunger | AlertKind::NoShelter => Tone::Bad,
            AlertKind::LowMood | AlertKind::TooFewWorkers | AlertKind::JoinRequest => Tone::Warn,
        }
    }
    /// A problem that stays until fixed (as opposed to something that happened).
    pub fn standing(self) -> bool {
        matches!(self, AlertKind::Hunger | AlertKind::NoShelter | AlertKind::LowMood | AlertKind::TooFewWorkers | AlertKind::JoinRequest)
    }
}

/// A brief notice that points at a place; clicking flies the camera to `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub kind: AlertKind,
    pub text: String,
    pub at: Cell,
    pub expires_day: f64,
    /// Set for join requests: the stranger's request, answered from the toast.
    pub request: Option<RequestId>,
}

fn cell_of(p: (f32, f32)) -> Cell {
    (p.0.max(0.0).round() as usize, p.1.max(0.0).round() as usize)
}

/// The middle cell of a building's footprint, so an alert about it points at the building itself.
pub fn centre(b: &sim::buildings::Building) -> Cell {
    let (w, h) = b.kind.spec().footprint;
    (b.pos.0 + w / 2, b.pos.1 + h / 2)
}

fn name_of(sim: &SimState, id: BeaverId) -> String {
    sim.colony.beaver(id).map(|b| super::screens::beaver_name(&sim.colony, b)).unwrap_or_else(|| format!("Beaver {}", id.0))
}

/// Where the colony lives: the first lodge, or the map middle before there is one.
fn home(sim: &SimState) -> Cell {
    sim.colony.lodges.first().map(|l| cell_of(l.pos)).unwrap_or((sim.height.width() / 2, sim.height.height() / 2))
}

fn cause_word(c: DeathCause) -> &'static str {
    match c {
        DeathCause::Starvation => "hunger",
        DeathCause::Cold => "cold",
        DeathCause::Injury => "injury",
        DeathCause::Exhaustion => "exhaustion",
        DeathCause::OldAge => "old age",
    }
}

/// Standing problems and open join requests for the current state, each with a place. (`events_ui` task.)
pub fn alerts(sim: &SimState) -> Vec<Alert> {
    let now = sim.time_days;
    let c = &sim.colony;
    let mut out = Vec::new();
    for r in sim.society.pending() {
        out.push(Alert { kind: AlertKind::JoinRequest, text: format!("{} asks to join", r.name), at: r.at, expires_day: f64::MAX, request: Some(r.id) });
    }
    if !c.is_founded() {
        return out;
    }
    let mut standing = |kind: AlertKind, text: String, at: Cell| out.push(Alert { kind, text, at, expires_day: now + 1.0, request: None });
    let hungry: Vec<_> = c.beavers.iter().filter(|b| b.needs.food < HUNGRY_BELOW).collect();
    if let Some(w) = hungry.iter().min_by(|a, b| a.needs.food.total_cmp(&b.needs.food)) {
        standing(AlertKind::Hunger, format!("{} beavers are hungry", hungry.len()), cell_of(w.pos));
    }
    let homeless: Vec<_> = c.beavers.iter().filter(|b| b.lodge.is_none()).collect();
    if let Some(b) = homeless.first() {
        standing(AlertKind::NoShelter, format!("{} beavers have no shelter", homeless.len()), cell_of(b.pos));
    }
    let gloomy: Vec<_> = c.beavers.iter().filter(|b| b.mood < LOW_MOOD_BELOW).collect();
    if let Some(w) = gloomy.iter().min_by(|a, b| a.mood.total_cmp(&b.mood)) {
        standing(AlertKind::LowMood, format!("{} is low in mood", name_of(sim, w.id)), cell_of(w.pos));
    }
    for p in &sim.work.places {
        let idle = !p.slots.is_empty() && p.slots.iter().all(|s| s.worker.is_none());
        if let Some(b) = sim.buildings.building(p.building).filter(|b| idle && b.status != Status::Complete) {
            standing(AlertKind::TooFewWorkers, format!("{:?} site has no workers", b.kind), centre(b));
            break;
        }
    }
    out
}

/// Short-lived notices of what just happened, fed from the colony log.
#[derive(Clone, Debug, Default)]
pub struct Notices {
    pub items: Vec<Alert>,
    unfinished: HashSet<BuildingId>,
}

impl Notices {
    /// Turns new log entries (and sites that finished since the last call) into notices.
    pub fn ingest(&mut self, sim: &SimState, events: &[LoggedEvent]) {
        let now = sim.time_days;
        let c = &sim.colony;
        let at_beaver = |id: BeaverId| c.beaver(id).map(|b| cell_of(b.pos)).unwrap_or_else(|| home(sim));
        let mut add = |kind: AlertKind, text: String, at: Cell| self.items.push(Alert { kind, text, at, expires_day: now + NOTICE_DAYS, request: None });
        for e in events {
            match &e.event {
                Event::Born { id, .. } => add(AlertKind::Birth, format!("A kit is born: {}", name_of(sim, *id)), at_beaver(*id)),
                Event::Matured { id, stage: Stage::Adult } => add(AlertKind::ComingOfAge, format!("{} comes of age", name_of(sim, *id)), at_beaver(*id)),
                Event::Died { id, cause, at } => add(AlertKind::Death, format!("Beaver {} died of {}", id.0, cause_word(*cause)), cell_of(*at)),
                Event::NewFamily { lodge, .. } => {
                    let at = c.lodges.iter().find(|l| l.id == *lodge).map(|l| cell_of(l.pos)).unwrap_or_else(|| home(sim));
                    add(AlertKind::NewFamily, "A new family founds a lodge".into(), at)
                }
                _ => {}
            }
        }
        let now_unfinished: HashSet<BuildingId> = sim.buildings.buildings.iter().filter(|b| b.status != Status::Complete).map(|b| b.id).collect();
        for id in self.unfinished.difference(&now_unfinished) {
            if let Some(b) = sim.buildings.building(*id) {
                add(AlertKind::SiteFinished, format!("{:?} is finished", b.kind), centre(b));
            }
        }
        self.unfinished = now_unfinished;
        self.items.retain(|a| a.expires_day > now);
        if self.items.len() > 40 {
            self.items.drain(..self.items.len() - 40);
        }
    }
}

/// One toast on the map: the alert and its buttons (join requests carry accept and turn away).
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub alert: Alert,
    /// Clicking the toast flies the camera here.
    pub jump: Do,
    pub actions: Vec<Action>,
}

/// Everything live, most urgent first: open requests, standing problems, then what just happened (newest first).
pub fn live(sim: &SimState, notices: &Notices) -> Vec<Alert> {
    let now = sim.time_days;
    let mut all = alerts(sim);
    all.extend(notices.items.iter().rev().filter(|a| a.expires_day > now).cloned());
    all.sort_by_key(|a| match a.kind {
        AlertKind::JoinRequest => 0,
        k if k.standing() => 1,
        _ => 2,
    });
    all
}

/// What to draw on the map now.
pub fn toasts(sim: &SimState, notices: &Notices) -> Vec<Toast> {
    live(sim, notices).into_iter().take(MAX_TOASTS).map(toast).collect()
}

pub fn toast(alert: Alert) -> Toast {
    let jump = Do::Jump(alert.at.0 as f32, alert.at.1 as f32);
    let actions = alert
        .request
        .map(|id| {
            vec![
                Action { icon: Icon::Accept, label: "Welcome".into(), act: Do::Answer(id, true) },
                Action { icon: Icon::TurnAway, label: "Turn away".into(), act: Do::Answer(id, false) },
            ]
        })
        .unwrap_or_default();
    Toast { alert, jump, actions }
}

/// The sim command a join answer issues.
pub fn answer_command(id: RequestId, accept: bool) -> Command {
    let answer = if accept { sim::society::Answer::Accept } else { sim::society::Answer::TurnAway };
    Command::AnswerJoin { id, answer }
}

/// Test helper for other modules' tests.
#[cfg(test)]
pub fn every_kind_for_tests(sim: &mut SimState) -> Notices {
    tests::every_kind(sim)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use sim::colony::Event as E;

    fn log(sim: &SimState, e: E) -> LoggedEvent {
        LoggedEvent { day: sim.time_days, event: e }
    }

    fn free_site(sim: &mut SimState, kind: sim::buildings::BuildingKind) -> BuildingId {
        (4..60).flat_map(|y| (4..60).map(move |x| (x, y))).find_map(|p| sim.place_site(kind, p).ok()).expect("a site fits")
    }

    /// A lived-in colony with one live example of every kind.
    pub fn every_kind(sim: &mut SimState) -> Notices {
        let (b0, fam) = (sim.colony.beavers[0].id, sim.colony.beavers[0].family);
        let lodge = sim.colony.lodges[0].id;
        let site = free_site(sim, sim::buildings::BuildingKind::FoodCache);
        let mut n = Notices::default();
        n.ingest(sim, &[]);
        sim.buildings.buildings.iter_mut().find(|b| b.id == site).unwrap().status = Status::Complete;
        let events = vec![
            log(sim, E::Born { id: b0, family: fam }),
            log(sim, E::Matured { id: b0, stage: Stage::Adult }),
            log(sim, E::Died { id: BeaverId(9999), cause: DeathCause::Cold, at: (23.0, 31.0) }),
            log(sim, E::NewFamily { family: fam, lodge, a: b0, b: b0 }),
        ];
        n.ingest(sim, &events);
        // standing problems: one beaver is starving, homeless and gloomy; a site has no workers; a stranger waits
        let b = &mut sim.colony.beavers[1];
        (b.needs.food, b.lodge, b.mood) = (0.05, None, 0.1);
        let lonely = free_site(sim, sim::buildings::BuildingKind::Lodge);
        if let Some(p) = sim.work.places.iter_mut().find(|p| p.building == lonely) {
            p.slots.iter_mut().for_each(|s| s.worker = None);
        }
        let colony = sim.colony.clone();
        sim.society.raise(&colony, true, sim.time_days).unwrap();
        n
    }

    #[test]
    fn every_kind_is_listed_and_carries_a_location_on_the_map() {
        let mut sim = super::super::screens::tests::lived_in(3);
        let n = every_kind(&mut sim);
        let (w, h) = (sim.height.width(), sim.height.height());
        let mut seen = HashSet::new();
        for a in alerts(&sim).into_iter().chain(n.items.iter().cloned()) {
            assert!(a.at.0 < w && a.at.1 < h, "{:?} points off the map: {:?}", a.kind, a.at);
            assert!(!a.text.is_empty());
            seen.insert(a.kind);
        }
        for k in AlertKind::ALL {
            assert!(seen.contains(&k), "{k:?} never produced a located notice");
        }
        let died = n.items.iter().find(|a| a.kind == AlertKind::Death).unwrap();
        assert_eq!(died.at, (23, 31), "a death points where the beaver died, not at its (removed) record");
    }

    #[test]
    fn toasts_are_capped_lead_with_requests_and_jump_to_their_place() {
        let mut sim = super::super::screens::tests::lived_in(3);
        let n = every_kind(&mut sim);
        let t = toasts(&sim, &n);
        assert_eq!(t.len(), MAX_TOASTS);
        assert_eq!(t[0].alert.kind, AlertKind::JoinRequest);
        for x in &t {
            assert_eq!(x.jump, Do::Jump(x.alert.at.0 as f32, x.alert.at.1 as f32));
        }
    }

    #[test]
    fn building_alerts_point_inside_the_building() {
        let mut sim = super::super::screens::tests::lived_in(3);
        let n = every_kind(&mut sim);
        let mut checked = 0;
        for a in alerts(&sim).into_iter().chain(n.items.iter().cloned()).filter(|a| matches!(a.kind, AlertKind::TooFewWorkers | AlertKind::SiteFinished)) {
            assert!(sim.buildings.buildings.iter().any(|b| b.cells().any(|c| c == a.at)), "{:?} at {:?} is not on a building", a.kind, a.at);
            checked += 1;
        }
        assert_eq!(checked, 2);
    }

    #[test]
    fn event_notices_expire() {
        let mut sim = super::super::screens::tests::lived_in(3);
        let n = every_kind(&mut sim);
        sim.time_days += NOTICE_DAYS + 1.0;
        assert!(live(&sim, &n).iter().all(|t| t.kind.standing()));
    }

    #[test]
    fn a_join_request_toast_offers_accept_and_turn_away() {
        let mut sim = super::super::screens::tests::lived_in(3);
        let colony = sim.colony.clone();
        let id = sim.society.raise(&colony, true, sim.time_days).unwrap();
        let t = toasts(&sim, &Notices::default());
        let j = t.iter().find(|t| t.alert.request == Some(id)).expect("request shows");
        assert_eq!(j.actions.iter().map(|a| a.act.clone()).collect::<Vec<_>>(), vec![Do::Answer(id, true), Do::Answer(id, false)]);
        assert_eq!(j.actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Welcome", "Turn away"]);
        assert_eq!(answer_command(id, true), Command::AnswerJoin { id, answer: sim::society::Answer::Accept });
        assert_eq!(answer_command(id, false), Command::AnswerJoin { id, answer: sim::society::Answer::TurnAway });
    }

    #[test]
    fn default_game_view_models_list_no_hazard_rival_or_predator() {
        use super::super::mapmodes::{overlay, MapMode};
        use super::super::screens::{screen, History};
        use super::super::shell::NavItem;
        let mut sim = super::super::screens::tests::lived_in(3);
        for _ in 0..400 {
            sim.step(0.25).expect("step");
        }
        let n = every_kind(&mut sim);
        let mut text = format!("{:?} {:?}", alerts(&sim), n.items).to_lowercase();
        for item in NavItem::ALL {
            text += &format!("{:?}", screen(&sim, item, History::default().rates())).to_lowercase();
        }
        for m in MapMode::ALL {
            text += &format!("{:?}", overlay(&sim, m)).to_lowercase();
        }
        for w in ["predator", "rival", "flood", "drought", "freeze", "fire", "disease"] {
            assert!(!text.contains(w), "a view-model mentions '{w}'");
        }
    }
}

/// Map pins: every toast on screen marks its place on the terrain with a stem and ring in its tone,
/// so an alert visibly points at the building, beaver or spot it is about.
pub fn draw_pins(mut gizmos: bevy::prelude::Gizmos, sim: bevy::prelude::Res<crate::state::Sim>, notices: bevy::prelude::Res<super::NoticesRes>) {
    use bevy::prelude::*;
    if !sim.0.colony.is_founded() {
        return;
    }
    let h = &sim.0.height;
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for t in toasts(&sim.0, &notices.0) {
        let color = super::kit::tone_color(t.alert.kind.tone());
        let ground = crate::flow::cell_world(h, (t.alert.at.0 as f32, t.alert.at.1 as f32)) + Vec3::Y * 0.2;
        let top = ground + Vec3::Y * 6.0;
        gizmos.line(ground, top, color);
        gizmos.sphere(Isometry3d::from_translation(top), 0.7, color);
        for r in [1.6, 1.9, 2.2] {
            gizmos.circle(Isometry3d::new(ground, flat), r, color);
        }
    }
}

/// `BC_DEMO_ALERTS=1` (screenshots): once the colony is founded, a stranger asks to join and a lodge
/// site is placed near the first lodge with nobody on it, so a join request and a located alert show.
/// `BC_DEMO_ALERTS=jump` then activates the "no workers" toast exactly as a click does, and holds the
/// screenshot until the camera has flown to the site and zoomed in.
#[allow(clippy::too_many_arguments)]
pub fn stage_demo(
    mut sim: bevy::prelude::ResMut<crate::state::Sim>,
    mut flow: bevy::prelude::ResMut<crate::flow::Flow>,
    mut sel: bevy::prelude::ResMut<crate::flow::Selection>,
    mut ui: bevy::prelude::ResMut<super::ShellUi>,
    mut build: bevy::prelude::ResMut<crate::build::BuildUi>,
    notices: bevy::prelude::Res<super::NoticesRes>,
    gate: Option<bevy::prelude::ResMut<crate::flow::CaptureGate>>,
    mut orbit: bevy::prelude::ResMut<crate::camera::OrbitCamera>,
    mut step: bevy::prelude::Local<u8>,
) {
    let Some(mode) = std::env::var_os("BC_DEMO_ALERTS") else { return };
    let jump = mode == "jump";
    match *step {
        0 if sim.0.colony.is_founded() => {
            stage(&mut sim.0);
            *step = 1;
            if jump {
                if let Some(mut g) = gate {
                    g.0 = false;
                }
            }
        }
        1 if jump && flow.focus.is_none() => {
            // the settle fly-in has finished; now activate the building alert like a click would
            if let Some(t) = toasts(&sim.0, &notices.0).into_iter().find(|t| t.alert.kind == AlertKind::TooFewWorkers) {
                let mut reset = false;
                super::panels::apply(t.jump, &mut sim, &mut flow, &mut sel, &mut ui, &mut build, &mut reset);
                *step = 2;
            }
        }
        2 if flow.focus.is_none() => {
            // arrived over the site; the player scrolls in for a closer look
            orbit.distance = 38.0;
            *step = 3;
        }
        3 => {
            if let Some(mut g) = gate {
                g.0 = true;
            }
            *step = 4;
        }
        _ => {}
    }
}

fn stage(s: &mut SimState) {
    let colony = s.colony.clone();
    let _ = s.society.raise(&colony, true, s.time_days);
    let (lx, ly) = home(s);
    let spot = (0..40usize).flat_map(|r| (0..=r).flat_map(move |d| [(r, d), (d, r)])).find_map(|(dx, dy)| {
        [(lx + 6 + dx, ly + dy), (lx + 6 + dx, ly.saturating_sub(dy))].into_iter().find_map(|p| s.check_site(sim::buildings::BuildingKind::Lodge, p).ok().map(|()| p))
    });
    if let Some(p) = spot {
        let _ = s.place_site(sim::buildings::BuildingKind::Lodge, p);
    }
    s.work.auto_organise = false;
    if let Some(p) = s.work.places.iter_mut().find(|p| p.site.is_some()) {
        p.slots.iter_mut().for_each(|sl| sl.worker = None);
    }
}
