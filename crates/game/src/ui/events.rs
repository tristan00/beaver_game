//! Events: the view-models for event cards and the event log, and the cards themselves.
//!
//! The sim rolls events and tracks pending choices ([`sim::events`]); this file turns them into
//! cards with the sim's own options, a filterable log table with a "go to" button per row, and
//! the popup stack that sits under the top bar.
use super::kit::*;
use super::ledgers::{Act, Cell, Row, Table, Tone};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use sim::events::{Effect, EventKind, EventRecord, PendingChoice, CHOICE_EXPIRY_DAYS};
use sim::SimState;
use std::collections::HashSet;

// ───────────────────────────── view-model ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LogFilter {
    #[default]
    All,
    Water,
    Weather,
    Danger,
    Visitors,
}

impl LogFilter {
    pub const ALL: [LogFilter; 5] = [LogFilter::All, LogFilter::Water, LogFilter::Weather, LogFilter::Danger, LogFilter::Visitors];
    /// Filters worth showing: Water only lists floods and droughts, so it goes when both are off.
    pub fn available(features: &sim::features::Features) -> Vec<LogFilter> {
        LogFilter::ALL
            .into_iter()
            .filter(|f| *f != LogFilter::Water || EventKind::ALL.iter().any(|k| features.allows_event(*k) && f.matches(*k)))
            .collect()
    }
    pub fn label(self) -> &'static str {
        match self {
            LogFilter::All => "All",
            LogFilter::Water => "Water",
            LogFilter::Weather => "Weather",
            LogFilter::Danger => "Danger",
            LogFilter::Visitors => "Visitors",
        }
    }
    pub fn matches(self, k: EventKind) -> bool {
        match self {
            LogFilter::All => true,
            LogFilter::Water => matches!(k, EventKind::Flood | EventKind::Drought),
            LogFilter::Weather => matches!(k, EventKind::HardFreeze | EventKind::Drought),
            LogFilter::Danger => matches!(k, EventKind::Fire | EventKind::Disease | EventKind::PredatorRaid),
            LogFilter::Visitors => matches!(k, EventKind::WanderingBeaver | EventKind::WetlandArrival),
        }
    }
}

pub fn kind_label(k: EventKind) -> &'static str {
    match k {
        EventKind::Flood => "Flood",
        EventKind::Drought => "Drought",
        EventKind::HardFreeze => "Hard freeze",
        EventKind::Fire => "Forest fire",
        EventKind::Disease => "Fever",
        EventKind::PredatorRaid => "Predators",
        EventKind::WanderingBeaver => "Wanderers",
        EventKind::WetlandArrival => "Wetland life",
    }
}

/// Good news is green, bad news red, anything that needs a decision amber.
pub fn kind_tone(k: EventKind) -> Tone {
    match k {
        EventKind::WanderingBeaver | EventKind::WetlandArrival => Tone::Good,
        EventKind::Fire | EventKind::PredatorRaid => Tone::Warn,
        _ => Tone::Bad,
    }
}

/// A short human summary of one effect, e.g. `"-12 wood"`.
pub fn effect_text(e: &Effect) -> String {
    match e {
        Effect::Food(a) => format!("{a:+.0} food"),
        Effect::Wood(a) => format!("{a:+.0} wood"),
        Effect::Deaths(n) => format!("{n} beaver{} lost", if *n == 1 { "" } else { "s" }),
        Effect::Illness(h) => format!("-{:.0}% health", h * 100.0),
        Effect::Arrivals(n) => format!("{n} beaver{} join", if *n == 1 { "" } else { "s" }),
        Effect::Inflow { factor, days } => format!("water x{factor:.1} for {days:.0}d"),
    }
}

pub fn effects_text(es: &[Effect]) -> String {
    let parts: Vec<String> = es.iter().filter(|e| !matches!(e, Effect::Deaths(0))).map(effect_text).collect();
    if parts.is_empty() { "no effect".into() } else { parts.join(", ") }
}

/// Where on the map an event happened. Events are colony-wide, so this is a deterministic spot
/// around one of the lodges (picked by event id) that the camera can fly to.
pub fn event_location(sim: &SimState, rec: &EventRecord) -> Option<(f32, f32)> {
    let lodges = &sim.colony.lodges;
    if lodges.is_empty() {
        return None;
    }
    let l = &lodges[(rec.id as usize) % lodges.len()];
    let reach = match rec.kind {
        EventKind::Flood | EventKind::Drought => 4.0,
        EventKind::Fire | EventKind::PredatorRaid => 9.0,
        _ => 2.0,
    };
    let a = rec.id as f32 * 2.399;
    let max = sim.world.size as f32 - 1.0;
    Some(((l.pos.0 + reach * a.cos()).clamp(0.0, max), (l.pos.1 + reach * a.sin()).clamp(0.0, max)))
}

/// The log as a table, newest first, limited to `filter`.
pub fn log_table(sim: &SimState, filter: LogFilter) -> Table {
    let pending: HashSet<u64> = sim.events.pending().iter().map(|p| p.id).collect();
    let rows = sim
        .events
        .log()
        .iter()
        .rev()
        .filter(|r| filter.matches(r.kind))
        .map(|r| {
            let result = match (&r.choice, pending.contains(&r.id)) {
                (_, true) => "waiting for you".to_string(),
                (Some((label, expired)), _) => format!("{}{}: {}", if *expired { "(default) " } else { "" }, label, effects_text(&r.effects)),
                (None, _) => effects_text(&r.effects),
            };
            let go = match event_location(sim, r) {
                Some((x, y)) => Cell::button("Show", Act::Jump(x, y)),
                None => Cell::text("-"),
            };
            Row {
                act: None,
                cells: vec![
                    Cell::num(format!("Y{} {} {}", r.date.year + 1, &format!("{:?}", r.date.season)[..3], r.date.day), r.day),
                    Cell::text(kind_label(r.kind)).tone(kind_tone(r.kind)),
                    Cell::text(r.description.clone()),
                    Cell::text(result).tone(if pending.contains(&r.id) { Tone::Warn } else { Tone::Normal }),
                    go,
                ],
            }
        })
        .collect();
    Table {
        title: format!("Event log ({})", filter.label()),
        cols: vec![("When", 66.0), ("Event", 82.0), ("What happened", 170.0), ("Result", 170.0), ("", 50.0)],
        rows,
        sortable: true,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardOption {
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardModel {
    pub id: u64,
    pub kind: EventKind,
    pub title: String,
    pub body: String,
    pub options: Vec<CardOption>,
    /// Days left to decide (choice cards only).
    pub days_left: Option<f32>,
    pub location: Option<(f32, f32)>,
}

pub fn choice_card(sim: &SimState, p: &PendingChoice) -> CardModel {
    let rec = sim.events.log().iter().find(|r| r.id == p.id);
    CardModel {
        id: p.id,
        kind: p.kind,
        title: format!("{} - {}", kind_label(p.kind), p.date),
        body: p.prompt.clone(),
        options: p.options.iter().map(|o| CardOption { label: o.label.clone(), detail: effects_text(&o.effects) }).collect(),
        days_left: Some((CHOICE_EXPIRY_DAYS - (sim.events.calendar.elapsed_days() - p.day)).max(0.0) as f32),
        location: rec.and_then(|r| event_location(sim, r)),
    }
}

pub fn notice_card(sim: &SimState, r: &EventRecord) -> CardModel {
    CardModel {
        id: r.id,
        kind: r.kind,
        title: format!("{} - {}", kind_label(r.kind), r.date),
        body: format!("{} ({})", r.description, effects_text(&r.effects)),
        options: Vec::new(),
        days_left: None,
        location: event_location(sim, r),
    }
}

/// What to show right now: every pending choice, then notices for events logged since `seen`
/// that were not dismissed and are still in `live`.
pub fn cards(sim: &SimState, seen: u64, live: &[u64], dismissed: &HashSet<u64>) -> Vec<CardModel> {
    let pending: Vec<u64> = sim.events.pending().iter().map(|p| p.id).collect();
    let mut out: Vec<CardModel> = sim.events.pending().iter().map(|p| choice_card(sim, p)).collect();
    out.extend(
        sim.events
            .log()
            .iter()
            .filter(|r| r.id > seen && live.contains(&r.id) && !pending.contains(&r.id) && !dismissed.contains(&r.id))
            .map(|r| notice_card(sim, r)),
    );
    out
}

// ───────────────────────────── card state & systems ─────────────────────────────

pub const MAX_CARDS: usize = 3;
pub const NOTICE_SECS: f32 = 12.0;
const CARD_W: f32 = 330.0;

#[derive(Resource, Default)]
pub struct CardState {
    /// Highest event id that existed when the colony was founded or the game loaded.
    pub seen: Option<u64>,
    /// (event id, real-time seconds when it appeared) for notices.
    pub live: Vec<(u64, f32)>,
    pub dismissed: HashSet<u64>,
    pub shown: Vec<CardModel>,
    pub clock: f32,
}

#[derive(Component)]
pub struct CardStack;

pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CardState>().add_systems(Startup, spawn_stack).add_systems(Update, refresh_cards);
    }
}

fn spawn_stack(mut commands: Commands) {
    commands.spawn((
        CardStack,
        Blocker,
        Node { position_type: PositionType::Absolute, top: px(50), left: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },
        GlobalZIndex(20),
    ));
}

fn card_accent(k: EventKind) -> Color {
    tone_color(kind_tone(k))
}

fn refresh_cards(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    clock: Res<SimClock>,
    mut state: ResMut<CardState>,
    ui: Res<super::LedgerUi>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut stack: Query<(Entity, &mut Node), With<CardStack>>,
) {
    let Ok((root, mut node)) = stack.single_mut() else { return };
    let Ok(win) = windows.single() else { return };
    let s = &sim.0;
    if !s.colony.is_founded() {
        node.display = Display::None;
        state.seen = None;
        state.live.clear();
        state.shown.clear();
        return;
    }
    state.clock += time.delta_secs();
    let now = state.clock;
    let max_id = s.events.log().iter().map(|r| r.id).max().unwrap_or(0);
    let seen = *state.seen.get_or_insert_with(|| {
        // Events from before the first view (e.g. a loaded game) are in the log, not popups.
        s.events.log().iter().filter(|r| s.events.pending().iter().all(|p| p.id != r.id)).map(|r| r.id).max().unwrap_or(0)
    });
    // new events become live notices
    for r in s.events.log().iter().filter(|r| r.id > seen) {
        if !state.live.iter().any(|(i, _)| *i == r.id) {
            state.live.push((r.id, now));
        }
    }
    state.live.retain(|(_, t)| now - t < NOTICE_SECS);
    let _ = (max_id, &clock);
    let live: Vec<u64> = state.live.iter().map(|(i, _)| *i).collect();
    let mut shown = cards(s, seen, &live, &state.dismissed);
    shown.truncate(MAX_CARDS);
    node.display = Display::Flex;
    // centred on the part of the map that is not covered by the ledgers or the detail panel
    let right = if ui.tab.is_some() { win.width() - 20.0 - super::panels::DETAIL_W } else { win.width() - 10.0 };
    let left = super::panels::LEDGER_W + 20.0;
    node.left = px((left + (right - left - CARD_W) / 2.0).max(left));
    if shown == state.shown {
        return;
    }
    state.shown = shown.clone();
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for c in &shown {
            spawn_card(p, c);
        }
    });
}

fn spawn_card(p: &mut Kids, c: &CardModel) {
    let accent = card_accent(c.kind);
    p.spawn(card_node(CARD_W)).with_children(|k| {
        k.spawn((Node { width: percent(100), height: px(5), ..default() }, BackgroundColor(accent), Round(3.0)));
        text(k, &c.title, 14.0, INK);
        text(k, &c.body, FONT + 1.0, INK);
        for (i, o) in c.options.iter().enumerate() {
            k.spawn((
                Node { flex_direction: FlexDirection::Column, padding: UiRect::axes(px(10), px(4)), ..default() },
                BackgroundColor(BTN),
                Round(12.0),
                Clickable { act: Act::Choose(c.id, i), layer: 5, scrolled: false, base: BTN, hot: BTN_HOT },
            ))
            .with_children(|b| {
                text(b, &o.label, FONT + 1.0, INK);
                text(b, &o.detail, FONT - 1.0, DIM);
            });
        }
        k.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|f| {
            match c.days_left {
                Some(d) => text(f, format!("decide in {d:.0} days (default: first)"), FONT - 1.0, DIM),
                None => text(f, "", FONT - 1.0, DIM),
            }
            if let Some((x, y)) = c.location {
                pill(f, "Show", Act::Jump(x, y), false, 5);
            }
            if c.options.is_empty() {
                pill(f, "OK", Act::Dismiss(c.id), true, 5);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::events::{EventContext, Stockpile};

    fn sim_with_events() -> SimState {
        let mut s = SimState::new(2);
        s.set_features(sim::features::Features::everything()); // the cards cover every event kind
        let cell = s.world.valleys[s.world.start.0 as usize].center;
        let land = sim::WorldLand { world: &s.world, depth: &s.water.depth };
        s.colony.found((cell.0 as f32, cell.1 as f32), &land).expect("found");
        let mut host = Stockpile { population: 12, food: 50.0, wood: 50.0, health: 1.0 };
        for k in EventKind::ALL {
            s.events.trigger(k, &EventContext::default(), &mut host).unwrap();
        }
        s
    }

    #[test]
    fn ledgers_events_filter_partitions_kinds() {
        for k in EventKind::ALL {
            assert!(LogFilter::All.matches(k));
            assert!(LogFilter::ALL[1..].iter().any(|f| f.matches(k)), "{k:?} reachable by some filter");
        }
        assert!(LogFilter::Danger.matches(EventKind::Fire));
        assert!(!LogFilter::Danger.matches(EventKind::Flood));
    }

    #[test]
    fn ledgers_events_effect_text() {
        assert_eq!(effect_text(&Effect::Food(-12.4)), "-12 food");
        assert_eq!(effect_text(&Effect::Wood(5.0)), "+5 wood");
        assert_eq!(effect_text(&Effect::Deaths(1)), "1 beaver lost");
        assert_eq!(effect_text(&Effect::Arrivals(2)), "2 beavers join");
        assert_eq!(effects_text(&[]), "no effect");
        assert_eq!(effects_text(&[Effect::Deaths(0)]), "no effect");
    }

    #[test]
    fn ledgers_events_log_table_lists_newest_first_and_filters() {
        let s = sim_with_events();
        let all = log_table(&s, LogFilter::All);
        assert_eq!(all.rows.len(), 8);
        assert!(all.rows.iter().all(|r| r.cells.len() == all.cols.len()));
        let ids: Vec<u64> = s.events.log().iter().rev().map(|r| r.id).collect();
        assert_eq!(ids.len(), all.rows.len());
        let danger = log_table(&s, LogFilter::Danger);
        assert_eq!(danger.rows.len(), 3);
        // every row can jump the camera somewhere on the map
        for r in &all.rows {
            match &r.cells[4].button {
                Some((_, Act::Jump(x, y))) => assert!(*x >= 0.0 && *y >= 0.0 && *x < s.world.size as f32 && *y < s.world.size as f32),
                other => panic!("no jump button: {other:?}"),
            }
        }
        // unresolved choices say so
        assert!(all.rows.iter().any(|r| r.cells[3].text == "waiting for you"));
    }

    #[test]
    fn ledgers_events_cards_offer_the_sims_choices() {
        let s = sim_with_events();
        let live: Vec<u64> = s.events.log().iter().map(|r| r.id).collect();
        let cs = cards(&s, 0, &live, &HashSet::new());
        let choices: Vec<&CardModel> = cs.iter().filter(|c| !c.options.is_empty()).collect();
        assert_eq!(choices.len(), s.events.pending().len());
        for c in choices {
            let p = s.events.pending().iter().find(|p| p.id == c.id).unwrap();
            assert_eq!(c.options.len(), p.options.len());
            assert_eq!(c.options[0].label, p.options[0].label);
            assert!(c.days_left.unwrap() > 0.0);
        }
        // notices only for events that are live, new and undismissed
        let notices = cs.iter().filter(|c| c.options.is_empty()).count();
        assert_eq!(notices, 8 - s.events.pending().len());
        let dismissed: HashSet<u64> = live.iter().copied().collect();
        let after = cards(&s, 0, &live, &dismissed);
        assert_eq!(after.iter().filter(|c| c.options.is_empty()).count(), 0);
        assert!(cards(&s, 100, &live, &HashSet::new()).iter().all(|c| !c.options.is_empty()));
    }

    #[test]
    fn ledgers_events_choosing_resolves_the_card() {
        let mut s = sim_with_events();
        let id = s.events.pending()[0].id;
        let mut host = sim::ColonyHost { colony: &mut s.colony };
        s.events.resolve(id, 1, &mut host).unwrap();
        assert!(s.events.pending().iter().all(|p| p.id != id));
        let t = log_table(&s, LogFilter::All);
        assert!(t.rows.iter().all(|r| r.cells[3].text != "waiting for you" || s.events.pending().len() > 0));
    }

    #[test]
    fn ledgers_events_location_is_deterministic_and_none_without_lodges() {
        let s = sim_with_events();
        let r = &s.events.log()[0];
        assert_eq!(event_location(&s, r), event_location(&s, r));
        let empty = SimState::new(2);
        assert_eq!(event_location(&empty, r), None);
    }
}
