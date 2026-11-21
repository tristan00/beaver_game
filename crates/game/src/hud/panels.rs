use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction, HudState, MapLens, Tool, CAT_KEY_LABELS};
use crate::build_logic::{self, Group};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use crate::sim_driver::{SimClock, Speed};
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Good, Status};
use sim::calendar::{self, Date, Season};
use sim::colony::Stage;
use sim::hud::{Breakdown, Fix, Problem, ProblemKind, Severity, Source, Subject};

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
    pub history: &'a [(f64, usize, f32)],
    pub cursor: Option<Vec2>,
}

impl Ctx<'_> {
    pub fn placing(&self) -> bool {
        self.hud.tool != Tool::None
    }
}

pub const STRIP_GOODS: [Good; 3] = Good::ALL;
pub const RIGHT_W: f32 = 320.0;
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

fn centred(bottom: f32, child: VNode) -> VNode {
    let mut v = VNode::new(Node { justify_content: JustifyContent::Center, ..abs(Some(0.0), Some(0.0), None, Some(bottom)) });
    v.children.push(child);
    v
}

pub fn food_days(s: &sim::SimState) -> Option<f32> {
    let need = sim::hud::food_need_per_day(s).max(s.ledger.out_per_day(Good::Food));
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
    if g == Good::Food { t.push(TipLine::new("Shoots and bark to eat", "")); }
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

/// Days of food in store at what the colony eats now.
pub fn food_left(s: &sim::SimState) -> f32 {
    food_days(s).unwrap_or_else(|| s.buildings.stores.amount(Good::Food) / sim::hud::food_need_per_day(s).max(0.01))
}

/// Top left: is there enough? Sticks and clay as counts, food as days, beavers.
pub fn strip(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let stores = &s.buildings.stores;
    let idle = sim::jobs::idle_reasons(s);
    let free: Vec<_> = idle.iter().filter(|(_, r)| *r != sim::jobs::IdleReason::Resting).collect();
    vec![panel_row(Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..abs(Some(12.0), None, Some(10.0), None) }, 16.0, |u| {
        for g in [Good::Sticks, Good::Clay] {
            let have = stores.amount(g);
            let short = have < 0.5 && s.ledger.days_left(g, have).is_some();
            u.row(6.0, |r| {
                r.icon(good_icon(g), 20.0, good_color(g));
                r.bold(format!("{}", have.round() as i64), 16.0, if short { BAD } else { INK });
            })
            .tip(good_tip(s, g));
        }
        u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(20.0), ..default() })).bg(GROOVE);
        let d = food_left(s);
        u.row(6.0, |r| {
            r.icon(good_icon(Good::Food), 20.0, good_color(Good::Food));
            r.bold(days(d.min(999.0)), 16.0, if d < 7.0 { BAD } else { INK });
        })
        .tip(good_tip(s, Good::Food));
        let mut tip = headcount_tip(s);
        if !free.is_empty() {
            tip.push(TipLine::new("Free now", format!("{}", free.len())).icon(Icon::Idle, ACCENT));
            for (id, why) in free.iter().take(6) {
                if let Some(b) = s.colony.beaver(*id) {
                    tip.push(TipLine::new(b.name.clone(), idle_reason_word(*why)).icon(stage_icon(b.stage), MUTED).sub());
                }
            }
        }
        u.row(6.0, |r| {
            r.icon(Icon::Beaver, 20.0, FUR);
            r.bold(format!("{}", s.colony.population()), 16.0, INK);
            if !free.is_empty() {
                r.add(VNode::new(Node { width: Val::Px(8.0), height: Val::Px(8.0), border_radius: BorderRadius::all(Val::Px(4.0)), margin: UiRect::bottom(Val::Px(12.0)), ..default() })).bg(ACCENT);
            }
        })
        .tip(tip)
        .action(if free.is_empty() { HudAction::Colony(Some(ColonyTab::Population)) } else { HudAction::CycleIdle });
    })]
}

fn season_color(se: Season) -> Color {
    match se {
        Season::Spring => Color::srgb(0.5, 0.74, 0.42),
        Season::Summer => Color::srgb(0.86, 0.7, 0.36),
        Season::Autumn => Color::srgb(0.72, 0.5, 0.33),
        Season::Winter => Color::srgb(0.8, 0.84, 0.9),
    }
}

/// How full the dammed ponds are, as a share of their full level.
fn pond_fill(s: &sim::SimState) -> Option<f32> {
    let fills: Vec<f32> = s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()).filter_map(|d| d.pond.as_ref()).map(|p| ((p.level - p.bottom) / (p.full_level - p.bottom).max(0.01)).clamp(0.0, 1.0)).collect();
    (!fills.is_empty()).then(|| fills.iter().sum::<f32>() / fills.len() as f32)
}

/// Top right: what will the water do next? The year, speed, days to the dry
/// spell, and a stake showing the ponds now and where summer will leave them.
pub fn calendar(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let t = s.time_days;
    let d = Date::at(t);
    let clock = c.clock;
    vec![panel_row(Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(9.0)), ..abs(None, Some(12.0), Some(10.0), None) }, 12.0, |u| {
        let phase = calendar::year_phase(t);
        u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |ring| {
            ring.bold(format!("{}", d.day), 15.0, INK);
            let ang = phase * std::f32::consts::TAU;
            let (x, y) = (26.0 + 22.0 * ang.sin() - 5.0, 26.0 - 22.0 * ang.cos() - 5.0);
            ring.add(VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(10.0), height: Val::Px(10.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() }))
                .bg(INK)
                .border(BorderColor::all(PANEL));
        })
        .skin(Skin::SeasonRing)
        .tip(Tip::titled(Some((season_icon(d.season), season_color(d.season))), d.season.name()).with_value(format!("Year {}", d.year)));
        u.col(8.0, |x| {
            x.row(5.0, |r| {
                r.pebble(Icon::Pause, 13.0, HudAction::Pause, if clock.paused { Peb::On } else { Peb::Idle }).tip(Tip::name("Pause").key("Space"));
                for (sp, icon, key) in [(Speed::X1, Icon::Play, "1"), (Speed::X2, Icon::Fast2, "2"), (Speed::X3, Icon::Fast3, "3")] {
                    let on = !clock.paused && clock.speed == sp;
                    r.pebble(icon, 13.0, HudAction::Speed(sp), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(format!("Speed {key}")).key(key));
                }
            });
            let (icon, col, n, name) = match d.season {
                Season::Summer => (Icon::Autumn, season_color(Season::Autumn), calendar::days_until(t, Season::Autumn), "Rain returns in"),
                Season::Winter => (Icon::Spring, season_color(Season::Spring), calendar::days_until(t, Season::Spring), "Spring in"),
                _ => (Icon::DrySummer, WARN, calendar::days_until(t, Season::Summer), "Dry summer in"),
            };
            x.row(5.0, |r| {
                r.icon(icon, 16.0, col);
                r.bold(days(n.ceil() as f32), 14.0, INK);
            })
            .tip(Tip::titled(Some((icon, col)), name).with_value(days(n.ceil() as f32)));
        });
        let ponds = sim::hud::ponds(s);
        let season = calendar::SEASON_DAYS as f32;
        let fill = pond_fill(s);
        let cover = ponds.summer_cover_days();
        let mark = match (fill, cover) {
            (Some(f), Some(cv)) => Some((f * (1.0 - season / cv.max(0.01))).max(0.0)),
            _ => None,
        };
        let mut tip = Tip::titled(Some((Icon::Gauge, WATER)), "Ponds");
        match (fill, cover) {
            (Some(f), Some(cv)) => {
                tip.push(TipLine::new("Full", format!("{:.0}%", f * 100.0)));
                tip.push(TipLine::new("Lasts a dry summer", format!("{} / {}", days(cv.min(999.0)), days(season))).color(if cv >= season { GOOD } else { BAD }));
                tip.push(TipLine::new("Red line", "where summer leaves it").sub());
            }
            (Some(f), None) => {
                tip.push(TipLine::new("Full", format!("{:.0}%", f * 100.0)));
            }
            _ => {
                tip.push(TipLine::new("No dammed pond yet", ""));
            }
        }
        u.stake(fill.unwrap_or(0.0), mark, 26.0, 52.0).tip(tip);
    })]
}

fn alert_face(s: &sim::SimState, p: &Problem) -> (Vec<(Icon, Color)>, String, &'static str) {
    let v = p.value;
    match p.kind {
        ProblemKind::Food => (vec![(Icon::Food, good_color(Good::Food))], if v < 0.5 { "0".into() } else { days(v) }, "Food runs out"),
        ProblemKind::WinterFood => (vec![(Icon::Winter,MUTED),(Icon::Food,good_color(Good::Food))],format!("{} / 90d",days(v)),"Prepare food for winter"),
        ProblemKind::Idle => (vec![(Icon::Idle, WARN)], format!("{}", v as i64), "Free beavers"),
        ProblemKind::NoBuilders => (vec![(Icon::Site, INK), (Icon::RoleBuilder, MUTED)], format!("{}", v as i64), "Sites without builders"),
        ProblemKind::StoreFull => {
            let g = match p.subject {
                Some(Subject::Good(g)) => g,
                _ => Good::Sticks,
            };
            (vec![(good_icon(g), good_color(g)), (Icon::Full, WARN)], format!("{:.0}%", v), "Store full")
        }
        ProblemKind::WeakDam => (vec![(Icon::Dam, INK)], format!("{:.0}%", v), "Dam weakening"),
        ProblemKind::Maintenance => (vec![(Icon::Hammer, WARN)], format!("{:.0}%",v), "Building needs repair"),
        ProblemKind::DamFailed => (vec![(Icon::Dam, INK), (Icon::Cross, BAD)], String::new(), "Dam failed"),
        ProblemKind::LoggingAreaRunningOut => (vec![(Icon::LoggingCamp, INK), (Icon::Tree, WARN)], format!("{}", v as i64), "Logging area running out"),
        ProblemKind::ForestFalling => (vec![(Icon::Tree, INK), (Icon::Down, WARN)], format!("-{:.0}%", v), "Forest falling"),
        ProblemKind::HighAndDry => {
            let k = match p.subject {
                Some(Subject::Building(k)) => k,
                _ => sim::buildings::BuildingKind::Shelter,
            };
            (vec![(super::words::building_icon(k), INK), (Icon::Down, WARN)], format!("{}", v as i64), "High and dry")
        }
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

fn fix_face(f: &Fix) -> Option<(Icon, String, String)> {
    match f {
        Fix::AddWorkers { n, .. } => Some((Icon::Plus, format!("{n}"), format!("Add {n} workers"))),
        Fix::Gather { kind: sim::resources::ResourceKind::Plant, n } => Some((Icon::Food, format!("+{n}"), format!("Send {n} workers to gather food"))),
        Fix::Gather { kind, n } => Some((resource_icon(*kind), format!("+{n}"), format!("{n} gather {}", resource_name(*kind).to_lowercase()))),
        Fix::Place(k) => Some((building_icon(*k), "+".into(), format!("Place {}", k.spec().name.to_lowercase()))),
        Fix::StaffSites => Some((Icon::RoleBuilder, "+".into(), "Staff the sites".into())),
        Fix::OpenDam { .. } => Some((Icon::Dam, String::new(), "Open the dam".into())),
        Fix::RepairBuilding { .. } => Some((Icon::Hammer, "Repair".into(), "Repair with materials and builders".into())),
        Fix::PutToWork { role, n } => Some((role_kind_icon(*role), format!("+{n}"), format!("{n} to {}", role_kind_name(*role).to_lowercase()))),
        Fix::OpenJobs => Some((Icon::Jobs, String::new(), "Open jobs".into())),
        Fix::Accept(_) => Some((Icon::Check, String::new(), "Welcome".into())),
        Fix::Decline(_) => Some((Icon::Cross, String::new(), "Turn away".into())),
        Fix::Focus => None,
    }
}

/// An alert's look: its icon, the one number it shows, and its name.
pub fn alert_token(s: &sim::SimState, p: &Problem) -> (Icon, Color, String, &'static str, Skin) {
    let (icons, value, name) = alert_face(s, p);
    let (icon, col) = icons.first().copied().unwrap_or((Icon::Warn, WARN));
    let skin = if p.severity == Severity::Urgent { Skin::TokenBad } else { Skin::TokenWarn };
    let value = value.split_whitespace().next().unwrap_or("").to_string();
    (icon, col, value, name, skin)
}

/// What an alert's tooltip says: why, and the fix as a line you can click.
pub fn alert_tip(c: &Ctx, p: &Problem) -> Tip {
    let (icons, value, name) = alert_face(c.s, p);
    let mut cause = Tip::titled(icons.first().copied(), name).with_value(value);
    for l in breakdown_lines(c.s, &p.cause, p.kind == ProblemKind::Food) {
        cause.push(l);
    }
    for f in std::iter::once(&p.fix).chain(p.alt_fix.iter()) {
        if let Some((icon, _, words)) = fix_face(f) {
            cause.push(TipLine::new(words, "").icon(icon, INK).act(HudAction::Fix(*f, p.focus)));
        }
    }
    cause
}

/// What needs the player: one token per problem, an icon and one number. A
/// click jumps there; the fix is in the tooltip.
pub fn alerts(c: &Ctx, u: &mut Ui) {
    if c.problems.is_empty() {
        return;
    }
    u.row(6.0, |r| {
        for p in c.problems {
            let (icon, col, value, _, skin) = alert_token(c.s, p);
            let t = r.node(Node { width: Val::Px(44.0), height: Val::Px(44.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: Val::Px(0.0), flex_shrink: 0.0, ..default() }, |k| {
                k.icon(icon, if value.is_empty() { 22.0 } else { 18.0 }, col);
                if !value.is_empty() {
                    k.bold(value.clone(), 11.0, INK);
                }
            });
            t.skin(skin);
            t.tip(alert_tip(c, p));
            t.action(match p.focus {
                Some(f) => HudAction::Fix(Fix::Focus, Some(f)),
                None => HudAction::Fix(p.fix, None),
            });
        }
    });
}

pub fn right(c: &Ctx) -> Vec<VNode> {
    let mut col = VNode::new(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: Val::Px(8.0), overflow: Overflow::clip(), ..abs(None, Some(12.0), Some(84.0), Some(90.0)) });
    let mut ui = Ui::default();
    alerts(c, &mut ui);
    if c.hud.tool == Tool::None && c.hud.colony.is_none() {
        for p in c.hud.pinned.iter().filter(|p| **p != c.sel) {
            if let Some(v) = super::inspect::inspector(c, *p) {
                ui.add(v);
            }
        }
        if let Some(v) = super::inspect::inspector(c, c.sel) {
            ui.add(v);
        }
    }
    if ui.nodes.is_empty() {
        return Vec::new();
    }
    col.children = ui.nodes;
    vec![col]
}

/// Left: how to read the map. Three lenses; clicking the one that is on returns to the plain view.
pub fn rail(c: &Ctx) -> Vec<VNode> {
    vec![panel_col(Node { padding: UiRect::all(Val::Px(8.0)), align_items: AlignItems::Center, ..abs(Some(12.0), None, Some(84.0), None) }, 6.0, |u| {
        for (l, icon, name, key) in [(MapLens::Water, Icon::ViewWater, "Water", "F1"), (MapLens::Forest, Icon::ViewForest, "Forest", "F2"), (MapLens::Elevation, Icon::ViewElevation, "Height", "F3")] {
            let on = c.lens == l;
            u.pebble(icon, 17.0, HudAction::SetLens(if on { MapLens::World } else { l }), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name).key(key));
        }
        u.rule().style(|n| n.width = Val::Px(28.0));
        let open = c.hud.colony.is_some();
        u.pebble(Icon::Colony, 17.0, HudAction::Colony(if open { None } else { Some(ColonyTab::Population) }), if open { Peb::On } else { Peb::Idle }).tip(Tip::name("Colony book").key("Tab"));
    })]
}

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
        BuildingKind::Dam => build_logic::path_cost(kind, 1),
        k => k.spec().cost.to_vec(),
    }
}

fn item_tip(s: &sim::SimState, e: &build_logic::MenuEntry) -> Tip {
    let kind = e.kind;
    let mut t = Tip::titled(Some((building_icon(kind), INK)), kind.spec().name);
    if e.unlocked {
        let cost = item_cost(kind);
        if kind == BuildingKind::Dam {
            t.push(TipLine::new("Per load", "raises one cell"));
        }
        for (g, a) in cost {
            t.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)).sub());
        }
    } else {
        let gaps = s.missing_prerequisites(kind);
        let tech = e.need.unwrap_or("");
        t.push(TipLine::new(tech, "").icon(Icon::Padlock, MUTED));
        for g in gaps {
            t.push(gap_line(&g).sub());
        }
    }
    t
}

pub fn gap_line(k: &BuildingKind) -> TipLine {
    TipLine::new(k.spec().name, "Finish first").icon(building_icon(*k), INK)
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
        if hud.tool == Tool::Dam {
            return out;
        }
        let (icon, name) = match hud.tool {
            Tool::Building(k) => (building_icon(k), k.spec().name),
            _ => (Icon::Dam, "Dam"),
        };
        out.push(centred(12.0, panel_row(Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)), ..default() }, 10.0, |u| {
            u.icon(icon, 24.0, INK).tip(Tip::name(name));
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
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(26.0), margin: UiRect::horizontal(Val::Px(2.0)), ..default() })).bg(TIMBER.with_alpha(0.35));
        let can_undo = !hud.undo_is_empty();
        u.icon_button(Icon::Undo, 18.0, HudAction::Undo, FAINT, false).tip(Tip::name("Undo").key("Ctrl+Z")).style(|_| {}).bg = if can_undo { FAINT } else { Color::NONE };
    })));
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
                    Tool::None => false,
                };
                item_tile(u, s, e, selected);
            }
        })));
    }
    out
}

pub fn settle_chip(_c: &Ctx) -> Vec<VNode> {
    let larder = (2.0 + 2.0 * sim::tuning::colony::KIT_FOOD_WEIGHT) * sim::tuning::colony::HUNGER_PER_DAY * sim::starts::START_LARDER_DAYS;
    let mut bundle: Vec<(Good, String)> = vec![(Good::Food, num(larder))];
    for (g, a) in sim::starts::START_BUNDLE {
        bundle.push((g, num(a)));
    }
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
            t.push(TipLine::new("Shelter", "").icon(Icon::Lodge, INK));
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
            for (i, x) in [(Icon::Drop, h.water), (Icon::Tree, h.trees), (Icon::Food, h.food), (Icon::Health, h.safety), (Icon::Building, h.room)] {
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

pub fn cursor_tag(c: &Ctx) -> Vec<VNode> {
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

/// The dam's cost and confirm, on the map: it follows the cursor while a run is
/// being dragged, then hangs from the line's last corner with a check and a cross.
pub fn dam_tag(c: &Ctx, at: Vec2, pinned: bool, win: Vec2) -> Vec<VNode> {
    let Some(plan) = c.hud.plan.as_ref().filter(|p| p.line == c.hud.line) else { return Vec::new() };
    let bad = plan.refusal.is_some();
    let mut ui = Ui::default();
    let row = ui.row(9.0, |r| {
        match &plan.refusal {
            Some((icon, why)) => {
                r.node(Node { width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, |x| {
                    x.icon(*icon, 20.0, if *icon == Icon::Drop { WATER } else { BAD });
                    x.icon(Icon::Cross, 12.0, BAD).style(|n| {
                        n.position_type = PositionType::Absolute;
                        n.right = Val::Px(-3.0);
                        n.bottom = Val::Px(-3.0);
                    });
                })
                .tip(Tip::titled(Some((Icon::Dam, INK)), why.clone()));
            }
            None => {
                r.row(4.0, |x| {
                    x.icon(good_icon(Good::Sticks), 18.0, good_color(Good::Sticks));
                    x.bold(format!("{}", plan.sticks.round() as i64), 15.0, INK);
                })
                .tip(Tip::titled(Some((good_icon(Good::Sticks), good_color(Good::Sticks))), "Sticks for the first stage").line("Carried load by load", ""));
                r.row(4.0, |x| {
                    x.icon(Icon::Drop, 17.0, WATER);
                    x.bold(format!("+{}", plan.new_wet), 15.0, INK);
                })
                .tip(Tip::titled(Some((Icon::Drop, WATER)), "New pond cells"));
                let drowned = plan.preview.as_ref().map_or(0, |p| p.flooded_trees);
                if drowned > 0 {
                    r.row(4.0, |x| {
                        x.icon(Icon::Tree, 16.0, WARN);
                        x.bold(format!("{drowned}"), 15.0, WARN);
                    })
                    .tip(Tip::titled(Some((Icon::Tree, WARN)), "Trees drowned"));
                }
            }
        }
        if pinned {
            r.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(24.0), ..default() })).bg(GROOVE);
            if bad {
                let why = plan.refusal.as_ref().map(|r| r.1.clone()).unwrap_or_default();
                r.pebble(Icon::Check, 15.0, HudAction::ConfirmLine, Peb::Faded).tip(Tip::name("Build").line(why, ""));
                if let Some(v) = r.nodes.last_mut() {
                    v.action = None;
                }
            } else {
                r.pebble(Icon::Check, 16.0, HudAction::ConfirmLine, Peb::Primary).tip(Tip::name("Build").key("Enter"));
            }
            r.pebble(Icon::Cross, 12.0, HudAction::CancelTool, Peb::Idle).tip(Tip::name("Drop the line").key("Esc"));
        }
    });
    panel(row);
    row.node.padding = UiRect::axes(Val::Px(12.0), Val::Px(8.0));
    if bad {
        row.skin(Skin::PlateBad);
    }
    let w = if pinned { 230.0 } else { 150.0 };
    let (left, top) = if pinned {
        let below = at.y + 86.0 < win.y;
        ((at.x - 26.0).clamp(8.0, (win.x - w - 8.0).max(8.0)), if below { at.y + 26.0 } else { at.y - 70.0 })
    } else {
        ((at.x + 20.0).min((win.x - w - 8.0).max(8.0)), at.y + 18.0)
    };
    let mut v = ui.nodes.pop().unwrap();
    v.node.position_type = PositionType::Absolute;
    v.node.left = Val::Px(left);
    v.node.top = Val::Px(top);
    if !pinned {
        v.solid = false;
    }
    vec![v]
}

pub fn minimap(c: &Ctx) -> Vec<VNode> {
    if c.placing() || c.hud.colony.is_some() {
        return Vec::new();
    }
    let Some(img) = c.minimap else { return Vec::new() };
    const SIZE: f32 = 150.0;
    let n = c.s.world.size as f32;
    let (cx, cy) = c.view_cell;
    let mut markers: Vec<(f32, f32, Color)> = Vec::new();
    for p in c.problems {
        if let Some(f) = p.focus {
            markers.push((f.0 as f32, f.1 as f32, if p.severity == Severity::Urgent { BAD } else { WARN }));
        }
    }
    vec![panel_col(Node { padding: UiRect::all(Val::Px(8.0)), ..abs(Some(12.0), None, None, Some(12.0)) }, 0.0, |u| {
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
                    m.add(VNode::new(Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px((x / n * SIZE - 4.0).clamp(0.0, SIZE - 8.0)),
                        top: Val::Px((y / n * SIZE - 4.0).clamp(0.0, SIZE - 8.0)),
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    }))
                    .bg(*col);
                }
                m.nodes
            });
    })]
}

pub fn status_line(s: &sim::SimState, id: BuildingId) -> Option<(Option<Icon>, String, Vec<Good>, Color)> {
    let b = s.buildings.building(id)?;
    let place = s.work.place(id);
    let empty = place.map_or(0, |pl| pl.slots.iter().filter(|sl| sl.worker.is_none()).count());
    let slots = place.map_or(0, |pl| pl.slots.len());
    if place.is_some_and(|p|p.paused) {return Some((Some(Icon::Pause),"Paused".into(),vec![],MUTED));}
    if let Some(site)=place.and_then(|p|p.site.as_ref()) {
        let waiting:Vec<Good>=site.needed.iter().filter(|a|site.missing(a.0)>0.0).map(|a|a.0).collect();
        let stage=match s.maintenance.jobs.get(&id).map(|j|j.kind) {
            Some(sim::maintenance::Refit::Repair)=>"Repairing",
            Some(sim::maintenance::Refit::Reinforce)=>"Reinforcing",
            None=>"Building",
        };
        return Some(if !waiting.is_empty() && !site.labour_open() {(None,"Waiting".into(),waiting,WARN)}
            else if slots>0 && empty==slots {(Some(Icon::RoleBuilder),if s.work.auto_organise{"Queued"}else{"No builders"}.into(),vec![],MUTED)}
            else {(Some(Icon::Hammer),stage.into(),vec![],GOOD)});
    }
    if b.is_complete() && !b.operational() {return Some((Some(Icon::Hammer),"Ruined".into(),vec![],BAD));}
    Some(match (b.status, place.and_then(|pl| pl.site.as_ref())) {
        (Status::Complete, _) if s.buildings.stranded(b, &s.water.depth) => (Some(Icon::Flood), "Dry".into(), vec![], WARN),
        (Status::Complete, _) if slots > 0 && empty == slots => (Some(Icon::Unstaffed), "Idle".into(), vec![], if s.work.auto_organise{MUTED}else{WARN}),
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
