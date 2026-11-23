use super::ui::*;
use super::words::*;
use super::{ColonyTab, Harvest, HudAction, HudState, LedgerTab, MapLens, Tool};
use crate::build_logic;
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
    pub minimap: Option<&'a super::minimap::MinimapImage>,
    pub view_cell: (f32, f32),
    pub cursor: Option<Vec2>,
    /// The journal's lines, oldest first, and the real time now.
    pub journal: &'a [super::Notice],
    pub now: f32,
}

impl Ctx<'_> {
    pub fn placing(&self) -> bool {
        self.hud.tool != Tool::None
    }
}

pub const RIGHT_W: f32 = 320.0;

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
        Free => "Free",
    }
}

/// Top left: is there enough? Sticks, clay and food as counts, then beavers.
pub fn strip(c: &Ctx) -> Vec<VNode> {
    let s = c.s;
    let stores = &s.buildings.stores;
    let idle = sim::jobs::idle_reasons(s);
    let free: Vec<_> = idle.iter().filter(|(_, r)| *r != sim::jobs::IdleReason::Resting).collect();
    vec![panel_row(Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..abs(Some(12.0), None, Some(10.0), None) }, 16.0, |u| {
        // Pebbles and saplings only once the colony has some.
        for g in [Good::Sticks, Good::Clay, Good::Food, Good::Pebbles, Good::Saplings] {
            let have = stores.amount(g);
            if matches!(g, Good::Pebbles | Good::Saplings) && have < 0.5 {
                continue;
            }
            let short = have < 0.5 && s.ledger.days_left(g, have).is_some();
            u.row(6.0, |r| {
                r.icon(good_icon(g), 20.0, good_color(g));
                r.bold(format!("{}", have.round() as i64), 16.0, if short { BAD } else { INK });
            })
            .tip(good_tip(s, g));
        }
        u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(20.0), ..default() })).bg(GROOVE);
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

/// A water rise for the dam badge: tenths of a metre, "+0" for none.
fn rise_text(m: f32) -> String {
    if m < 0.05 { "+0".into() } else { format!("+{:.1} m", m) }
}

fn season_color(se: Season) -> Color {
    match se {
        Season::Spring => Color::srgb(0.5, 0.74, 0.42),
        Season::Summer => Color::srgb(0.86, 0.7, 0.36),
        Season::Autumn => Color::srgb(0.72, 0.5, 0.33),
        Season::Winter => Color::srgb(0.8, 0.84, 0.9),
    }
}

/// Top right: the year and the speed.
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
        .tip(Tip::titled(Some((season_icon(d.season), season_color(d.season))), d.season.name()));
        u.col(8.0, |x| {
            x.bold(format!("Year {}", d.year), 13.0, INK);
            x.row(5.0, |r| {
                r.pebble(Icon::Pause, 13.0, HudAction::Pause, if clock.paused { Peb::On } else { Peb::Idle }).tip(Tip::name("Pause").key("Space"));
                for (sp, icon, key) in [(Speed::X1, Icon::Play, "1"), (Speed::X2, Icon::Fast2, "2"), (Speed::X3, Icon::Fast3, "3")] {
                    let on = !clock.paused && clock.speed == sp;
                    r.pebble(icon, 13.0, HudAction::Speed(sp), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(format!("Speed {key}")).key(key));
                }
            });
        });
    })]
}

fn alert_face(s: &sim::SimState, p: &Problem) -> (Vec<(Icon, Color)>, String, &'static str) {
    let v = p.value;
    match p.kind {
        ProblemKind::Food => (vec![(Icon::Food, good_color(Good::Food))], if v < 0.5 { "0".into() } else { days(v) }, "Food runs out"),
        ProblemKind::WinterFood => (vec![(Icon::Winter,MUTED),(Icon::Food,good_color(Good::Food))],format!("{} / {}d",days(v),sim::calendar::SEASON_DAYS as i64),"Prepare food for winter"),
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
        ProblemKind::CanalDry => (vec![(Icon::Canal, INK), (Icon::Drop, WARN)], String::new(), "Canal dry"),
        ProblemKind::CanalSilting => (vec![(Icon::Canal, INK), (Icon::Down, WARN)], format!("{:.0}%", v), "Canal silting"),
        ProblemKind::PondDraining => (vec![(Icon::Dam, INK), (Icon::Flood, WARN)], format!("-{:.0}cm", v), "Pond draining"),
        ProblemKind::Wolf => (vec![(Icon::Wolf, BAD)], String::new(), "Wolf"),
        ProblemKind::Flooded => {
            let k = match p.subject {
                Some(Subject::Building(k)) => k,
                _ => sim::buildings::BuildingKind::Shelter,
            };
            (vec![(super::words::building_icon(k), INK), (Icon::Flood, BAD)], format!("{}", v as i64), "Flooded")
        }
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
        Fix::RepairBuilding { .. } => Some((Icon::Hammer, "Repair".into(), "Repair with materials and maintainers".into())),
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
    alert_tip_in(c.s, p)
}

/// `alert_tip`, from the world alone (for the pins over the map).
pub fn alert_tip_in(s: &sim::SimState, p: &Problem) -> Tip {
    let (icons, value, name) = alert_face(s, p);
    let mut cause = Tip::titled(icons.first().copied(), name).with_value(value);
    for l in breakdown_lines(s, &p.cause, p.kind == ProblemKind::Food) {
        cause.push(l);
    }
    for f in std::iter::once(&p.fix).chain(p.alt_fix.iter()) {
        if let Some((icon, _, words)) = fix_face(f) {
            cause.push(TipLine::new(words, "").icon(icon, INK).act(HudAction::Fix(*f, p.focus)));
        }
    }
    cause
}

const BADGE: f32 = 44.0;

fn badge_node() -> Node {
    Node { width: Val::Px(BADGE), height: Val::Px(BADGE), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: Val::Px(0.0), flex_shrink: 0.0, ..default() }
}

/// What wants the player, as badges side by side at the top centre: standing problems, each
/// an icon and one number. No words until a badge is clicked; then its card opens beneath the
/// row, with the fix. What merely happened goes to the journal instead.
pub fn badges(c: &Ctx) -> Vec<VNode> {
    if c.problems.is_empty() {
        return Vec::new();
    }
    let open = c.hud.open_badge.as_deref();
    let mut col = VNode::new(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(8.0), ..abs(Some(0.0), Some(0.0), Some(12.0), None) });
    let mut ui = Ui::default();
    let mut card: Option<VNode> = None;
    ui.row(6.0, |r| {
        for p in c.problems {
            let key = format!("alert:{:?}", p.kind);
            let (icon, col, value, name, skin) = alert_token(c.s, p);
            let t = r.node(badge_node(), |k| {
                k.icon(icon, if value.is_empty() { 22.0 } else { 18.0 }, col);
                if !value.is_empty() {
                    k.bold(value.clone(), 11.0, INK);
                }
            });
            t.skin(skin);
            t.action(HudAction::Badge(key.clone()));
            if open == Some(key.as_str()) {
                let mut k = Ui::default();
                k.row(8.0, |row| {
                    row.icon(icon, 20.0, col);
                    row.bold(name.to_string(), 16.0, INK);
                    if !value.is_empty() {
                        row.bold(value.clone(), 16.0, MUTED);
                    }
                });
                k.row(6.0, |row| {
                    if let Some(f) = p.focus {
                        row.pebble(Icon::Focus, 12.0, HudAction::Fix(Fix::Focus, Some(f)), Peb::Idle).tip(Tip::name("Go there"));
                    }
                    for f in std::iter::once(&p.fix).chain(p.alt_fix.iter()) {
                        if let Some((icon, _, words)) = fix_face(f) {
                            row.pebble(icon, 12.0, HudAction::Fix(*f, p.focus), Peb::Good).tip(Tip::name(words));
                        }
                    }
                });
                card = Some(card_node(k, Some(alert_tip(c, p))));
            }
        }
    });
    col.children = ui.nodes;
    if let Some(card) = card {
        col.children.push(card);
    }
    vec![col]
}

/// The words of an opened badge, on a plate under the row.
fn card_node(k: Ui, tip: Option<Tip>) -> VNode {
    let mut v = VNode::new(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)), ..default() });
    v.children = k.nodes;
    v.skin(Skin::Plate);
    if let Some(tip) = tip {
        v.tip(tip);
    }
    v
}

pub fn right(c: &Ctx) -> Vec<VNode> {
    let bottom = super::journal::height(c) + 20.0;
    let mut col = VNode::new(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: Val::Px(8.0), overflow: Overflow::clip(), ..abs(None, Some(12.0), Some(84.0), Some(bottom)) });
    let mut ui = Ui::default();
    // A beaver pointed at in the colony book or ledger shows on its own.
    if let Some(id) = c.hud.peek.filter(|id| c.s.colony.beaver(*id).is_some()) {
        if let Some(v) = super::inspect::inspector(c, Selection::Beaver(id)) {
            ui.add(v);
        }
    } else if c.hud.tool == Tool::None {
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

/// Left: how to read the map. Five lenses; clicking the one that is on returns to the plain view.
pub fn rail(c: &Ctx) -> Vec<VNode> {
    vec![panel_col(Node { padding: UiRect::all(Val::Px(8.0)), align_items: AlignItems::Center, ..abs(Some(12.0), None, Some(84.0), None) }, 6.0, |u| {
        for (l, icon, name, key) in [(MapLens::Water, Icon::ViewWater, "Water", "F1"), (MapLens::Forest, Icon::ViewForest, "Forest", "F2"), (MapLens::Elevation, Icon::ViewElevation, "Height", "F3"), (MapLens::Deposits, Icon::ViewDeposits, "Deposits", "F4"), (MapLens::Underwater, Icon::ViewUnderwater, "Under water", "F5")] {
            let on = c.lens == l;
            u.pebble(icon, 17.0, HudAction::SetLens(if on { MapLens::World } else { l }), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name).key(key));
        }
        u.rule().style(|n| n.width = Val::Px(28.0));
        let open = c.hud.colony.is_some();
        u.pebble(Icon::Colony, 17.0, HudAction::Colony(if open { None } else { Some(ColonyTab::Population) }), if open { Peb::On } else { Peb::Idle }).tip(Tip::name("Colony book").key("Tab"));
        let open = c.hud.ledger.is_some();
        u.pebble(Icon::Ledger, 17.0, HudAction::Ledger(if open { None } else { Some(LedgerTab::Charts) }), if open { Peb::On } else { Peb::Idle }).tip(Tip::name("Colony ledger").key("L"));
    })]
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

fn item_tip(kind: BuildingKind) -> Tip {
    let mut t = Tip::titled(Some((building_icon(kind), INK)), kind.spec().name);
    if kind == BuildingKind::Dam {
        t.push(TipLine::new("Per load", "raises one cell"));
    }
    for (g, a) in item_cost(kind) {
        t.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)).sub());
    }
    t
}

/// Bottom: what can I make or order? Three buildings, two orders and undo, in one row.
pub fn bottom(c: &Ctx) -> Vec<VNode> {
    let hud = c.hud;
    vec![centred(12.0, panel_row(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)), ..default() }, 7.0, |u| {
        for (i, (kind, key)) in [(BuildingKind::Dam, "Z"), (BuildingKind::Canal, "V"), (BuildingKind::Shelter, "X"), (BuildingKind::Warehouse, "C"), (BuildingKind::Larder, "B"), (BuildingKind::Toolshed, "N"), (BuildingKind::Forester, "F"), (BuildingKind::Logistics, "H"), (BuildingKind::Lookout, "O"), (BuildingKind::Harbor, "P")].into_iter().enumerate() {
            if i == 2 {
                // Waterworks first, as a pair; the drop-down buildings after a groove.
                u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(30.0), margin: UiRect::horizontal(Val::Px(3.0)), ..default() })).bg(GROOVE);
            }
            let tool = match kind { BuildingKind::Dam => Tool::Dam, BuildingKind::Canal => Tool::Canal, k => Tool::Building(k) };
            let on = hud.tool == tool;
            let locked = c.s.unlocked(kind).err();
            let look = if on { Peb::On } else if locked.is_some() { Peb::Faded } else { Peb::Idle };
            let mut tip = item_tip(kind).key(key);
            if let Some(e) = locked {
                tip = tip.line(e.to_string(), "");
            }
            u.pebble(building_icon(kind), 21.0, HudAction::Pick(kind), look).tip(tip);
        }
        u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(30.0), margin: UiRect::horizontal(Val::Px(3.0)), ..default() })).bg(GROOVE);
        for (h, icon, name, line, key) in [
            (Harvest::Gather, Icon::Plant, "Gather", "Click food or clay, or drag an area", "G"),
            (Harvest::Cut, Icon::Tree, "Cut trees", "Click a tree, or drag an area", "T"),
        ] {
            let on = hud.tool == Tool::Harvest(h);
            u.pebble(icon, 20.0, HudAction::Harvest(h), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name).line(line, "").line("Selected beavers, or free ones", "").key(key));
        }
        u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(30.0), margin: UiRect::horizontal(Val::Px(3.0)), ..default() })).bg(GROOVE);
        let can_undo = !hud.undo_is_empty();
        u.pebble(Icon::Undo, 17.0, HudAction::Undo, if can_undo { Peb::Idle } else { Peb::Faded }).tip(Tip::name("Undo").key("Ctrl+Z"));
    }))]
}

pub fn settle_chip(_c: &Ctx) -> Vec<VNode> {
    vec![centred(12.0, panel_row(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..default() }, 12.0, |u| {
        u.row(4.0, |r| {
            r.icon(Icon::Beaver, 22.0, INK);
            r.bold("4", 14.0, INK);
        })
        .tip(Tip::titled(Some((Icon::Beaver, INK)), "The family").line("Adults", "2").line("Young", "2"));
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

/// The beavers an order would send: the selection, or every free one.
pub fn order_crew(s: &sim::SimState, hud: &HudState) -> Vec<sim::colony::BeaverId> {
    if !hud.selected.is_empty() {
        return hud.selected.clone();
    }
    sim::hud::idle_beavers(s)
}

pub fn cursor_tag(c: &Ctx) -> Vec<VNode> {
    if let (Tool::Harvest(h), Some(cur)) = (c.hud.tool, c.cursor) {
        let n = order_crew(c.s, c.hud).len();
        let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..abs(Some(cur.x + 20.0), None, Some(cur.y + 16.0), None) });
        let mut ui = Ui::default();
        ui.icon(if h == Harvest::Cut { Icon::Tree } else { Icon::Plant }, 16.0, if h == Harvest::Cut { GOOD } else { GOOD });
        ui.icon(Icon::Beaver, 16.0, if n == 0 { BAD } else { FUR });
        ui.bold(format!("{n}"), 14.0, if n == 0 { BAD } else { INK });
        v.children = ui.nodes;
        v.skin(if n == 0 { Skin::PlateBad } else { Skin::Plate });
        return vec![v];
    }
    let (Tool::Building(kind), Some(cell), Some(cur)) = (c.hud.tool, c.hover, c.cursor) else { return Vec::new() };
    let pv = build_logic::site_preview(c.s, kind, cell);
    let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..abs(Some(cur.x + 20.0), None, Some(cur.y + 16.0), None) });
    v.skin(if pv.verdict.is_ok() { Skin::Plate } else { Skin::PlateBad });
    let mut ui = Ui::default();
    match &pv.verdict {
        Ok(()) => {
            cost_row(&mut ui, kind.spec().cost, 13.0, INK);
            ui.icon(Icon::Check, 14.0, GOOD);
        }
        Err(e) => {
            ui.icon(Icon::Cross, 14.0, BAD);
            let w = place_error(e);
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
/// While a canal is drawn: labour, clay from the spoil, the share that floats a load, and
/// (once the line is set) build and drop.
pub fn canal_tag(c: &Ctx, at: Vec2, pinned: bool, win: Vec2) -> Vec<VNode> {
    let extending = c.hud.extending.is_some();
    let plan = c.hud.canal_plan.as_ref().filter(|p| p.0 == c.hud.line).map(|p| &p.1);
    if plan.is_none() && !extending {
        return Vec::new();
    }
    let refusal: Option<String> = match plan {
        Some(Err(e)) => Some(e.clone()),
        Some(Ok(p)) => sim::canals::check(p).err(),
        None => None,
    };
    let bad = refusal.is_some();
    let mut ui = Ui::default();
    let row = ui.row(9.0, |r| {
        match (&refusal, plan) {
            (Some(why), _) => {
                r.node(Node { width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, |x| {
                    x.icon(Icon::Canal, 20.0, BAD);
                    x.icon(Icon::Cross, 12.0, BAD).style(|n| {
                        n.position_type = PositionType::Absolute;
                        n.right = Val::Px(-3.0);
                        n.bottom = Val::Px(-3.0);
                    });
                })
                .tip(Tip::titled(Some((Icon::Canal, INK)), why.clone()));
            }
            (None, Some(Ok(p))) => {
                r.row(4.0, |x| {
                    x.icon(Icon::Beaver, 17.0, Color::srgb(0.73, 0.47, 0.25));
                    x.bold(format!("{}d", p.labour_days().ceil().max(1.0) as i64), 15.0, INK);
                })
                .tip(Tip::titled(Some((Icon::Beaver, INK)), "Digging").line("Beaver-days", num(p.labour_days())));
                r.row(4.0, |x| {
                    x.icon(good_icon(Good::Clay), 17.0, good_color(Good::Clay));
                    x.bold(format!("+{}", p.clay().round() as i64), 15.0, INK);
                })
                .tip(Tip::titled(Some((good_icon(Good::Clay), good_color(Good::Clay))), "Clay from the spoil"));
                r.row(4.0, |x| {
                    x.icon(Icon::Logs, 17.0, WATER);
                    x.bold(format!("{:.0}%", p.floatable_share() * 100.0), 15.0, INK);
                })
                .tip(Tip::titled(Some((Icon::Logs, WATER)), "Floats a load").line(if p.through { "Runs into other water" } else { "Ends on land" }, ""));
            }
            _ => {
                r.icon(Icon::Extend, 18.0, INK).tip(Tip::name("Extend"));
            }
        }
        if pinned {
            r.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(24.0), ..default() })).bg(GROOVE);
            if bad {
                r.pebble(Icon::Check, 15.0, HudAction::ConfirmLine, Peb::Faded).tip(Tip::name("Dig").line(refusal.clone().unwrap_or_default(), ""));
                if let Some(v) = r.nodes.last_mut() {
                    v.action = None;
                }
            } else {
                r.pebble(Icon::Check, 16.0, HudAction::ConfirmLine, Peb::Primary).tip(Tip::name("Dig").key("Enter"));
            }
            r.pebble(Icon::Cross, 12.0, HudAction::CancelTool, Peb::Idle).tip(Tip::name("Drop the line").key("Esc"));
        }
    });
    panel(row);
    row.node.padding = UiRect::axes(Val::Px(12.0), Val::Px(8.0));
    if bad {
        row.skin(Skin::PlateBad);
    }
    let w = if pinned { 250.0 } else { 170.0 };
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
                // The engine's own answer once its background run is in; the quick estimate till then.
                let rise = c.hud.dam_settled().map(|r| r.rise).or(plan.preview.as_ref().map(|p| p.flow_rise)).unwrap_or(0.0);
                let mut tip = Tip::titled(Some((Icon::Drop, WATER)), "Water rises").with_value(rise_text(rise)).line("New pond cells", format!("{}", plan.new_wet));
                if plan.preview.as_ref().is_some_and(|p| p.spill_at.is_some()) {
                    tip = tip.line("Runs around the dam", "");
                }
                r.row(4.0, |x| {
                    x.icon(Icon::Drop, 17.0, WATER);
                    x.bold(rise_text(rise), 15.0, INK);
                })
                .tip(tip);
                let drowned = c.hud.drowned.as_ref().filter(|d| d.0.0 == plan.line).map_or(&[][..], |d| &d.1[..]);
                if !drowned.is_empty() {
                    let mut tip = Tip::titled(Some((Icon::Flood, BAD)), "Buildings flooded").with_value(format!("{}", drowned.len()));
                    for b in drowned.iter().filter_map(|id| c.s.buildings.building(*id)) {
                        tip.push(TipLine::new(b.kind.spec().name, "").icon(building_icon(b.kind), INK).sub());
                    }
                    r.row(3.0, |x| {
                        x.icon(Icon::Warn, 15.0, BAD);
                        x.icon(Icon::Building, 15.0, BAD);
                        x.bold(format!("{}", drowned.len()), 15.0, BAD);
                    })
                    .tip(tip);
                }
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
    let Some(map) = c.minimap else { return Vec::new() };
    const SIZE: f32 = 150.0;
    let at = |p: (f32, f32)| {
        let (u, v) = map.to_map(p);
        (u * SIZE, v * SIZE)
    };
    let inside = |(x, y): (f32, f32)| (0.0..=SIZE).contains(&x) && (0.0..=SIZE).contains(&y);
    let (cx, cy) = at(c.view_cell);
    // Each alert shows where it is as its own icon, ringed red if urgent.
    let mut markers: Vec<((f32, f32), Icon, Color, Color)> = Vec::new();
    for p in c.problems {
        if let Some(f) = p.focus {
            let m = at((f.0 as f32 + 0.5, f.1 as f32 + 0.5));
            let (icons, _, _) = alert_face(c.s, p);
            if let (true, Some(&(icon, col))) = (inside(m), icons.first()) {
                markers.push((m, icon, col, if p.severity == Severity::Urgent { BAD } else { WARN }));
            }
        }
    }
    vec![panel_col(Node { padding: UiRect::all(Val::Px(8.0)), ..abs(Some(12.0), None, None, Some(12.0)) }, 0.0, |u| {
        u.image(map.image.clone(), Node { width: Val::Px(SIZE), height: Val::Px(SIZE), ..default() })
            .tag(Tag::Minimap)
            .solid()
            .children
            .extend({
                let mut m = Ui::default();
                m.add(VNode::new(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px((cx - 7.0).clamp(0.0, SIZE - 14.0)),
                    top: Val::Px((cy - 7.0).clamp(0.0, SIZE - 14.0)),
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                }))
                .border(BorderColor::all(SELECT));
                for &((x, y), icon, col, ring) in &markers {
                    let v = m.node(
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px((x - 9.0).clamp(0.0, SIZE - 18.0)),
                            top: Val::Px((y - 9.0).clamp(0.0, SIZE - 18.0)),
                            width: Val::Px(18.0),
                            height: Val::Px(18.0),
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(9.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        |d| {
                            d.icon(icon, 11.0, col);
                        },
                    );
                    v.bg(Color::srgba(0.1, 0.09, 0.08, 0.9)).border(BorderColor::all(ring));
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
            Some(sim::maintenance::Refit::Pebbles)=>"Facing with pebbles",
            None=>"Building",
        };
        return Some(if !waiting.is_empty() && !site.labour_open() {(None,"Waiting".into(),waiting,WARN)}
            else if slots>0 && empty==slots {(Some(Icon::RoleBuilder),"No builders".into(),vec![],MUTED)}
            else {(Some(Icon::Hammer),stage.into(),vec![],GOOD)});
    }
    if b.is_complete() && !b.operational() {return Some((Some(Icon::Hammer),"Ruined".into(),vec![],BAD));}
    Some(match (b.status, place.and_then(|pl| pl.site.as_ref())) {
        (Status::Complete, _) if sim::buildings::flooded(b, &s.water.depth) => (Some(Icon::Flood), "Flooded".into(), vec![], BAD),
        (Status::Complete, _) if s.buildings.stranded(b, &s.water.depth) => (Some(Icon::Flood), "Dry".into(), vec![], WARN),
        (Status::Complete, _) if slots > 0 && empty == slots => (Some(Icon::Unstaffed), "Idle".into(), vec![], WARN),
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
