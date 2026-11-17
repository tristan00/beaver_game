//! Draws the top bar, the four compact ledgers and the detail panel from the view-models in
//! `ledgers.rs`, and runs what the player clicks. Content is rebuilt only when its view-model
//! actually changed, so hover and scroll state survive between sim updates.
use super::events::{CardState, LogFilter};
use super::kit::*;
use super::ledgers::*;
use crate::flow::{cell_world, Flow, Selection};
use crate::flow_view::{Action, UiLayout, UiState};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use sim::events::Season;
use sim::region::Link;
use sim::ColonyHost;
use std::collections::HashMap;

pub const LEDGER_W: f32 = 250.0;
pub const DETAIL_W: f32 = 570.0;
pub const TOP: f32 = 48.0;
const ROW_H: f32 = 21.0;
const REFRESH_SECS: f32 = 0.25;

#[derive(Resource)]
pub struct LedgerUi {
    pub tab: Option<Tab>,
    pub filter: LogFilter,
    /// Sort state per (tab, table index): (column, ascending).
    pub sort: HashMap<(Tab, usize), (usize, bool)>,
    pub status: String,
    pub hist: History,
    dirty: bool,
    timer: f32,
    last_top: Option<TopBar>,
    last_ledgers: Option<[Compact; 4]>,
    last_detail: Option<DetailSnap>,
}

impl Default for LedgerUi {
    fn default() -> Self {
        LedgerUi {
            tab: None,
            filter: LogFilter::All,
            sort: HashMap::new(),
            status: String::new(),
            hist: History::default(),
            dirty: true,
            timer: 0.0,
            last_top: None,
            last_ledgers: None,
            last_detail: None,
        }
    }
}

#[derive(Clone, PartialEq)]
struct DetailSnap {
    tab: Tab,
    filter: LogFilter,
    tables: Vec<Table>,
    sorts: Vec<Option<(usize, bool)>>,
    status: String,
}

#[derive(Component)]
struct TopBarRoot;
#[derive(Component)]
struct LedgerColumn;
#[derive(Component)]
struct DetailRoot;
#[derive(Component)]
struct DetailHeader;
#[derive(Component)]
struct DetailFooter;

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        let mut ui = LedgerUi::default();
        // Debug aid for screenshots: BC_UI_TAB=families|beavers|jobs|stores|trade|tech|events
        ui.tab = std::env::var("BC_UI_TAB").ok().and_then(|v| Tab::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(&v)));
        app.insert_resource(ui)
            .add_systems(Startup, spawn_roots)
            .add_systems(
                Update,
                (
                    (layout_roots, refresh, hover_tint, scroll_wheel).chain(),
                    publish_blockers.before(crate::flow_view::ClickSet),
                    handle_clicks.after(crate::flow_view::ClickSet),
                ),
            );
    }
}

fn spawn_roots(mut commands: Commands) {
    let abs = Node { position_type: PositionType::Absolute, ..default() };
    commands.spawn((TopBarRoot, Blocker, Node { height: px(34), align_items: AlignItems::Center, column_gap: px(14), padding: UiRect::axes(px(16), px(0)), ..abs.clone() }, BackgroundColor(PANEL), Round(17.0), BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.28), px(0), px(3), px(0), px(8)), GlobalZIndex(10)));
    commands.spawn((LedgerColumn, Blocker, Node { width: px(LEDGER_W), flex_direction: FlexDirection::Column, row_gap: px(6), ..abs.clone() }, GlobalZIndex(10)));
    commands
        .spawn((
            DetailRoot,
            Blocker,
            Node { width: px(DETAIL_W), flex_direction: FlexDirection::Column, padding: UiRect::all(px(10)), row_gap: px(6), ..abs },
            BackgroundColor(PANEL),
            Round(18.0),
            BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.32), px(0), px(4), px(0), px(12)),
            GlobalZIndex(12),
        ))
        .with_children(|p| {
            p.spawn((DetailHeader, Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), align_items: AlignItems::Center, ..default() }));
            p.spawn((
                ScrollArea,
                ScrollPosition::default(),
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_height: px(0), overflow: Overflow::scroll_y(), row_gap: px(8), padding: UiRect::right(px(4)), ..default() },
            ));
            p.spawn((DetailFooter, Node { min_height: px(16), ..default() }));
        });
}

fn season_color(s: Season) -> Color {
    match s {
        Season::Spring => Color::srgb(0.55, 0.78, 0.45),
        Season::Summer => Color::srgb(0.95, 0.75, 0.30),
        Season::Autumn => Color::srgb(0.88, 0.52, 0.25),
        Season::Winter => Color::srgb(0.58, 0.74, 0.88),
    }
}

fn layout_roots(
    windows: Query<&Window, With<PrimaryWindow>>,
    layout: Res<UiLayout>,
    sim: Res<Sim>,
    ui: Res<LedgerUi>,
    mut q: ParamSet<(
        Query<&mut Node, With<TopBarRoot>>,
        Query<&mut Node, With<LedgerColumn>>,
        Query<&mut Node, With<DetailRoot>>,
    )>,
) {
    let Ok(w) = windows.single() else { return };
    let founded = sim.0.colony.is_founded();
    let show = |on: bool| if on { Display::Flex } else { Display::None };
    for mut n in &mut q.p0() {
        let right = layout.rect(Action::Pause).map(|r| r.min.x - 8.0).unwrap_or(w.width() - 10.0);
        n.display = show(founded);
        n.left = px(10);
        n.top = px(8);
        n.width = px((right - 10.0).max(200.0));
    }
    for mut n in &mut q.p1() {
        n.display = show(founded);
        n.left = px(10);
        n.top = px(TOP);
    }
    for mut n in &mut q.p2() {
        n.display = show(founded && ui.tab.is_some());
        n.left = px(w.width() - 10.0 - DETAIL_W);
        n.top = px(TOP);
        n.height = px((w.height() - TOP - 10.0).max(200.0));
    }
}


fn trend_color(t: Trend, good_up: bool) -> Color {
    match (t, good_up) {
        (Trend::Flat, _) => DIM,
        (Trend::Up, true) | (Trend::Down, false) => GOOD,
        _ => BAD,
    }
}

fn spawn_top_bar(p: &mut Kids, t: &TopBar) {
    text(p, "Beaver Colony", 14.0, INK);
    text(p, &t.date, 13.0, INK);
    p.spawn((Node { padding: UiRect::axes(px(9), px(2)), ..default() }, BackgroundColor(season_color(t.season)), Round(99.0)))
        .with_children(|c| text(c, format!("{:?}", t.season), 12.0, INK));
    p.spawn(Node { column_gap: px(5), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        text(c, format!("Beavers {}", t.population), 13.0, INK);
        if t.pop_change != 0 {
            text(c, format!("{:+}", t.pop_change), 12.0, if t.pop_change > 0 { GOOD } else { BAD });
        }
    });
    p.spawn(Node { column_gap: px(5), align_items: AlignItems::Center, ..default() }).with_children(|c| {
        c.spawn((Node { width: px(10), height: px(10), ..default() }, BackgroundColor(tone_color(t.mood_tone)), Round(5.0)));
        text(c, format!("Mood {:.0}%", t.mood * 100.0), 13.0, INK);
    });
    text(p, format!("Speed {}", t.speed), 13.0, INK);
    if t.pending > 0 {
        pill(p, &format!("{} decision{} waiting", t.pending, if t.pending == 1 { "" } else { "s" }), Act::Open(Tab::Log), true, 4);
    } else {
        pill(p, "Event log", Act::Open(Tab::Log), false, 4);
    }
}

fn spawn_compact(p: &mut Kids, c: &Compact, open: bool) {
    let base = if open { Color::srgba(0.90, 0.97, 0.84, 0.97) } else { PANEL };
    p.spawn((
        Node { width: percent(100), flex_direction: FlexDirection::Column, padding: UiRect::axes(px(10), px(7)), row_gap: px(2), ..default() },
        BackgroundColor(base),
        Round(16.0),
        BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.28), px(0), px(3), px(0), px(8)),
        Clickable { act: Act::Open(c.tab), layer: 1, scrolled: false, base, hot: Color::srgb(1.0, 0.93, 0.76) },
    ))
    .with_children(|k| {
        k.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|r| {
            text(r, c.title.to_uppercase(), 11.0, DIM);
            r.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|s| {
                let good_up = true;
                sparkline(s, &c.spark, 64.0, 13.0, trend_color(c.trend, good_up).with_alpha(0.85));
                text(s, c.delta.clone(), 12.0, trend_color(c.trend, good_up));
            });
        });
        k.spawn(Node { column_gap: px(8), align_items: AlignItems::Baseline, ..default() }).with_children(|r| {
            r.spawn((Text::new(c.headline.clone()), TextFont::from_font_size(15.0), TextColor(INK), TextLayout::no_wrap()));
            r.spawn((Text::new(c.sub.clone()), TextFont::from_font_size(11.0), TextColor(DIM), TextLayout::no_wrap()));
        });
        for row in &c.rows {
            k.spawn(Node { height: px(14), align_items: AlignItems::Center, column_gap: px(5), ..default() }).with_children(|r| {
                let wide = row.frac.is_none() && row.value.is_empty();
                let label_color = if row.tone == Tone::Good { INK } else { tone_color(row.tone) };
                r.spawn(Node { width: if wide { auto() } else { px(96) }, overflow: Overflow::clip(), ..default() }).with_children(|l| {
                    l.spawn((Text::new(row.label.clone()), TextFont::from_font_size(11.0), TextColor(label_color), TextLayout::no_wrap()));
                });
                match row.frac {
                    Some(f) => bar(r, f, 52.0, 6.0, tone_color(if row.tone == Tone::Normal || row.tone == Tone::Dim { Tone::Good } else { row.tone })),
                    None => {
                        r.spawn(Node { width: px(52), ..default() });
                    }
                }
                text(r, row.value.clone(), 11.0, tone_color(row.tone));
            });
        }
    });
}

fn spawn_table(body: &mut Kids, t: &Table, tab: Tab, idx: usize, sort: Option<(usize, bool)>) {
    body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|k| {
        text(k, &t.title, 13.0, INK);
        // header
        k.spawn(Node { padding: UiRect::horizontal(px(5)), column_gap: px(0), ..default() }).with_children(|h| {
            for (ci, (name, w)) in t.cols.iter().enumerate() {
                let arrow = match sort {
                    Some((c, asc)) if c == ci => if asc { " ^" } else { " v" },
                    _ => "",
                };
                let base = PANEL_SOFT;
                let mut e = h.spawn((
                    Node { width: px(*w), height: px(19), padding: UiRect::horizontal(px(3)), align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() },
                    BackgroundColor(base),
                    Round(8.0),
                ));
                if t.sortable && !name.is_empty() {
                    e.insert(Clickable { act: Act::Sort(idx * 100 + ci), layer: 2, scrolled: false, base, hot: ROW_HOT });
                }
                e.with_children(|c| {
                    c.spawn((Text::new(format!("{name}{arrow}")), TextFont::from_font_size(11.0), TextColor(DIM), TextLayout::no_wrap()));
                });
            }
        });
        if t.rows.is_empty() {
            text(k, "Nothing here yet.", FONT, DIM);
        }
        for (ri, row) in t.rows.iter().enumerate() {
            let base = if ri % 2 == 0 { ROW_A } else { ROW_B };
            let mut e = k.spawn((
                Node { height: px(ROW_H), padding: UiRect::horizontal(px(5)), align_items: AlignItems::Center, flex_shrink: 0.0, ..default() },
                BackgroundColor(base),
                Round(8.0),
            ));
            if let Some(a) = &row.act {
                e.insert(Clickable { act: a.clone(), layer: 1, scrolled: true, base, hot: ROW_HOT });
            }
            e.with_children(|r| {
                for (ci, cell) in row.cells.iter().enumerate() {
                    let w = t.cols.get(ci).map(|c| c.1).unwrap_or(60.0);
                    r.spawn(Node { width: px(w), height: percent(100), padding: UiRect::horizontal(px(3)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(1), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() })
                        .with_children(|c| {
                            if let Some((label, act)) = &cell.button {
                                small_button(c, label, act.clone());
                                return;
                            }
                            c.spawn((Text::new(cell.text.clone()), TextFont::from_font_size(FONT), TextColor(tone_color(cell.tone)), TextLayout::no_wrap()));
                            if let Some(f) = cell.bar {
                                bar(c, f, (w - 10.0).min(44.0), 3.0, tone_color(if cell.tone == Tone::Normal || cell.tone == Tone::Dim { Tone::Good } else { cell.tone }));
                            }
                        });
                }
            });
            let _ = tab;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    clock: Res<SimClock>,
    mut ui: ResMut<LedgerUi>,
    top: Query<Entity, With<TopBarRoot>>,
    column: Query<Entity, With<LedgerColumn>>,
    header: Query<Entity, With<DetailHeader>>,
    body: Query<Entity, With<ScrollArea>>,
    footer: Query<Entity, With<DetailFooter>>,
) {
    let s = &sim.0;
    if !s.colony.is_founded() {
        ui.hist = History::default();
        ui.last_top = None;
        ui.last_ledgers = None;
        ui.last_detail = None;
        return;
    }
    // a restarted world rewinds time
    if ui.hist.samples.back().is_some_and(|x| x.day > s.time_days) {
        ui.hist = History::default();
    }
    ui.hist.record(s);
    ui.timer -= time.delta_secs();
    if ui.timer > 0.0 && !ui.dirty {
        return;
    }
    ui.timer = REFRESH_SECS;
    ui.dirty = false;

    let speed = if clock.paused { "paused".to_string() } else { clock.speed.label().to_string() };
    let tb = top_bar(s, &ui.hist, &speed);
    if ui.last_top.as_ref() != Some(&tb) {
        if let Ok(e) = top.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| spawn_top_bar(p, &tb));
        }
        ui.last_top = Some(tb);
    }

    let ledgers = all_ledgers(s, &ui.hist);
    let open_tab = ui.tab;
    if ui.last_ledgers.as_ref() != Some(&ledgers) || ui.last_detail.as_ref().map(|d| d.tab) != open_tab {
        if let Ok(e) = column.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| {
                for c in &ledgers {
                    spawn_compact(p, c, Some(c.tab) == open_tab);
                }
            });
        }
        ui.last_ledgers = Some(ledgers);
    }

    let Some(tab) = ui.tab else {
        ui.last_detail = None;
        return;
    };
    let mut tables = detail_tables(s, &ui.hist, tab, ui.filter);
    let mut sorts = Vec::new();
    for (i, t) in tables.iter_mut().enumerate() {
        let st = ui.sort.get(&(tab, i)).copied();
        if let Some((c, asc)) = st {
            sort_rows(&mut t.rows, c, asc);
        }
        sorts.push(st);
    }
    let snap = DetailSnap { tab, filter: ui.filter, tables, sorts, status: ui.status.clone() };
    if ui.last_detail.as_ref() == Some(&snap) {
        return;
    }
    let tab_changed = ui.last_detail.as_ref().map(|d| (d.tab, d.filter)) != Some((snap.tab, snap.filter));
    if tab_changed || ui.last_detail.is_none() {
        if let Ok(e) = header.single() {
            commands.entity(e).despawn_children();
            commands.entity(e).with_children(|p| {
                for t in Tab::ALL {
                    pill(p, t.label(), Act::Open(t), t == tab, 4);
                }
                p.spawn(Node { flex_grow: 1.0, ..default() });
                pill(p, "X", Act::Close, false, 4);
                if tab == Tab::Log {
                    p.spawn(Node { width: percent(100), column_gap: px(4), ..default() }).with_children(|f| {
                        text(f, "Show:", FONT, DIM);
                        for fl in LogFilter::ALL {
                            pill(f, fl.label(), Act::Filter(fl), fl == ui.filter, 4);
                        }
                    });
                }
            });
        }
    }
    if let Ok(e) = body.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            for (i, t) in snap.tables.iter().enumerate() {
                spawn_table(p, t, tab, i, snap.sorts[i]);
            }
        });
    }
    if let Ok(e) = footer.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| {
            let hint = if snap.status.is_empty() { "Click a column to sort, scroll for more, click a beaver to find it on the map.".to_string() } else { snap.status.clone() };
            text(p, hint, 11.0, if snap.status.is_empty() { DIM } else { INK });
        });
    }
    ui.last_detail = Some(snap);
}

/// Tells the world picking code which screen areas are UI.
fn publish_blockers(mut layout: ResMut<UiLayout>, q: Query<(&ComputedNode, &UiGlobalTransform, &Node), With<Blocker>>) {
    layout.extra.clear();
    for (n, t, node) in &q {
        if node.display == Display::None || n.size.x < 1.0 {
            continue;
        }
        let c = t.transform_point2(Vec2::ZERO) * n.inverse_scale_factor;
        let half = n.size * n.inverse_scale_factor / 2.0;
        layout.extra.push(Rect::from_corners(c - half, c + half));
    }
}

fn scroll_wheel(
    windows: Query<&Window, With<PrimaryWindow>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut q: Query<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<ScrollArea>>,
) {
    let Some(cursor) = windows.single().ok().and_then(|w| w.physical_cursor_position()) else { return };
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * 36.0,
        MouseScrollUnit::Pixel => scroll.delta.y,
    };
    if lines == 0.0 {
        return;
    }
    for (n, t, mut pos) in &mut q {
        if n.size.x > 1.0 && n.contains_point(*t, cursor) {
            let max = ((n.content_size.y - n.size.y) * n.inverse_scale_factor).max(0.0);
            pos.y = (pos.y - lines).clamp(0.0, max);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_clicks(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut press: Local<Option<Vec2>>,
    clickables: Query<(&Clickable, &ComputedNode, &UiGlobalTransform)>,
    scroll_areas: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
    mut scroll_pos: Query<&mut ScrollPosition, With<ScrollArea>>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
    mut ui: ResMut<LedgerUi>,
    mut cards: ResMut<CardState>,
    mut old_ui: ResMut<UiState>,
) {
    let Ok(w) = windows.single() else { return };
    let (Some(phys), Some(logical)) = (w.physical_cursor_position(), w.cursor_position()) else { return };
    if buttons.just_pressed(MouseButton::Left) {
        *press = Some(logical);
    }
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(start) = press.take() else { return };
    if start.distance(logical) > 5.0 {
        return;
    }
    let Some((act, _)) = hit_test(phys, &clickables, &scroll_areas) else { return };
    let mut reset_scroll = false;
    apply(act, &mut sim, &mut flow, &mut sel, &mut ui, &mut cards, &mut old_ui, &mut reset_scroll);
    ui.dirty = true;
    if reset_scroll {
        for mut p in &mut scroll_pos {
            p.y = 0.0;
        }
    }
}

/// Runs one click. Errors from the sim are shown in the detail footer, never swallowed.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    act: Act,
    sim: &mut Sim,
    flow: &mut Flow,
    sel: &mut Selection,
    ui: &mut LedgerUi,
    cards: &mut CardState,
    old_ui: &mut UiState,
    reset_scroll: &mut bool,
) {
    let s = &mut sim.0;
    match act {
        Act::Open(t) => {
            ui.tab = if ui.tab == Some(t) && t != Tab::Log { None } else { Some(t) };
            ui.status.clear();
            old_ui.ledger = false;
            *reset_scroll = true;
        }
        Act::Close => {
            ui.tab = None;
            ui.status.clear();
        }
        Act::Sort(code) => {
            if let Some(tab) = ui.tab {
                let key = (tab, code / 100);
                let col = code % 100;
                let next = match ui.sort.get(&key) {
                    Some((c, asc)) if *c == col => (col, !asc),
                    _ => (col, true),
                };
                ui.sort.insert(key, next);
            }
        }
        Act::Pick(id) => {
            if let Some(b) = s.colony.beaver(id) {
                *sel = Selection::Beaver(id);
                flow.focus = Some(cell_world(&s.height, b.pos));
                ui.status = format!("Following {} on the map.", b.name);
            } else {
                ui.status = "That beaver is gone.".into();
            }
        }
        Act::Accept(id) => {
            let name = s.region.offers().iter().find(|o| o.id == id).and_then(|o| s.region.colony(o.colony).ok()).map(|n| n.name.clone()).unwrap_or_default();
            let mut host = ColonyHost { colony: &mut s.colony };
            ui.status = match s.region.accept_offer(id, &mut host) {
                Ok(()) => format!("Trade with {name} done."),
                Err(e) => format!("Trade failed: {e}."),
            };
        }
        Act::Connect(id) => {
            ui.status = match s.region.connect(id, Link::Path) {
                Ok(()) => "Path opened; offers will follow.".into(),
                Err(e) => format!("Cannot connect: {e}."),
            };
        }
        Act::Jump(x, y) => {
            flow.focus = Some(cell_world(&s.height, (x, y)));
            *sel = Selection::Cell((x as usize, y as usize));
            ui.status = format!("Camera moved to ({x:.0}, {y:.0}).");
        }
        Act::Choose(id, opt) => {
            let mut host = ColonyHost { colony: &mut s.colony };
            ui.status = match s.events.resolve(id, opt, &mut host) {
                Ok(()) => "Decision made.".into(),
                Err(e) => format!("Cannot decide: {e}."),
            };
        }
        Act::Dismiss(id) => {
            cards.dismissed.insert(id);
        }
        Act::Filter(f) => ui.filter = f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::events::CardState;
    use sim::events::EventKind;
    use sim::region::Resource;

    struct Rig {
        sim: Sim,
        flow: Flow,
        sel: Selection,
        ui: LedgerUi,
        cards: CardState,
        old: UiState,
    }

    fn rig() -> Rig {
        let mut sim = Sim(sim::SimState::new(4));
        let mut flow = Flow::new(&sim.0.world);
        crate::midgame::build_midgame(&mut sim.0, &mut flow, &mut History::default()).expect("midgame");
        Rig { sim, flow, sel: Selection::None, ui: LedgerUi::default(), cards: CardState::default(), old: UiState::default() }
    }

    fn click(r: &mut Rig, a: Act) {
        let mut reset = false;
        apply(a, &mut r.sim, &mut r.flow, &mut r.sel, &mut r.ui, &mut r.cards, &mut r.old, &mut reset);
    }

    #[test]
    fn ledgers_click_opens_toggles_and_closes_detail() {
        let mut r = rig();
        r.old.ledger = true;
        click(&mut r, Act::Open(Tab::Stores));
        assert_eq!(r.ui.tab, Some(Tab::Stores));
        assert!(!r.old.ledger, "the old beaver ledger gives way");
        click(&mut r, Act::Open(Tab::Trade));
        assert_eq!(r.ui.tab, Some(Tab::Trade));
        click(&mut r, Act::Open(Tab::Trade));
        assert_eq!(r.ui.tab, None);
        click(&mut r, Act::Open(Tab::Log));
        click(&mut r, Act::Open(Tab::Log));
        assert_eq!(r.ui.tab, Some(Tab::Log), "the log button never toggles it shut");
        click(&mut r, Act::Close);
        assert_eq!(r.ui.tab, None);
    }

    #[test]
    fn ledgers_click_header_sorts_then_reverses() {
        let mut r = rig();
        click(&mut r, Act::Open(Tab::Families));
        click(&mut r, Act::Sort(4));
        assert_eq!(r.ui.sort[&(Tab::Families, 0)], (4, true));
        click(&mut r, Act::Sort(4));
        assert_eq!(r.ui.sort[&(Tab::Families, 0)], (4, false));
        click(&mut r, Act::Sort(102)); // second table, column 2
        assert_eq!(r.ui.sort[&(Tab::Families, 1)], (2, true));
    }

    #[test]
    fn ledgers_click_accept_trades_and_reports_failure() {
        let mut r = rig();
        let offer = r.sim.0.region.offers().first().cloned().expect("midgame has open offers");
        let have = |r: &Rig, res: Resource| match res {
            Resource::Food => r.sim.0.colony.food,
            Resource::Wood => r.sim.0.colony.wood,
        };
        let (give0, want0) = (have(&r, offer.give.0), have(&r, offer.want.0));
        click(&mut r, Act::Accept(offer.id));
        assert!(r.ui.status.starts_with("Trade with"), "{}", r.ui.status);
        // they give `give` of one resource and take `want` of the other (the two are always different goods)
        assert_ne!(offer.give.0, offer.want.0);
        assert!((have(&r, offer.give.0) - (give0 + offer.give.1)).abs() < 1e-3);
        assert!((have(&r, offer.want.0) - (want0 - offer.want.1)).abs() < 1e-3);
        assert!(r.sim.0.region.offers().iter().all(|o| o.id != offer.id), "offer consumed");
        click(&mut r, Act::Accept(offer.id));
        assert!(r.ui.status.starts_with("Trade failed"), "{}", r.ui.status);
        // cannot afford
        if let Some(o) = r.sim.0.region.offers().first().cloned() {
            r.sim.0.colony.food = 0.0;
            r.sim.0.colony.wood = 0.0;
            click(&mut r, Act::Accept(o.id));
            assert!(r.ui.status.contains("only have"), "{}", r.ui.status);
            assert!(r.sim.0.region.offers().iter().any(|x| x.id == o.id), "failed trade keeps the offer");
        }
    }

    #[test]
    fn ledgers_click_connect_opens_a_route() {
        let mut r = rig();
        let id = r.sim.0.region.colonies.iter().find(|n| !n.is_connected() && n.friendliness >= sim::region::WARY_BELOW).map(|n| n.id);
        if let Some(id) = id {
            click(&mut r, Act::Connect(id));
            assert!(r.sim.0.region.colony(id).unwrap().is_connected());
            click(&mut r, Act::Connect(id));
            assert!(r.ui.status.starts_with("Cannot connect"));
        }
    }

    #[test]
    fn ledgers_click_choose_resolves_pending_event() {
        let mut r = rig();
        let p = r.sim.0.events.pending().first().cloned().expect("midgame has a pending choice");
        click(&mut r, Act::Choose(p.id, 1));
        assert!(r.sim.0.events.pending().iter().all(|x| x.id != p.id));
        assert_eq!(r.ui.status, "Decision made.");
        click(&mut r, Act::Choose(p.id, 0));
        assert!(r.ui.status.starts_with("Cannot decide"));
        assert!(r.sim.0.events.log().iter().any(|e| e.id == p.id && e.kind == p.kind && e.choice.is_some()));
        let _ = EventKind::Fire;
    }

    #[test]
    fn ledgers_click_jump_and_pick_move_the_camera_focus() {
        let mut r = rig();
        click(&mut r, Act::Jump(10.0, 12.0));
        assert!(r.flow.focus.is_some());
        assert_eq!(r.sel, Selection::Cell((10, 12)));
        r.flow.focus = None;
        let id = r.sim.0.colony.beavers[0].id;
        click(&mut r, Act::Pick(id));
        assert_eq!(r.sel, Selection::Beaver(id));
        assert!(r.flow.focus.is_some());
        let gone = sim::colony::BeaverId(u64::MAX);
        click(&mut r, Act::Pick(gone));
        assert_eq!(r.ui.status, "That beaver is gone.");
    }

    #[test]
    fn ledgers_midgame_is_deterministic() {
        let a = rig();
        let b = rig();
        assert_eq!(a.sim.0.colony.population(), b.sim.0.colony.population());
        assert_eq!(a.sim.0.events.log(), b.sim.0.events.log());
        assert!(a.sim.0.events.pending().len() >= 2);
    }
}
