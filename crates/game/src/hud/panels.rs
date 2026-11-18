//! What each panel shows, as [`VNode`] trees (spec §5, §7, Appendix A). Words only where
//! Appendix A allows them; everything else is icons, numbers, bars and tooltips.
use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction, HudState, MapLens, Tool, CAT_KEY_LABELS};
use crate::build_logic::{self, Group, MAX_DAM_HEIGHT, MIN_DAM_HEIGHT};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use crate::sim_driver::{SimClock, Speed};
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Good, Status};
use sim::calendar::{self, Date, Season};
use sim::colony::Stage;
use sim::hud::{Breakdown, Fix, Problem, ProblemKind, Severity, Source, Subject};
use sim::jobs::RoleKind;


/// Everything a panel may read.
pub struct Ctx<'a> {
    pub s: &'a sim::SimState,
    pub hud: &'a HudState,
    pub clock: &'a SimClock,
    pub sel: Selection,
    pub lens: MapLens,
    pub hover: Option<Cell>,
    pub problems: &'a [Problem],
    pub minimap: Option<&'a Handle<Image>>,
    pub view_cell: (f32, f32),
    pub log: &'a std::collections::VecDeque<String>,
    /// (day, population, food) samples every five days.
    pub history: &'a [(f64, usize, f32)],
    pub cursor: Option<Vec2>,
}

impl Ctx<'_> {
    /// A placement or line tool is active (spec F3.3 build mode).
    pub fn placing(&self) -> bool {
        self.hud.tool != Tool::None
    }
}

/// Goods always in the strip; crafted ones join once the colony has any.
pub const STRIP_GOODS: [Good; 5] = [Good::Food, Good::Logs, Good::Sticks, Good::Mud, Good::Stone];
pub const RIGHT_W: f32 = 320.0;
/// Space the right column keeps free above the bottom edge (the dam profile docks below it).
pub const RIGHT_BOTTOM: f32 = 150.0;

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    let v = |x: Option<f32>| x.map_or(Val::Auto, Val::Px);
    Node { position_type: PositionType::Absolute, left: v(left), right: v(right), top: v(top), bottom: v(bottom), ..default() }
}

fn panel_col(node: Node, gap: f32, f: impl FnOnce(&mut Ui)) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(gap), ..node }, f);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v
}

fn panel_row(node: Node, gap: f32, f: impl FnOnce(&mut Ui)) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(gap), ..node }, f);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v
}

/// A transparent full-width strip that centres its child horizontally and lets the pointer through.
fn centred(bottom: f32, child: VNode) -> VNode {
    let mut v = VNode::new(Node { justify_content: JustifyContent::Center, ..abs(Some(0.0), Some(0.0), None, Some(bottom)) });
    v.children.push(child);
    v
}

pub fn food_days(s: &sim::SimState) -> Option<f32> {
    let need = sim::hud::food_need_per_day(s);
    let gain = s.ledger.in_per_day(Good::Food);
    (need > gain + 1e-3).then(|| s.buildings.stores.amount(Good::Food) / (need - gain))
}

fn rate_color(r: f32) -> Color {
    if r > 0.05 {
        GOOD
    } else if r < -0.05 {
        BAD
    } else {
        MUTED
    }
}

// ───────────────────────────── goods strip (A1) ─────────────────────────────

/// A sim breakdown as tooltip rows: label, signed value, the source's icon.
pub fn breakdown_lines(s: &sim::SimState, b: &Breakdown, signed_values: bool) -> Vec<TipLine> {
    b.rows
        .iter()
        .map(|r| {
            let v = if signed_values { signed(r.value) } else { num(r.value) };
            let col = if !signed_values { INK } else if r.value > 0.05 { GOOD } else if r.value < -0.05 { BAD } else { MUTED };
            let mut l = TipLine::new(r.label.clone(), v).color(col);
            match r.source {
                Source::Building(id) => {
                    if let Some(bb) = s.buildings.building(id) {
                        l = l.icon(building_icon(bb.kind), INK);
                    }
                }
                Source::Beaver(id) => {
                    if let Some(bv) = s.colony.beaver(id) {
                        l = l.icon(stage_icon(bv.stage), INK);
                    }
                }
                _ => {}
            }
            l
        })
        .collect()
}

pub fn good_tip(s: &sim::SimState, g: Good) -> Tip {
    let st = &s.buildings.stores;
    let mut t = Tip::titled(Some((good_icon(g), good_color(g))), good_name(g)).with_value(format!("{}/{}", st.amount(g).round() as i64, st.capacity(g).round() as i64));
    if !s.ledger.has_data() {
        t.push(TipLine::new("Per day", "-"));
        return t;
    }
    let inn = sim::hud::good_in_breakdown(s, g);
    let out = sim::hud::good_out_breakdown(s, g);
    let mut tin = Tip::titled(Some((good_icon(g), good_color(g))), "Gathered").with_value(format!("{}/d", signed(inn.total)));
    for l in breakdown_lines(s, &inn, true) {
        tin.push(l);
    }
    let mut tout = Tip::titled(Some((good_icon(g), good_color(g))), "Used").with_value(format!("{}/d", signed(-out.total.abs())));
    for mut l in breakdown_lines(s, &out, true) {
        if !l.value.starts_with('-') && l.value != "0" {
            l.value = format!("-{}", l.value.trim_start_matches('+'));
            l.color = Some(BAD);
        }
        tout.push(l);
    }
    t.push(TipLine::new("Gathered", format!("{}/d", signed(inn.total))).color(GOOD).nested(tin));
    t.push(TipLine::new(if g == Good::Food { "Eaten, used" } else { "Used" }, format!("{}/d", signed(-out.total.abs()))).color(BAD).nested(tout));
    let left = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, st.amount(g)) };
    if let Some(d) = left {
        t.push(TipLine::new("Empty in", days(d)).icon(Icon::Hourglass, WARN).color(WARN));
        if g == Good::Food {
            let gap = sim::hud::food_need_per_day(s) - inn.total;
            if gap > 0.05 {
                t.push(TipLine::new("Short", num(gap)).color(WARN));
            }
        }
    }
    t
}

fn headcount_tip(s: &sim::SimState) -> Tip {
    let h = sim::jobs::headcount(s);
    let mut t = Tip::titled(Some((Icon::Beaver, INK)), "Beavers").with_value(format!("{}", h.total));
    t.push(TipLine::new("Working", format!("{}", h.working)).icon(Icon::Jobs, INK));
    t.push(TipLine::new("Free", format!("{}", h.free)).icon(Icon::Idle, if h.free > 0 { WARN } else { INK }));
    t.push(TipLine::new("Kits", format!("{}", h.kits)).icon(Icon::Kit, MUTED));
    t.push(TipLine::new("Elders", format!("{}", h.elders)).icon(Icon::ElderBeaver, MUTED));
    let b = sim::hud::headcount_breakdown(s);
    let mut more = Tip::name("Beavers");
    for l in breakdown_lines(s, &b, false) {
        more.push(l);
    }
    t.push(TipLine::new("Housing", "").nested(more));
    t.key("Tab")
}

pub fn stage_icon(st: Stage) -> Icon {
    match st {
        Stage::Kit => Icon::Kit,
        Stage::Yearling => Icon::Yearling,
        Stage::Adult => Icon::Adult,
        Stage::Elder => Icon::ElderBeaver,
    }
}

pub fn idle_reason_word(r: sim::jobs::IdleReason) -> &'static str {
    use sim::jobs::IdleReason::*;
    match r {
        NoOpenSlot => "No open job",
        Unreachable => "Unreachable",
        Resting => "Resting",
        Paused => "Job paused",
        AutoOff => "Auto off",
    }
}

pub fn strip(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let stores = &s.buildings.stores;
    let extra = [Good::Baskets, Good::Rope, Good::Tools].into_iter().filter(|g| stores.amount(*g) > 0.0);
    let idle = sim::jobs::idle_reasons(s);
    vec![panel_row(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..abs(Some(12.0), None, Some(10.0), None) }, 14.0, |u| {
        for g in STRIP_GOODS.into_iter().chain(extra) {
            let have = stores.amount(g);
            let rate = s.ledger.net_per_day(g);
            let lasts = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, have) };
            u.row(4.0, |r| {
                r.icon(good_icon(g), 18.0, good_color(g));
                r.bold(format!("{}", have.round() as i64), 14.0, if have < 0.5 && lasts.is_some() { BAD } else { INK });
                if rate.abs() >= 0.05 {
                    r.icon(if rate > 0.0 { Icon::Up } else { Icon::Down }, 9.0, rate_color(rate));
                    r.text(format!("{:.1}", rate.abs()), 11.0, rate_color(rate));
                }
                if let Some(d) = lasts.filter(|d| *d < sim::hud::FOOD_WARN_DAYS) {
                    let col = if d < 7.0 { BAD } else { WARN };
                    r.icon(Icon::Hourglass, 11.0, col);
                    r.bold(days(d), 11.0, col);
                }
            })
            .tip(good_tip(s, g));
        }
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(18.0), ..default() })).bg(LINE);
        u.row(4.0, |r| {
            r.icon(Icon::Beaver, 18.0, INK);
            r.bold(format!("{}", s.colony.population()), 14.0, INK);
        })
        .tip(headcount_tip(s))
        .action(HudAction::Colony(Some(ColonyTab::Population)));
        // Free beavers who are not resting.
        let free: Vec<_> = idle.iter().filter(|(_, r)| *r != sim::jobs::IdleReason::Resting).collect();
        if !free.is_empty() {
            let mut t = Tip::titled(Some((Icon::Idle, WARN)), "Free").with_value(format!("{}", free.len()));
            for (id, why) in free.iter().take(8) {
                if let Some(b) = s.colony.beaver(*id) {
                    t.push(TipLine::new(b.name.clone(), idle_reason_word(*why)).icon(stage_icon(b.stage), INK).color(MUTED));
                }
            }
            let t = t.key("I").row(TipLine { label: "Jobs".into(), key: Some("J".into()), ..default() });
            u.icon_label_button(Icon::Idle, format!("{}", free.len()), 14.0, HudAction::CycleIdle, Color::srgba(0.94, 0.75, 0.29, 0.22), false).tip(t);
        }
    })]
}

// ───────────────────────────── calendar (A2) ─────────────────────────────

fn season_color(se: Season) -> Color {
    match se {
        Season::Spring => Color::srgb(0.5, 0.74, 0.42),
        Season::Summer => Color::srgb(0.86, 0.7, 0.36),
        Season::Autumn => Color::srgb(0.72, 0.5, 0.33),
        Season::Winter => Color::srgb(0.8, 0.84, 0.9),
    }
}

pub fn calendar(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let t = s.time_days;
    let d = Date::at(t);
    let clock = c.clock;
    vec![panel_col(Node { width: Val::Px(270.0), ..abs(None, Some(12.0), Some(10.0), None) }, 6.0, |u| {
        u.row(6.0, |r| {
            r.row(5.0, |x| {
                x.icon(season_icon(d.season), 18.0, season_color(d.season));
                x.bold(format!("{}", d.day), 15.0, INK);
            })
            .tip(Tip::titled(Some((season_icon(d.season), season_color(d.season))), d.season.name()).with_value(format!("Year {}", d.year)));
            r.spacer();
            r.icon_button(Icon::Pause, 14.0, HudAction::Pause, FAINT, clock.paused).tip(Tip::name("Pause").key("Space"));
            for (sp, icon, key) in [(Speed::X1, Icon::Play, "1"), (Speed::X2, Icon::Fast2, "2"), (Speed::X3, Icon::Fast3, "3")] {
                r.icon_button(icon, 14.0, HudAction::Speed(sp), FAINT, !clock.paused && clock.speed == sp).tip(Tip::name(format!("Speed x{key}")).key(key));
            }
        });
        // Year ribbon: four seasons, a tick for today.
        let phase = calendar::year_phase(t);
        u.node(Node { width: Val::Percent(100.0), height: Val::Px(8.0), flex_direction: FlexDirection::Row, column_gap: Val::Px(2.0), ..default() }, |r| {
            for se in Season::ALL {
                r.add(VNode::new(Node { flex_grow: 1.0, height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() })).bg(season_color(se).with_alpha(0.75));
            }
            r.add(VNode::new(Node { position_type: PositionType::Absolute, left: Val::Percent(phase * 100.0), top: Val::Px(-3.0), width: Val::Px(2.0), height: Val::Px(14.0), ..default() })).bg(SELECT);
        });
        u.row(10.0, |r| {
            // Next thing that matters, as an icon and a count of days.
            let (icon, col, n, tip) = match d.season {
                Season::Summer => (Icon::Autumn, WARN, calendar::days_until(t, Season::Autumn), Tip::titled(Some((Icon::DrySummer, WARN)), "Dry summer").line("Autumn in", days(calendar::days_until(t, Season::Autumn).ceil() as f32))),
                Season::Winter => (Icon::Spring, MUTED, calendar::days_until(t, Season::Spring), Tip::titled(Some((Icon::Winter, INK)), "Winter: low inflow").line("Spring in", days(calendar::days_until(t, Season::Spring).ceil() as f32))),
                _ => (Icon::DrySummer, WARN, calendar::days_until(t, Season::Summer), Tip::titled(Some((Icon::DrySummer, WARN)), "Dry summer").line("In", days(calendar::days_until(t, Season::Summer).ceil() as f32))),
            };
            r.row(4.0, |x| {
                x.icon(icon, 15.0, col);
                x.bold(days(n.ceil() as f32), 12.0, col);
            })
            .tip(tip);
            r.spacer();
            // Pond cover gauge: stored water against a dry summer.
            let ponds = sim::hud::ponds(s);
            let season = calendar::SEASON_DAYS as f32;
            match ponds.summer_cover_days() {
                Some(cover) => {
                    let ok = cover >= season;
                    let tip = Tip::titled(Some((Icon::Gauge, WATER)), "Ponds").with_value(num(ponds.volume)).line("Cover", format!("{} / {}", days(cover.min(999.0)), days(season))).line("Summer loss", format!("{}/d", num(ponds.summer_loss_per_day)));
                    r.row(4.0, |x| {
                        x.icon(Icon::Gauge, 15.0, WATER);
                        x.bar(cover / season, 56.0, 7.0, WATER);
                        if !ok {
                            x.icon(Icon::Warn, 10.0, WARN);
                        }
                        x.bold(days(cover.min(999.0)), 12.0, if ok { INK } else { WARN });
                    })
                    .tip(tip);
                }
                None => {
                    r.row(4.0, |x| {
                        x.icon(Icon::Gauge, 15.0, MUTED);
                        x.add(VNode::new(Node { width: Val::Px(56.0), height: Val::Px(7.0), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() })).border(BorderColor::all(MUTED));
                    })
                    .tip(Tip::titled(Some((Icon::Gauge, MUTED)), "No ponds"));
                }
            }
        });
    })]
}

// ───────────────────────────── alerts (A3) ─────────────────────────────

pub fn role_kind_icon(r: RoleKind) -> Icon {
    match r {
        RoleKind::Forager => Icon::RoleForager,
        RoleKind::Logger => Icon::RoleLogger,
        RoleKind::MudDigger => Icon::RoleMudDigger,
        RoleKind::Quarrier => Icon::RoleQuarrier,
        RoleKind::Builder => Icon::RoleBuilder,
        RoleKind::Hauler => Icon::RoleHauler,
        RoleKind::Crafter => Icon::RoleCrafter,
        RoleKind::Elder => Icon::RoleElder,
        RoleKind::Forester => Icon::RoleForester,
    }
}

pub fn role_kind_name(r: RoleKind) -> &'static str {
    match r {
        RoleKind::Forager => "Forager",
        RoleKind::Logger => "Logger",
        RoleKind::MudDigger => "Mud digger",
        RoleKind::Quarrier => "Quarrier",
        RoleKind::Builder => "Builder",
        RoleKind::Hauler => "Hauler",
        RoleKind::Crafter => "Crafter",
        RoleKind::Elder => "Elder",
        RoleKind::Forester => "Forester",
    }
}

/// The icons and number an alert shows instead of a title (A3).
fn alert_face(s: &sim::SimState, p: &Problem) -> (Vec<(Icon, Color)>, String, &'static str) {
    let v = p.value;
    match p.kind {
        ProblemKind::Food => (vec![(Icon::Food, good_color(Good::Food))], if v < 0.5 { "0".into() } else { days(v) }, "Food runs out"),
        ProblemKind::Idle => (vec![(Icon::Idle, WARN)], format!("{}", v as i64), "Free beavers"),
        ProblemKind::NoBuilders => (vec![(Icon::Site, INK), (Icon::RoleBuilder, MUTED)], format!("{}", v as i64), "Sites without builders"),
        ProblemKind::StoreFull => {
            let g = match p.subject {
                Some(Subject::Good(g)) => g,
                _ => Good::Logs,
            };
            (vec![(good_icon(g), good_color(g)), (Icon::Full, WARN)], format!("{:.0}%", v), "Store full")
        }
        ProblemKind::WeakDam => (vec![(Icon::Dam, INK)], format!("{:.0}%", v), "Dam weakening"),
        ProblemKind::DamFailed => (vec![(Icon::Dam, INK), (Icon::Cross, BAD)], String::new(), "Dam failed"),
        ProblemKind::LoggingAreaRunningOut => (vec![(Icon::LoggingCamp, INK), (Icon::Tree, WARN)], format!("{}", v as i64), "Logging area running out"),
        ProblemKind::ForestFalling => (vec![(Icon::Tree, INK), (Icon::Down, WARN)], format!("-{:.0}%", v), "Forest falling"),
        ProblemKind::Stranger => {
            let st = match p.subject {
                Some(Subject::Stage(st)) => stage_icon(st),
                _ => Icon::Beaver,
            };
            let name = match p.fix {
                Fix::Accept(id) => s.society.pending().iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default(),
                _ => String::new(),
            };
            (vec![(st, INK)], name, "Asks to join")
        }
    }
}

/// The face of a fix button and its tooltip (≤ 6 words).
fn fix_face(f: &Fix) -> Option<(Icon, String, String)> {
    match f {
        Fix::AddWorkers { n, .. } => Some((Icon::Plus, format!("{n}"), format!("Add {n} workers"))),
        Fix::Gather { kind, n } => Some((resource_icon(*kind), format!("+{n}"), format!("{n} gather {}", resource_name(*kind).to_lowercase()))),
        Fix::Place(k) => Some((building_icon(*k), "+".into(), format!("Place {}", k.spec().name.to_lowercase()))),
        Fix::StaffSites => Some((Icon::RoleBuilder, "+".into(), "Staff the sites".into())),
        Fix::PutToWork { role, n } => Some((role_kind_icon(*role), format!("+{n}"), format!("{n} to {}", role_kind_name(*role).to_lowercase()))),
        Fix::OpenJobs => Some((Icon::Jobs, String::new(), "Open jobs".into())),
        Fix::Accept(_) => Some((Icon::Check, String::new(), "Welcome".into())),
        Fix::Decline(_) => Some((Icon::Cross, String::new(), "Turn away".into())),
        Fix::Focus => None,
    }
}

pub fn alerts(c: &Ctx, u: &mut Ui) {
    for p in c.problems {
        let col = if p.severity == Severity::Urgent { BAD } else { WARN };
        let (icons, value, name) = alert_face(c.s, p);
        let mut cause = Tip::titled(icons.first().copied(), name).with_value(value.clone());
        for l in breakdown_lines(c.s, &p.cause, p.kind == ProblemKind::Food) {
            cause.push(l);
        }
        let card = u.row(6.0, |r| {
            r.icon(if p.severity == Severity::Urgent { Icon::Urgent } else { Icon::Warn }, 10.0, col);
            for (i, ic) in &icons {
                r.icon(*i, 18.0, *ic);
            }
            if !value.is_empty() {
                r.bold(value.clone(), 14.0, INK);
            }
            r.spacer();
            for (k, f) in std::iter::once(&p.fix).chain(p.alt_fix.iter()).enumerate() {
                if let Some((icon, label, tip)) = fix_face(f) {
                    let fill = if k == 0 { ACCENT } else { FAINT };
                    let b = if label.is_empty() { r.icon_button(icon, 14.0, HudAction::Fix(*f, p.focus), fill, false) } else { r.icon_label_button(icon, label, 14.0, HudAction::Fix(*f, p.focus), fill, false) };
                    b.tip(Tip::name(tip));
                }
            }
            if let Some(f) = p.focus {
                r.icon_button(Icon::Focus, 14.0, HudAction::Focus(f), FAINT, false).tip(Tip::name("Show"));
            }
        });
        card.node.padding = UiRect::axes(Val::Px(8.0), Val::Px(6.0));
        card.node.border = UiRect::left(Val::Px(3.0));
        card.node.border_radius = BorderRadius::all(Val::Px(6.0));
        card.node.min_width = Val::Px(180.0);
        card.bg = PANEL;
        card.border = Some(BorderColor::all(col));
        card.tip(cause);
    }
}

// ───────────────────────────── right column ─────────────────────────────

/// Alerts at rest, the inspector on a selection, the line-tool card while drawing.
pub fn right(c: &Ctx) -> Vec<VNode> {
    let bottom = if c.placing() { RIGHT_BOTTOM } else { 90.0 };
    let mut col = VNode::new(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: Val::Px(6.0), overflow: Overflow::clip(), ..abs(None, Some(12.0), Some(112.0), Some(bottom)) });
    let mut ui = Ui::default();
    match c.hud.tool {
        Tool::Dam | Tool::Canal => {
            ui.add(line_card(c));
        }
        Tool::Building(_) => {}
        Tool::None => {
            if c.hud.colony.is_none() {
                // Pinned inspectors stay open above the current one (spec 4.2).
                for p in c.hud.pinned.iter().filter(|p| **p != c.sel) {
                    if let Some(v) = super::inspect::inspector(c, *p) {
                        ui.add(v);
                    }
                }
                match c.sel {
                    Selection::None => {
                        if c.hud.pinned.is_empty() {
                            alerts(c, &mut ui)
                        }
                    }
                    sel => {
                        if let Some(v) = super::inspect::inspector(c, sel) {
                            ui.add(v);
                        }
                    }
                }
            }
        }
    }
    if ui.nodes.is_empty() {
        return Vec::new();
    }
    col.children = ui.nodes;
    vec![col]
}

// ───────────────────────────── view rail (A10) ─────────────────────────────

pub fn rail(c: &Ctx) -> Vec<VNode> {
    vec![panel_col(Node { padding: UiRect::all(Val::Px(5.0)), align_items: AlignItems::Center, ..abs(Some(12.0), None, Some(64.0), None) }, 4.0, |u| {
        for (l, icon, name, key) in [(MapLens::World, Icon::ViewWorld, "World", "F1"), (MapLens::Water, Icon::ViewWater, "Water", "F2"), (MapLens::Goods, Icon::ViewGoods, "Goods", "F3"), (MapLens::Forest, Icon::ViewForest, "Forest", "F4"), (MapLens::Elevation, Icon::ViewElevation, "Elevation", "F5")] {
            let on = c.lens == l;
            u.icon_button(icon, 18.0, HudAction::SetLens(l), if on { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }, on).tip(Tip::name(name).key(key).row(TipLine { label: "Peek water".into(), key: Some("Alt".into()), ..default() }));
        }
        if c.lens == MapLens::Elevation {
            elevation_legend(c, u);
        }
        u.rule().style(|n| n.width = Val::Px(24.0));
        let open = c.hud.colony.is_some();
        u.icon_button(Icon::Colony, 18.0, HudAction::Colony(if open { None } else { Some(ColonyTab::Population) }), FAINT, open).tip(Tip::name("Colony").key("Tab"));
        let jobs = c.hud.colony == Some(ColonyTab::Jobs);
        u.icon_button(Icon::Jobs, 18.0, HudAction::Colony(if jobs { None } else { Some(ColonyTab::Jobs) }), FAINT, jobs).tip(Tip::name("Jobs").key("J"));
    })]
}

/// A contour step as written in the legend: `0.25`, `0.5`, `2`.
fn step_label(v: f32) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The elevation lens legend under its button: the height ramp between the map's lowest and
/// highest ground, then the contour interval (thin line) and every fourth line (thick).
fn elevation_legend(c: &Ctx, u: &mut Ui) {
    use super::elevation::{interval_for, surface_range, MAJOR_EVERY};
    let (lo, hi) = surface_range(c.s);
    let step = interval_for(hi - lo);
    let (tlo, thi) = (crate::terrain_render::min_height(c.s), crate::terrain_render::max_height(c.s));
    u.rule().style(|n| n.width = Val::Px(24.0));
    u.col(1.0, |l| {
        l.text(num(thi), 10.0, MUTED);
        for i in (0..5).rev() {
            let k = crate::terrain_render::elevation_ramp(i as f32 / 4.0);
            l.swatch(Color::srgb(k[0], k[1], k[2]), 16.0, 7.0);
        }
        l.text(num(tlo), 10.0, MUTED);
    })
    .style(|n| n.align_items = AlignItems::Center)
    .tip(Tip::titled(Some((Icon::ViewElevation, INK)), "Height").line("Low", num(tlo)).line("High", num(thi)));
    let w = crate::terrain_render::ELEVATION_WATER;
    u.swatch(Color::srgb(w[0], w[1], w[2]), 16.0, 7.0).tip(Tip::titled(Some((Icon::Drop, WATER)), "Water"));
    for (thick, every) in [(1.5, step), (3.0, step * MAJOR_EVERY as f32)] {
        u.col(2.0, |r| {
            r.swatch(INK, 16.0, thick);
            r.bold(step_label(every), 10.0, INK);
        })
        .style(|n| n.align_items = AlignItems::Center)
        .tip(Tip::name("Contour").line("Every", step_label(every)));
    }
}

// ───────────────────────────── build bar, items, search (A4) ─────────────────────────────

pub fn categories(s: &sim::SimState) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for e in build_logic::menu(s) {
        if !out.contains(&e.group) {
            out.push(e.group);
        }
    }
    out
}

fn cost_row(u: &mut Ui, cost: &[(Good, f32)], size: f32, col: Color) {
    u.row(6.0, |r| {
        for (g, a) in cost {
            r.row(2.0, |x| {
                x.icon(good_icon(*g), size, good_color(*g));
                x.bold(num(*a), size - 1.0, col);
            });
        }
    });
}

fn item_cost(kind: BuildingKind) -> Vec<(Good, f32)> {
    match kind {
        BuildingKind::Dam | BuildingKind::Canal => build_logic::path_cost(kind, 1),
        k => k.spec().cost.to_vec(),
    }
}

fn item_tip(s: &sim::SimState, e: &build_logic::MenuEntry) -> Tip {
    let kind = e.kind;
    let mut t = Tip::titled(Some((building_icon(kind), INK)), kind.spec().name);
    if e.unlocked {
        let cost = item_cost(kind);
        if matches!(kind, BuildingKind::Dam | BuildingKind::Canal) {
            t.push(TipLine::new("Per cell", ""));
        }
        for (g, a) in cost {
            t.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)).sub());
        }
    } else {
        let gaps = s.tech.missing(kind, &s.colony);
        let tech = e.need.map(|n| n.0).unwrap_or("");
        t.push(TipLine::new(tech, "").icon(Icon::Padlock, MUTED));
        for g in gaps {
            t.push(gap_line(&g).sub());
        }
    }
    t
}

pub fn gap_line(g: &sim::tech::Gap) -> TipLine {
    use sim::tech::{Achievement, Gap};
    match g {
        Gap::Achievement(a) => {
            let (icon, label) = match a {
                Achievement::FirstBurrow => (Icon::Burrow, "First burrow"),
                Achievement::FirstDam => (Icon::Dam, "First dam"),
                Achievement::FirstWinter => (Icon::Winter, "First winter"),
            };
            TipLine::new(label, "").icon(icon, INK)
        }
        Gap::Research(n) => TipLine::new(*n, "").icon(Icon::Padlock, MUTED),
        Gap::Size { have, need } => TipLine::new("Beavers", format!("{have}/{need}")).icon(Icon::Beaver, INK),
        Gap::Experience { have, need, .. } => TipLine::new("Experience", format!("{}/{}", num(*have), num(*need))).icon(Icon::Hammer, INK),
        Gap::Elder => TipLine::new("An elder", "").icon(Icon::ElderBeaver, INK),
        Gap::Lore { have, need } => TipLine::new("Lore", format!("{}/{}", num(*have), num(*need))).icon(Icon::Journal, INK),
        _ => TipLine::new("-", "").icon(Icon::Padlock, MUTED),
    }
}

fn item_tile(u: &mut Ui, s: &sim::SimState, e: &build_logic::MenuEntry, selected: bool) {
    let kind = e.kind;
    let tile = u.col(3.0, |t| {
        t.row(4.0, |r| {
            r.icon(building_icon(kind), 30.0, if e.unlocked { INK } else { MUTED });
            if !e.unlocked {
                r.icon(Icon::Padlock, 13.0, MUTED);
            }
        });
        if e.unlocked {
            cost_row(t, &item_cost(kind), 12.0, MUTED);
        } else if let Some((_, need, have)) = e.need.filter(|n| n.1 > 0.0) {
            t.bar(have / need, 60.0, 4.0, MUTED);
        }
    });
    tile.node.padding = UiRect::all(Val::Px(6.0));
    tile.node.align_items = AlignItems::Center;
    tile.node.min_width = Val::Px(64.0);
    tile.node.border = UiRect::all(Val::Px(if selected { 2.0 } else { 1.0 }));
    tile.node.border_radius = BorderRadius::all(Val::Px(5.0));
    tile.bg = if selected { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT };
    tile.border = Some(BorderColor::all(if selected { SELECT } else { Color::NONE }));
    tile.action(HudAction::Pick(kind)).tip(item_tip(s, e));
}

pub fn search_matches(s: &sim::SimState, q: &str) -> Vec<build_logic::MenuEntry> {
    super::search_matches(s, q)
}

pub fn bottom(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let hud = c.hud;
    let mut out = Vec::new();
    if c.placing() {
        // Build mode: one chip with the tool's icon, its keys and close.
        if hud.tool == Tool::Dam {
            return out; // the profile takes the bottom edge and carries the keys
        }
        let icon = match hud.tool {
            Tool::Building(k) => building_icon(k),
            Tool::Canal => Icon::Canal,
            _ => Icon::Dam,
        };
        let name = match hud.tool {
            Tool::Building(k) => k.spec().name,
            _ => "Canal",
        };
        out.push(centred(12.0, panel_row(Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)), ..default() }, 10.0, |u| {
            u.icon(icon, 24.0, INK).tip(Tip::name(name));
            key_hints(u, hud.tool);
            u.icon_button(Icon::Cross, 13.0, HudAction::CancelTool, FAINT, false).tip(Tip::name("Done").key("Esc"));
        })));
        return out;
    }
    let cats = categories(s);
    out.push(centred(12.0, panel_row(Node { padding: UiRect::all(Val::Px(6.0)), ..default() }, 6.0, |u| {
        for (i, g) in cats.iter().enumerate() {
            let open = hud.open_cat == Some(i);
            u.icon_button(group_icon(*g), 22.0, HudAction::OpenCat(i), if open { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }, open).tip(Tip::name(g.name()).key(CAT_KEY_LABELS.get(i).copied().unwrap_or("")));
        }
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(26.0), margin: UiRect::horizontal(Val::Px(2.0)), ..default() })).bg(LINE);
        u.icon_button(Icon::Search, 18.0, HudAction::OpenSearch, FAINT, hud.search.is_some()).tip(Tip::name("Search").key("/"));
        let can_undo = !hud.undo_is_empty();
        u.icon_button(Icon::Undo, 18.0, HudAction::Undo, FAINT, false).tip(Tip::name("Undo").key("Ctrl+Z")).style(|_| {}).bg = if can_undo { FAINT } else { Color::NONE };
    })));
    // Items above the bar: the open category or the search results.
    let entries: Option<(Option<String>, Vec<build_logic::MenuEntry>)> = if let Some(q) = &hud.search {
        Some((Some(q.clone()), search_matches(s, q)))
    } else {
        hud.open_cat.and_then(|i| cats.get(i).copied()).map(|g| (None, build_logic::menu(s).into_iter().filter(|e| e.group == g).collect()))
    };
    if let Some((query, entries)) = entries {
        out.push(centred(72.0, panel_row(Node { padding: UiRect::all(Val::Px(6.0)), ..default() }, 6.0, |u| {
            if let Some(q) = &query {
                u.row(4.0, |r| {
                    r.icon(Icon::Search, 16.0, MUTED);
                    r.bold(format!("{q}_"), 13.0, INK);
                })
                .style(|n| n.min_width = Val::Px(90.0));
                if entries.is_empty() {
                    u.text("None", 12.0, MUTED);
                }
            }
            for e in &entries {
                let selected = match hud.tool {
                    Tool::Building(k) => k == e.kind,
                    Tool::Dam => e.kind == BuildingKind::Dam,
                    Tool::Canal => e.kind == BuildingKind::Canal,
                    Tool::None => false,
                };
                item_tile(u, s, e, selected);
            }
        })));
    }
    out
}

/// Keycaps for the active tool: place, cancel, undo (and crest, confirm for line tools).
fn key_hints(u: &mut Ui, tool: Tool) {
    let pairs: Vec<(Icon, &str)> = match tool {
        Tool::Dam => vec![(Icon::Check, "Enter"), (Icon::Crest, "[ ]"), (Icon::Cross, "RMB"), (Icon::Undo, "Ctrl+Z")],
        Tool::Canal => vec![(Icon::Check, "Enter"), (Icon::Cross, "RMB"), (Icon::Undo, "Ctrl+Z")],
        _ => vec![(Icon::Check, "LMB"), (Icon::Cross, "RMB"), (Icon::Undo, "Ctrl+Z")],
    };
    u.row(8.0, |r| {
        for (i, k) in pairs {
            r.row(3.0, |x| {
                x.icon(i, 12.0, MUTED);
                x.keycap(k);
            });
        }
    });
}

// ───────────────────────────── choosing where to settle ─────────────────────────────

/// Before the colony exists: one chip with the family, what it carries, and the click that settles.
pub fn settle_chip(c: &Ctx) -> Vec<VNode> {
    let food = sim::hud::food_need_per_day(c.s);
    let mut bundle: Vec<(Good, String)> = vec![(Good::Food, days(sim::starts::START_LARDER_DAYS))];
    for (g, a) in sim::starts::START_BUNDLE {
        bundle.push((g, num(a)));
    }
    let _ = food;
    vec![centred(12.0, panel_row(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..default() }, 12.0, |u| {
        u.row(4.0, |r| {
            r.icon(Icon::Beaver, 22.0, INK);
            r.bold("4", 14.0, INK);
        })
        .tip(Tip::titled(Some((Icon::Beaver, INK)), "The family").line("Adults", "2").line("Young", "2"));
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(22.0), ..default() })).bg(LINE);
        u.row(8.0, |r| {
            for (g, a) in &bundle {
                r.row(2.0, |x| {
                    x.icon(good_icon(*g), 16.0, good_color(*g));
                    x.bold(a.clone(), 12.0, INK);
                })
                .tip(Tip::name(good_name(*g)));
            }
        })
        .tip({
            let mut t = Tip::titled(Some((Icon::Haul, INK)), "What they carry");
            t.push(TipLine::new("Dam", "").icon(Icon::Dam, INK));
            t.push(TipLine::new("Shelter", "").icon(Icon::Burrow, INK));
            t.push(TipLine::new("Store", "").icon(Icon::Warehouse, INK));
            t
        });
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(22.0), ..default() })).bg(LINE);
        u.row(4.0, |r| {
            r.icon(Icon::Home, 18.0, ACCENT);
            r.keycap("LMB");
        })
        .tip(Tip::name("Settle here").line("Pick the spot", ""));
    }))]
}

/// At the cursor while choosing: whether the family can settle here and how good the spot is.
pub fn settle_tag(s: &sim::SimState, flow: &crate::flow::Flow, cell: Cell, cur: Vec2) -> Vec<VNode> {
    let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(5.0)), ..abs(Some(cur.x + 18.0), None, Some(cur.y + 14.0), None) });
    v.bg = Color::srgba(0.08, 0.07, 0.06, 0.88);
    let mut ui = Ui::default();
    let _ = flow;
    match crate::flow::can_settle(s, cell) {
        Ok(()) => {
            let h = crate::flow::site_hint(s, cell);
            let q = h.overall();
            let col = if q >= 0.5 { GOOD } else if q >= 0.3 { WARN } else { BAD };
            ui.icon(Icon::Home, 14.0, col);
            for (i, x) in [(Icon::Drop, h.water), (Icon::Tree, h.trees), (Icon::Health, h.safety), (Icon::Building, h.room)] {
                ui.icon(i, 12.0, MUTED);
                ui.bar(x, 22.0, 4.0, if x >= 0.5 { GOOD } else if x >= 0.3 { WARN } else { BAD });
            }
            ui.bold(h.verdict(), 12.0, col);
        }
        Err(e) => {
            ui.icon(Icon::Cross, 14.0, BAD);
            ui.bold(super::settle_error(e), 12.0, INK);
        }
    }
    v.children = ui.nodes;
    vec![v]
}

// ───────────────────────────── cursor tag (5.2) ─────────────────────────────

pub fn cursor_tag(c: &Ctx) -> Vec<VNode> {
    if let (Tool::None, MapLens::Elevation, Some(cell), Some(cur)) = (c.hud.tool, c.lens, c.hover, c.cursor) {
        return elevation_tag(c.s, cell, cur);
    }
    let (Tool::Building(kind), Some(cell), Some(cur)) = (c.hud.tool, c.hover, c.cursor) else { return Vec::new() };
    let pv = build_logic::site_preview(c.s, kind, cell);
    let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(5.0)), ..abs(Some(cur.x + 18.0), None, Some(cur.y + 14.0), None) });
    v.bg = Color::srgba(0.08, 0.07, 0.06, 0.88);
    let mut ui = Ui::default();
    match &pv.verdict {
        Ok(()) => {
            cost_row(&mut ui, kind.spec().cost, 13.0, INK);
            ui.icon(Icon::Check, 14.0, GOOD);
        }
        Err(e) => {
            ui.icon(Icon::Cross, 14.0, BAD);
            let w = place_error(e);
            if matches!(e, build_logic::PlaceError::Locked(..)) {
                ui.icon(Icon::Padlock, 13.0, MUTED);
            }
            if !w.is_empty() {
                ui.bold(w, 12.0, INK);
            }
        }
    }
    v.children = ui.nodes;
    vec![v]
}

/// At the cursor in the elevation lens: the ground height, a dam's crest where there is one, and
/// the water surface where the cell is wet. Numbers and icons only.
pub fn elevation_tag(s: &sim::SimState, cell: Cell, cur: Vec2) -> Vec<VNode> {
    let get = |g: &sim::Grid<f32>| g.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let (ground, dam, depth) = (get(&s.height), get(&s.water.dam_height), get(&s.water.depth));
    let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), column_gap: Val::Px(5.0), align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(5.0)), ..abs(Some(cur.x + 18.0), None, Some(cur.y + 14.0), None) });
    v.bg = Color::srgba(0.08, 0.07, 0.06, 0.88);
    let mut ui = Ui::default();
    ui.icon(Icon::ViewElevation, 13.0, MUTED);
    ui.bold(format!("{ground:.1}"), 12.0, INK);
    if dam > 0.0 {
        ui.icon(Icon::Dam, 13.0, MUTED);
        ui.bold(format!("{:.1}", ground + dam), 12.0, INK);
    }
    if depth > crate::terrain_render::WET_EPS {
        ui.icon(Icon::Drop, 13.0, WATER);
        ui.bold(format!("{:.1}", ground + dam + depth), 12.0, WATER);
    }
    v.children = ui.nodes;
    vec![v]
}

// ───────────────────────────── line-tool card (A5) ─────────────────────────────

fn card_row(u: &mut Ui, label: &str, value: String, col: Color, tip: Tip) {
    u.row(8.0, |r| {
        r.text(label, 12.0, MUTED);
        r.spacer();
        r.bold(value, 13.0, col);
    })
    .tip(tip);
}

pub fn line_card(c: &Ctx) -> VNode {
    let s = c.s;
    let hud = c.hud;
    let n = hud.line.len();
    panel_col(Node { width: Val::Px(RIGHT_W), ..default() }, 6.0, |u| {
        let is_dam = hud.tool == Tool::Dam;
        u.row(6.0, |r| {
            r.icon(if is_dam { Icon::Dam } else { Icon::Canal }, 20.0, INK).tip(Tip::name(if is_dam { "Dam" } else { "Canal" }));
            if n > 0 {
                r.bold(format!("{n}"), 15.0, INK).tip(Tip::name(format!("{n} cells")));
            }
            r.spacer();
            r.icon_button(Icon::Cross, 12.0, HudAction::CancelTool, FAINT, false).tip(Tip::name("Done").key("Esc"));
        });
        if is_dam {
            u.row(6.0, |r| {
                r.icon(Icon::Crest, 16.0, MUTED);
                r.icon_button(Icon::Minus, 11.0, HudAction::DamHeight(-0.25), FAINT, false).tip(Tip::name("Lower").key("["));
                r.bold(num(hud.dam_height), 14.0, INK).style(|nn| nn.min_width = Val::Px(30.0));
                r.icon_button(Icon::Plus, 11.0, HudAction::DamHeight(0.25), FAINT, false).tip(Tip::name("Raise").key("]"));
            })
            .tip(Tip::name(format!("Crest {}-{}", num(MIN_DAM_HEIGHT), num(MAX_DAM_HEIGHT))).key("Ctrl+wheel"));
        }
        if n == 0 {
            return;
        }
        let check = if is_dam { build_logic::check_dam(s, &hud.line, hud.dam_height) } else { build_logic::check_canal(s, &hud.line) };
        if let Err(e) = &check {
            let w = place_error(e);
            u.row(6.0, |r| {
                r.icon(Icon::Cross, 13.0, BAD);
                if !w.is_empty() {
                    r.bold(w, 12.0, INK);
                }
            });
        }
        if is_dam {
            match c.hud.dam_preview() {
                Some(pv) => {
                    let r = build_logic::DamReadout::from_preview(pv);
                    let new = pv.flooded.iter().filter(|&&(x, y)| !build_logic::is_wet(s, x, y)).count();
                    card_row(u, "Rise", format!("+{}", num(r.rise)), WATER, Tip::name("Water rises").line("Level", num(r.level)));
                    card_row(u, "Flood", format!("{new}"), INK, Tip::name("Newly flooded").line("Under water", format!("{}", r.flooded_cells)));
                    card_row(u, "Trees", format!("{}", r.flooded_trees), if r.flooded_trees > 0 { WARN } else { INK }, Tip::name("Trees drowned"));
                    u.row(8.0, |x| {
                        x.spacer();
                        for (g, a) in &r.cost {
                            x.row(2.0, |y| {
                                y.icon(good_icon(*g), 14.0, good_color(*g));
                                y.bold(format!("{}", a.round() as i64), 13.0, INK);
                            });
                        }
                    })
                    .tip(Tip::name("Materials"));
                    card_row(u, "Push", num(r.pressure), INK, Tip::name("River push on the dam").line("Per day", num(r.pressure)));
                    let fill = if r.fill_days.is_finite() { days(r.fill_days.ceil()) } else { "-".into() };
                    card_row(u, "Fill", fill, INK, Tip::name("Days to fill").line("No inflow", if r.fill_days.is_finite() { "" } else { "-" }));
                    if r.flooded_buildings > 0 {
                        u.row(6.0, |x| {
                            x.icon(Icon::Urgent, 10.0, BAD);
                            x.icon(Icon::Building, 16.0, INK);
                            x.bold(format!("{}", r.flooded_buildings), 13.0, BAD);
                        })
                        .tip(Tip::name("Floods buildings"));
                    }
                }
                None => {
                    u.row(6.0, |r| {
                        r.icon(Icon::Cross, 13.0, BAD);
                        r.bold("No pond", 12.0, INK);
                    });
                }
            }
        } else {
            u.row(8.0, |x| {
                x.spacer();
                for (g, a) in build_logic::path_cost(sim::buildings::BuildingKind::Canal, n) {
                    x.row(2.0, |y| {
                        y.icon(good_icon(g), 14.0, good_color(g));
                        y.bold(format!("{}", a.round() as i64), 13.0, INK);
                    });
                }
            });
        }
        u.row(6.0, |r| {
            r.spacer();
            let ok = check.is_ok();
            r.icon_button(Icon::Clear, 13.0, HudAction::ClearLine, FAINT, false).tip(Tip::name("Clear"));
            r.icon_button(Icon::Check, 15.0, HudAction::ConfirmLine, if ok { ACCENT } else { FAINT }, false).tip(Tip::name(if is_dam { "Build" } else { "Dig" }).key("Enter"));
        });
    })
}

// ───────────────────────────── river profile (A6) ─────────────────────────────

pub fn profile(c: &Ctx) -> Vec<VNode> {
    if c.hud.tool != Tool::Dam {
        return Vec::new();
    }
    let s = c.s;
    let hud = c.hud;
    const H: f32 = 92.0;
    const UP: usize = 70;
    const DOWN: usize = 40;
    const COL_W: f32 = 5.0;
    let collapsed = hud.profile_collapsed;
    vec![panel_col(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..abs(Some(12.0), Some(12.0), None, Some(12.0)) }, 4.0, |u| {
        u.row(10.0, |r| {
            r.icon_button(if collapsed { Icon::Expand } else { Icon::Collapse }, 11.0, HudAction::ToggleProfile, FAINT, false).tip(Tip::name("Profile").key("P"));
            r.icon(Icon::Profile, 16.0, MUTED);
            r.icon(Icon::Upstream, 14.0, MUTED).tip(Tip::name("Upstream"));
            r.spacer();
            key_hints(r, Tool::Dam);
        });
        if collapsed {
            return;
        }
        let anchor = if hud.line.is_empty() {
            s.colony.beavers.first().map(|b| b.pos).unwrap_or((0.0, 0.0))
        } else {
            let n = hud.line.len() as f32;
            (hud.line.iter().map(|c| c.0 as f32).sum::<f32>() / n, hud.line.iter().map(|c| c.1 as f32).sum::<f32>() / n)
        };
        let Some((win, mid)) = sim::hud::river_profile_through(s, anchor, UP, DOWN) else {
            u.text("No river", 12.0, MUTED);
            return;
        };
        let near_line = !hud.line.is_empty();
        let preview = hud.dam_preview();
        let new_surface = preview.map(|pv| sim::hud::profile_with_preview(&win, pv));
        let lo = win.iter().map(|q| q.bed).fold(f32::MAX, f32::min) - 0.3;
        let hi = win.iter().map(|q| q.surface).chain(new_surface.iter().flatten().copied()).fold(f32::MIN, f32::max) + 0.3;
        let span = (hi - lo).max(0.5);
        let px = |v: f32| (v / span * H).max(0.0);
        let pond = Color::srgb(0.68, 0.84, 0.95);
        let wood = Color::srgb(0.54, 0.35, 0.2);
        let bed = Color::srgb(0.42, 0.3, 0.2);
        u.row(8.0, |r| {
            r.col(0.0, |axis| {
                axis.text(num(hi), 10.0, MUTED);
                axis.spacer();
                axis.text(num(lo), 10.0, MUTED);
            })
            .style(|n| {
                n.height = Val::Px(H);
                n.align_items = AlignItems::FlexEnd;
            });
            r.node(Node { height: Val::Px(H), flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, ..default() }, |cols| {
                for (i, q) in win.iter().enumerate() {
                    let ns = new_surface.as_ref().map(|v| v[i]).unwrap_or(q.surface);
                    let here = near_line && i == mid;
                    cols.col(0.0, |cc| {
                        if ns > q.surface + 0.005 {
                            cc.add(VNode::new(Node { height: Val::Px(px(ns - q.surface)), ..default() })).bg(pond);
                        }
                        cc.add(VNode::new(Node { height: Val::Px(px(q.surface - q.crest)), ..default() })).bg(WATER);
                        cc.add(VNode::new(Node { height: Val::Px(px(q.crest - q.bed)), ..default() })).bg(wood);
                        cc.add(VNode::new(Node { height: Val::Px(px(q.bed - lo)), ..default() })).bg(if here { SELECT } else { bed });
                    })
                    .style(|n| n.width = Val::Px(COL_W));
                }
            });
            r.col(5.0, |legend| {
                for (col, name) in [(WATER, "Water"), (pond, "New"), (wood, "Dams"), (SELECT, "Line")] {
                    legend.swatch(col, 12.0, 8.0).tip(Tip::name(name));
                }
            });
        })
        .style(|n| n.justify_content = JustifyContent::Center);
    })]
}

// ───────────────────────────── minimap (A10) ─────────────────────────────

pub fn minimap(c: &Ctx) -> Vec<VNode> {
    if c.placing() {
        return Vec::new();
    }
    let Some(img) = c.minimap else { return Vec::new() };
    const SIZE: f32 = 150.0;
    if c.hud.minimap_collapsed {
        return vec![panel_row(Node { padding: UiRect::all(Val::Px(4.0)), ..abs(Some(12.0), None, None, Some(12.0)) }, 4.0, |u| {
            u.icon_button(Icon::North, 16.0, HudAction::ToggleMinimap, FAINT, false).tip(Tip::name("Map"));
        })];
    }
    let n = c.s.world.size as f32;
    let (cx, cy) = c.view_cell;
    let mut markers: Vec<(f32, f32, Color)> = Vec::new();
    for p in c.problems {
        if let Some(f) = p.focus {
            markers.push((f.0 as f32, f.1 as f32, if p.severity == Severity::Urgent { BAD } else { WARN }));
        }
    }
    vec![panel_col(Node { padding: UiRect::all(Val::Px(5.0)), ..abs(Some(12.0), None, None, Some(12.0)) }, 0.0, |u| {
        u.image(img.clone(), Node { width: Val::Px(SIZE), height: Val::Px(SIZE), ..default() })
            .tag(Tag::Minimap)
            .solid()
            .children
            .extend({
                let mut m = Ui::default();
                m.add(VNode::new(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px((cx / n * SIZE - 7.0).clamp(0.0, SIZE - 14.0)),
                    top: Val::Px((cy / n * SIZE - 7.0).clamp(0.0, SIZE - 14.0)),
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                }))
                .border(BorderColor::all(SELECT));
                for (x, y, col) in &markers {
                    m.icon(Icon::Warn, 10.0, *col).style(|nn| {
                        nn.position_type = PositionType::Absolute;
                        nn.left = Val::Px((x / n * SIZE - 5.0).clamp(0.0, SIZE - 10.0));
                        nn.top = Val::Px((y / n * SIZE - 5.0).clamp(0.0, SIZE - 10.0));
                    });
                }
                m.icon(Icon::North, 14.0, INK).style(|nn| {
                    nn.position_type = PositionType::Absolute;
                    nn.right = Val::Px(3.0);
                    nn.top = Val::Px(3.0);
                });
                m.nodes
            });
        u.row(0.0, |r| {
            r.spacer();
            r.icon_button(Icon::Collapse, 10.0, HudAction::ToggleMinimap, Color::NONE, false).tip(Tip::name("Hide map"));
        })
        .style(|nn| {
            nn.position_type = PositionType::Absolute;
            nn.right = Val::Px(2.0);
            nn.bottom = Val::Px(2.0);
        });
    })]
}

// ───────────────────────────── message lane (A8) ─────────────────────────────

pub fn lane(c: &Ctx) -> Vec<VNode> {
    let Some((m, _)) = &c.hud.msg else { return Vec::new() };
    let row = panel_row(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() }, 6.0, |u| {
        if let Some((i, col)) = m.icon {
            u.icon(i, 16.0, col);
        }
        if let Some((i, col)) = m.icon2 {
            u.icon(i, 16.0, col);
        }
        if !m.text.is_empty() {
            u.bold(m.text.clone(), 12.5, INK);
        }
    });
    let mut v = VNode::new(Node { justify_content: JustifyContent::Center, ..abs(Some(0.0), Some(0.0), Some(58.0), None) });
    v.children.push(row);
    vec![v]
}

/// Status of a finished or unfinished building in at most two words, with its colour.
pub fn status_line(s: &sim::SimState, id: BuildingId) -> Option<(Option<Icon>, String, Vec<Good>, Color)> {
    let b = s.buildings.building(id)?;
    let place = s.work.place(id);
    let empty = place.map_or(0, |pl| pl.slots.iter().filter(|sl| sl.worker.is_none()).count());
    let slots = place.map_or(0, |pl| pl.slots.len());
    Some(match (b.status, place.and_then(|pl| pl.site.as_ref())) {
        (Status::Complete, _) if b.kind == BuildingKind::WaterWheel => {
            if b.output > 0.0 {
                (Some(Icon::Wheel), format!("{:.0}%", b.output * 100.0), vec![], GOOD)
            } else {
                (None, "No current".into(), vec![], WARN)
            }
        }
        (Status::Complete, _) if slots > 0 && empty == slots => (Some(Icon::Unstaffed), "No workers".into(), vec![], WARN),
        (Status::Complete, _) => (None, "Working".into(), vec![], if empty > 0 { WARN } else { GOOD }),
        (_, Some(site)) => {
            let waiting: Vec<Good> = site.needed.iter().filter(|a| site.missing(a.0) > 0.0).map(|a| a.0).collect();
            if slots > 0 && empty == slots {
                (Some(Icon::RoleBuilder), "No builders".into(), vec![], WARN)
            } else if !waiting.is_empty() {
                (None, "Needs".into(), waiting, WARN)
            } else {
                (Some(Icon::Hammer), "Building".into(), vec![], GOOD)
            }
        }
        _ => (None, "Planned".into(), vec![], MUTED),
    })
}
