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
    pub works_cell: Option<Cell>,
    pub problems: &'a [Problem],
    pub minimap: Option<&'a super::minimap::MinimapImage>,
    pub portrait: Option<&'a crate::entities_render::portrait::PortraitImage>,
    pub view_cell: (f32, f32),
    pub cursor: Option<Vec2>,
    pub journal: &'a [super::Notice],
    pub now: f32,
}

impl Ctx<'_> { pub fn placing(&self) -> bool { self.hud.tool != Tool::None } }

pub const RIGHT_W: f32 = 320.0;
pub const LEGEND_TOP: f32 = 52.0;
const BADGE: f32 = 44.0;

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    let v = |x: Option<f32>| x.map_or(Val::Auto, Val::Px);
    Node { position_type: PositionType::Absolute, left: v(left), right: v(right), top: v(top), bottom: v(bottom), ..default() }
}

fn panel_box(node: Node, f: impl FnOnce(&mut Ui)) -> VNode {
    let mut ui = Ui::default();
    ui.node(node, f);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v
}

fn centred(bottom: f32, child: VNode) -> VNode {
    let mut v = VNode::new(Node { justify_content: JustifyContent::Center, ..abs(Some(0.0), Some(0.0), None, Some(bottom)) });
    v.children.push(child);
    v
}

pub fn food_days(s: &sim::SimState) -> Option<f32> { Some(sim::hud::food_days_left(s)).filter(|d| d.is_finite()) }

pub fn breakdown_lines(s: &sim::SimState, b: &Breakdown, signed_values: bool) -> Vec<TipLine> {
    b.rows.iter().map(|r| {
        let v = if signed_values { signed(r.value) } else { num(r.value) };
        let col = if !signed_values { INK } else if r.value > 0.05 { GOOD } else if r.value < -0.05 { BAD } else { MUTED };
        let l = TipLine::new(r.label.clone(), v).color(col);
        match r.source {
            Source::Building(id) => s.buildings.building(id).map_or(l.clone(), |bb| l.icon(building_icon(bb.kind), INK)),
            Source::Beaver(id) => s.colony.beaver(id).map_or(l.clone(), |bv| l.icon(stage_icon(bv.stage), INK)),
            _ => l,
        }
    }).collect()
}

pub(super) fn staple(g: Good) -> bool {
    g.row().edible || BuildingKind::all().any(|k| k.spec().cost.iter().any(|c| c.0 == g))
}

pub fn good_tip(s: &sim::SimState, g: Good) -> Tip {
    let st = &s.buildings.stores;
    let mut t = Tip::titled(Some((good_icon(g), good_color(g))), g.name()).with_value(format!("{}/{}", st.amount(g).round() as i64, st.capacity(g).round() as i64));
    if st.capacity(g) <= 0.0 {
        let hint = BuildingKind::store_for(g).map_or_else(|| "Build a store".into(), |k| format!("Build a {}", k.name().to_lowercase()));
        t.push(TipLine::new("No place to store goods", hint).icon(Icon::Warn, BAD).color(BAD));
    }
    if g == Good::food() { t.push(TipLine::new("Spoils", format!("{:.1}%/d", sim::colony::FOOD_SPOILAGE * 100.0))); }
    if !s.ledger.has_data() { t.push(TipLine::new("Per day", "-")); return t; }
    let (inn, out) = (sim::hud::good_in_breakdown(s, g), sim::hud::good_out_breakdown(s, g));
    let mut tin = Tip::titled(Some((good_icon(g), good_color(g))), "Gathered").with_value(format!("{}/d", signed(inn.total)));
    for l in breakdown_lines(s, &inn, true) { tin.push(l); }
    let mut tout = Tip::titled(Some((good_icon(g), good_color(g))), "Used").with_value(format!("{}/d", signed(-out.total.abs())));
    for mut l in breakdown_lines(s, &out, true) {
        if !l.value.starts_with('-') && l.value != "0" { l.value = format!("-{}", l.value.trim_start_matches('+')); l.color = Some(BAD); }
        tout.push(l);
    }
    t.push(TipLine::new("Gathered", format!("{}/d", signed(inn.total))).color(GOOD).nested(tin));
    t.push(TipLine::new(if g == Good::food() { "Eaten, used" } else { "Used" }, format!("{}/d", signed(-out.total.abs()))).color(BAD).nested(tout));
    let left = if g == Good::food() { food_days(s) } else { s.ledger.days_left(g, st.amount(g)) };
    if let Some(d) = left {
        t.push(TipLine::new("Empty in", days(d)).icon(Icon::Hourglass, WARN).color(WARN));
        if g == Good::food() {
            let gap = sim::hud::food_need_per_day(s) - inn.total;
            if gap > 0.05 { t.push(TipLine::new("Short", num(gap)).color(WARN)); }
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
    let empty = s.buildings.buildings.iter().filter(|b| b.is_home_ready())
        .filter(|b| !s.colony.beavers.iter().any(|bv| bv.lodge == Some(b.id))).count();
    t.push(TipLine::new("Empty lodges", format!("{empty}")).icon(building_icon(BuildingKind::home()), INK));
    let mut more = Tip::name("Beavers");
    for l in breakdown_lines(s, &sim::hud::headcount_breakdown(s), false) { more.push(l); }
    t.push(TipLine::new("Housing", "").nested(more));
    t.push(match s.society.newcomers(s.time_days, &s.colony, s.buildings.stores.amount(Good::food())) {
        Ok(n) => TipLine::new("Newcomers", format!("{}/y", num(n))),
        Err(why) => TipLine::new("Newcomers", why).color(MUTED),
    });
    t.key("Tab")
}

pub fn stage_icon(st: Stage) -> Icon { match st { Stage::Kit => Icon::Kit, Stage::Yearling => Icon::Yearling, Stage::Adult => Icon::Adult } }

pub fn idle_reason_word(r: sim::jobs::IdleReason) -> &'static str {
    use sim::jobs::IdleReason::*;
    match r { NoOpenSlot => "No open job", Unreachable => "Unreachable", Resting => "Resting", Paused => "Job paused", Free => "Free" }
}

pub fn strip(c: &Ctx) -> Vec<VNode> {
    let (s, stores) = (c.s, &c.s.buildings.stores);
    let idle = sim::jobs::idle_reasons(s);
    let free: Vec<_> = idle.iter().filter(|(_, r)| *r != sim::jobs::IdleReason::Resting).collect();
    vec![panel_box(Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..abs(Some(12.0), None, Some(10.0), None) }.with_row(16.0), |u| {
        for g in Good::all() {
            let have = stores.amount(g);
            if !staple(g) && have < 0.5 { continue; }
            let short = have < 0.5 && s.ledger.days_left(g, have).is_some();
            u.row(6.0, |r| { r.icon(good_icon(g), 20.0, good_color(g)); r.bold(format!("{}", have.round() as i64), 16.0, if short { BAD } else { INK }); }).tip(good_tip(s, g));
        }
        u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(20.0), ..default() })).bg(GROOVE);
        let mut tip = headcount_tip(s);
        if !free.is_empty() {
            tip.push(TipLine::new("Free now", format!("{}", free.len())).icon(Icon::Idle, ACCENT));
            for (id, why) in free.iter().take(6) {
                if let Some(b) = s.colony.beaver(*id) { tip.push(TipLine::new(b.name.clone(), idle_reason_word(*why)).icon(stage_icon(b.stage), MUTED).sub()); }
            }
        }
        u.row(6.0, |r| {
            r.icon(Icon::Beaver, 20.0, FUR);
            r.bold(format!("{}", s.colony.population()), 16.0, INK);
            if !free.is_empty() { r.add(VNode::new(Node { width: Val::Px(8.0), height: Val::Px(8.0), border_radius: BorderRadius::all(Val::Px(4.0)), margin: UiRect::bottom(Val::Px(12.0)), ..default() })).bg(ACCENT); }
        }).tip(tip).action(if free.is_empty() { HudAction::Colony(Some(ColonyTab::Population)) } else { HudAction::CycleIdle });
    })]
}

trait NodeExt { fn with_row(self, gap: f32) -> Self; fn with_col(self, gap: f32) -> Self; }
impl NodeExt for Node {
    fn with_row(mut self, gap: f32) -> Self { self.flex_direction = FlexDirection::Row; self.align_items = AlignItems::Center; self.column_gap = Val::Px(gap); self }
    fn with_col(mut self, gap: f32) -> Self { self.flex_direction = FlexDirection::Column; self.row_gap = Val::Px(gap); self }
}

fn season_color(se: Season) -> Color {
    match se { Season::Spring => Color::srgb(0.5, 0.74, 0.42), Season::Summer => Color::srgb(0.86, 0.7, 0.36), Season::Autumn => Color::srgb(0.72, 0.5, 0.33), Season::Winter => Color::srgb(0.8, 0.84, 0.9) }
}

fn weather(r: &mut Ui, s: &sim::SimState) {
    use sim::weather::Kind;
    let now = s.water.sky.at(s.time_days);
    let (icon, col) = match now.kind {
        Kind::Clear => (Icon::Summer, Color::srgb(0.95, 0.8, 0.4)),
        Kind::Cloudy => (Icon::Cloud, MUTED),
        Kind::Rain => (Icon::Rain, WATER),
        Kind::Storm => (Icon::Storm, Color::srgb(0.62, 0.7, 0.92)),
        Kind::Snow => (Icon::Winter, season_color(Season::Winter)),
        Kind::Blizzard => (Icon::Blizzard, season_color(Season::Winter)),
    };
    let (bitter, deg) = (s.agents.holed_up, crate::menu::degrees);
    let mut tip = Tip::titled(Some((icon, col)), now.kind.name()).line("Air", deg(now.temp_c));
    if now.temp_c - now.feels_c >= 2.0 { tip = tip.line("Feels", deg(now.feels_c)); }
    let falling = (now.rain + now.snow) * 1000.0;
    if falling >= 0.5 { tip = tip.line(if now.snow > 0.0 { "Snow" } else { "Rain" }, format!("{falling:.0} mm/day")); }
    if now.ground_frozen() { tip = tip.line("Ground frozen", ""); }
    if bitter { tip = tip.line("Beavers keep to their lodges", ""); }
    r.row(4.0, |w| { w.icon(icon, 15.0, col); w.text(deg(now.temp_c), 12.0, if bitter { WARN } else { MUTED }); }).tip(tip);
}

pub fn calendar(c: &Ctx) -> Vec<VNode> {
    let (s, d, clock) = (c.s, Date::at(c.s.time_days), c.clock);
    vec![panel_box(Node { padding: UiRect::axes(Val::Px(12.0), Val::Px(9.0)), ..abs(None, Some(12.0), Some(10.0), None) }.with_row(12.0), |u| {
        let ang = calendar::year_phase(s.time_days) * std::f32::consts::TAU;
        u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |ring| {
            ring.bold(format!("{}", d.day), 14.0, INK);
            ring.add(VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(26.0 + 22.0 * ang.sin() - 5.0), top: Val::Px(26.0 - 22.0 * ang.cos() - 5.0), width: Val::Px(10.0), height: Val::Px(10.0), border: UiRect::all(Val::Px(2.0)), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() }))
                .bg(INK).border(BorderColor::all(PANEL));
        }).skin(Skin::SeasonRing).tip(Tip::titled(Some((season_icon(d.season), season_color(d.season))), d.season.name()));
        u.col(8.0, |x| {
            x.row(8.0, |r| { r.bold(format!("Year {}", d.year), 13.0, INK); r.spacer(); weather(r, s); });
            x.row(5.0, |r| {
                r.pebble(Icon::Pause, 13.0, HudAction::Pause, if clock.paused { Peb::On } else { Peb::Idle }).tip(Tip::name("Pause").key("Space"));
                for (sp, icon, key, name) in [(Speed::X1, Icon::Play, "1", "Normal speed"), (Speed::X2, Icon::Fast2, "2", "Faster"), (Speed::X3, Icon::Fast3, "3", "Fast"), (Speed::X4, Icon::Fast3, "4", "Dev speed")] {
                    if sp.allowed() {
                        r.pebble(icon, 13.0, HudAction::Speed(sp), if !clock.paused && clock.speed == sp { Peb::On } else { Peb::Idle }).tip(Tip::name(name).key(key));
                    }
                }
            });
        });
    })]
}

fn alert_face(s: &sim::SimState, p: &Problem) -> (Vec<(Icon, Color)>, String, &'static str) {
    let v = p.value;
    let role_icon = || match p.subject { Some(Subject::Role(r)) => job_icon(r), _ => Icon::Jobs };
    let bld_icon = || building_icon(match p.subject { Some(Subject::Building(k)) => k, _ => BuildingKind::home() });
    let good = || match p.subject { Some(Subject::Good(g)) => g, _ => sim::dams::material() };
    let (dam_i, canal_i, build_i) = (building_icon(BuildingKind::dam()), building_icon(BuildingKind::canal()), job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)));
    match p.kind {
        ProblemKind::Food => (vec![(good_icon(Good::food()), good_color(Good::food()))], if v < 0.5 { "0".into() } else { days(v) }, "Food runs out"),
        ProblemKind::WinterFood => (vec![(Icon::Winter, MUTED), (good_icon(Good::food()), good_color(Good::food()))], format!("{} / {}d", days(v), sim::calendar::SEASON_DAYS as i64), "Prepare food for winter"),
        ProblemKind::Idle => (vec![(Icon::Idle, WARN)], format!("{}", v as i64), "Free beavers"),
        ProblemKind::NoBuilders => (vec![(Icon::Site, INK), (build_i, MUTED)], format!("{}", v as i64), "Sites without builders"),
        ProblemKind::NoStorage => (vec![(Icon::Warn, BAD)], String::new(), "No place to store goods"),
        ProblemKind::StoreFull => { let g = good(); (vec![(good_icon(g), good_color(g)), (Icon::Full, WARN)], format!("{v:.0}%"), "Store full") }
        ProblemKind::WeakDam => (vec![(dam_i, INK)], format!("{v:.0}%"), "Dam weakening"),
        ProblemKind::Maintenance => (vec![(Icon::Hammer, WARN)], format!("{v:.0}%"), "Building needs repair"),
        ProblemKind::ForestFalling => (vec![(resource_icon(sim::resources::ResourceKind::tree()), INK), (Icon::Down, WARN)], format!("-{v:.0}%"), "Forest falling"),
        ProblemKind::CanalDry => (vec![(canal_i, INK), (Icon::Drop, WARN)], String::new(), "Canal dry"),
        ProblemKind::CanalSilting => (vec![(canal_i, INK), (Icon::Down, WARN)], format!("{v:.0}%"), "Canal silting"),
        ProblemKind::PondDraining => (vec![(dam_i, INK), (Icon::Flood, WARN)], format!("-{v:.0}cm"), "Pond draining"),
        ProblemKind::Wolf => (vec![(Icon::Wolf, BAD)], String::new(), "Wolf"),
        ProblemKind::NoFreeLodge => (vec![(building_icon(BuildingKind::home()), INK), (Icon::Warn, WARN)], format!("{}", v as i64), "No free lodge"),
        ProblemKind::GroundFrozen => (vec![(role_icon(), INK), (Icon::Winter, season_color(Season::Winter))], format!("{}", v as i64), "Ground frozen"),
        ProblemKind::NoWork => (vec![(role_icon(), INK), (Icon::Idle, WARN)], format!("{}", v as i64), match p.subject { Some(Subject::Role(r)) => phrase("No work for", &r.row().plural), _ => "No work" }),
        ProblemKind::WaitingFor => { let g = good(); (vec![(build_i, INK), (good_icon(g), good_color(g))], format!("{}", v as i64), good_phrase("Builders waiting for", g)) }
        ProblemKind::Flooded => (vec![(bld_icon(), INK), (Icon::Flood, BAD)], format!("{}", v as i64), "Flooded"),
        ProblemKind::HighAndDry => (vec![(bld_icon(), INK), (Icon::Down, WARN)], format!("{}", v as i64), "High and dry"),
        ProblemKind::Stranger => {
            let st = match p.subject { Some(Subject::Stage(st)) => stage_icon(st), _ => Icon::Beaver };
            let name = match p.fix { Fix::Accept(id) => s.society.pending().iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default(), _ => String::new() };
            (vec![(st, INK)], name, "Asks to join")
        }
    }
}

fn fix_face(f: &Fix) -> Option<(Icon, String, String)> {
    Some(match f {
        Fix::Gather { kind, n } if kind.is_food_plant() => (good_icon(Good::food()), format!("+{n}"), format!("Send {n} beavers to gather food")),
        Fix::Gather { kind, n } => (resource_icon(*kind), format!("+{n}"), format!("{n} gather {}", resource_name(*kind).to_lowercase())),
        Fix::Place(k) => (building_icon(*k), "+".into(), format!("Place {}", k.spec().name.to_lowercase())),
        Fix::OpenDam { .. } => (building_icon(BuildingKind::dam()), String::new(), "Open the dam".into()),
        Fix::RepairBuilding { .. } => (Icon::Hammer, "Repair".into(), "Repair with materials and menders".into()),
        Fix::PutToWork { role, n } => (job_icon(*role), format!("+{n}"), format!("{n} to {}", role.name().to_lowercase())),
        Fix::OpenJobs => (Icon::Jobs, String::new(), "Open jobs".into()),
        Fix::Accept(_) => (Icon::Check, String::new(), "Welcome".into()),
        Fix::Decline(_) => (Icon::Cross, String::new(), "Turn away".into()),
        Fix::Focus => return None,
    })
}

pub fn alert_token(s: &sim::SimState, p: &Problem) -> (Icon, Color, String, &'static str, Skin) {
    let (icons, value, name) = alert_face(s, p);
    let (icon, col) = icons.first().copied().unwrap_or((Icon::Warn, WARN));
    (icon, col, value.split_whitespace().next().unwrap_or("").to_string(), name, if p.severity == Severity::Urgent { Skin::TokenBad } else { Skin::TokenWarn })
}

pub fn alert_tip(c: &Ctx, p: &Problem) -> Tip { alert_tip_in(c.s, p) }

pub fn alert_tip_in(s: &sim::SimState, p: &Problem) -> Tip {
    let (icons, value, name) = alert_face(s, p);
    let mut cause = Tip::titled(icons.first().copied(), name).with_value(value);
    for l in breakdown_lines(s, &p.cause, p.kind == ProblemKind::Food) { cause.push(l); }
    if p.kind == ProblemKind::Wolf && cause.lines.is_empty() { cause.push(TipLine::new("Wolf sighted", "Click to locate")); }
    for f in std::iter::once(&p.fix).chain(p.alt_fix.iter()) {
        if let Some((icon, _, words)) = fix_face(f) { cause.push(TipLine::new(words, "").icon(icon, INK).act(HudAction::Fix(*f, p.focus))); }
    }
    cause
}

pub fn badges(c: &Ctx) -> Vec<VNode> {
    if c.problems.is_empty() { return Vec::new(); }
    let open = c.hud.open_badge.as_deref();
    let mut col = VNode::new(Node { align_items: AlignItems::Center, ..abs(Some(0.0), Some(0.0), Some(12.0), None) }.with_col(8.0));
    let mut ui = Ui::default();
    let mut card = None;
    ui.row(6.0, |r| {
        for p in c.problems {
            let key = format!("alert:{:?}:{:?}", p.kind, p.subject);
            let (icon, col, value, name, skin) = alert_token(c.s, p);
            let t = r.node(Node { width: Val::Px(BADGE), height: Val::Px(BADGE), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |k| {
                k.icon(icon, if value.is_empty() { 22.0 } else { 18.0 }, col);
                if !value.is_empty() { k.bold(value.clone(), 11.0, INK); }
            });
            t.skin(skin).tip(alert_tip(c, p)).action(if p.kind == ProblemKind::Wolf { HudAction::FocusWolf } else { HudAction::Badge(key.clone()) });
            if open == Some(key.as_str()) {
                let mut v = panel_box(Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)), ..default() }.with_col(6.0), |k| {
                    k.row(8.0, |row| {
                        row.icon(icon, 20.0, col);
                        row.bold(name, 16.0, INK);
                        if !value.is_empty() { row.bold(value.clone(), 16.0, MUTED); }
                    });
                    k.row(6.0, |row| {
                        if let Some(f) = p.focus { row.pebble(Icon::Focus, 12.0, HudAction::Fix(Fix::Focus, Some(f)), Peb::Idle).tip(Tip::name("Go there")); }
                        for f in std::iter::once(&p.fix).chain(p.alt_fix.iter()) {
                            if let Some((icon, _, words)) = fix_face(f) { row.pebble(icon, 12.0, HudAction::Fix(*f, p.focus), Peb::Good).tip(Tip::name(words)); }
                        }
                    });
                });
                v.tip(alert_tip(c, p));
                card = Some(v);
            }
        }
    });
    col.children = ui.nodes;
    if let Some(card) = card { col.children.push(card); }
    vec![col]
}

pub fn right(c: &Ctx) -> Vec<VNode> {
    let mut ui = Ui::default();
    if let Some(id) = c.hud.peek.filter(|id| c.s.colony.beaver(*id).is_some()) {
        if let Some(v) = super::inspect::inspector(c, Selection::Beaver(id)) { ui.add(v); }
    } else if c.hud.tool == Tool::None {
        for &p in c.hud.pinned.iter().filter(|p| **p != c.sel).chain(std::iter::once(&c.sel)) {
            if let Some(v) = super::inspect::inspector(c, p) { ui.add(v); }
        }
    }
    if ui.nodes.is_empty() { return Vec::new(); }
    let mut col = VNode::new(Node { width: Val::Px(RIGHT_W + 14.0), align_items: AlignItems::FlexEnd, ..abs(None, Some(12.0), Some(84.0), Some(super::journal::height(c) + 20.0)) }.with_col(8.0));
    col.children = ui.nodes;
    col.scroll_y();
    col.solid = false;
    vec![col]
}

pub fn rail(c: &Ctx) -> Vec<VNode> {
    vec![panel_box(Node { padding: UiRect::all(Val::Px(8.0)), align_items: AlignItems::Center, ..abs(Some(12.0), None, Some(84.0), None) }.with_col(6.0), |u| {
        for (l, icon, name, key) in [(MapLens::World, Icon::ViewWorld, "Normal", "Esc"), (MapLens::Water, Icon::ViewWater, "Water", "F1"), (MapLens::Forest, Icon::ViewForest, "Forest", "F2"), (MapLens::Elevation, Icon::ViewElevation, "Height", "F3"), (MapLens::Deposits, Icon::ViewDeposits, "Deposits", "F4"), (MapLens::Paths, Icon::Walk, "Paths", "F5")] {
            let on = c.lens == l;
            u.pebble(icon, 17.0, HudAction::SetLens(if l == MapLens::World || on { MapLens::World } else { l }), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name).key(key));
        }
        u.rule().style(|n| n.width = Val::Px(28.0));
        let (co, lo) = (c.hud.colony.is_some(), c.hud.ledger.is_some());
        u.pebble(Icon::Colony, 17.0, HudAction::Colony(if co { None } else { Some(ColonyTab::Jobs) }), if co { Peb::On } else { Peb::Idle }).tip(Tip::name("Colony").key("Tab"));
        u.pebble(Icon::Territory, 17.0, HudAction::Ledger(if lo { None } else { Some(LedgerTab::Charts) }), if lo { Peb::On } else { Peb::Idle }).tip(Tip::name("Territory").key("L"));
    })]
}

fn item_tip(kind: BuildingKind) -> Tip {
    let spec = kind.spec();
    let mut t = Tip::titled(Some((building_icon(kind), INK)), spec.name.as_str());
    if !spec.summary.is_empty() { t.push(TipLine::new(spec.summary.as_str(), "")); }
    if kind.is_dam() { t.push(TipLine::new("Per load", "raises one cell")); }
    let cost = if kind.is_dam() { build_logic::path_cost(kind, 1) } else { spec.cost.to_vec() };
    for (g, a) in cost { t.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)).sub()); }
    if kind.is_canal() {
        if let Some(g) = sim::canals::spoil_good() { t.push(TipLine::new(g.name(), "+").icon(good_icon(g), good_color(g)).color(GOOD).sub()); }
    }
    t
}

pub fn bottom(c: &Ctx) -> Vec<VNode> {
    vec![centred(12.0, panel_box(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)), ..default() }.with_row(7.0), |u| {
        let mut menu: Vec<BuildingKind> = BuildingKind::all().filter(|k| k.row().menu.is_some()).collect();
        menu.sort_by_key(|k| k.row().menu);
        let groove = menu.iter().position(|k| !k.is_waterworks()).filter(|&i| i > 0);
        for (i, kind) in menu.into_iter().enumerate() {
            if Some(i) == groove { u.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(30.0), margin: UiRect::horizontal(Val::Px(3.0)), ..default() })).bg(GROOVE); }
            let locked = c.s.unlocked(kind).err();
            let look = if c.hud.tool == Tool::of(kind) { Peb::On } else if locked.is_some() { Peb::Faded } else { Peb::Idle };
            let mut tip = item_tip(kind).key(kind.row().hotkey.as_deref().unwrap_or(""));
            if let Some(e) = locked { tip = tip.line(e.to_string(), ""); }
            u.pebble(building_icon(kind), 21.0, HudAction::Pick(kind), look).tip(tip);
        }
    }))]
}

pub fn settle_chip(_c: &Ctx) -> Vec<VNode> {
    vec![centred(12.0, panel_box(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..default() }.with_row(12.0), |u| {
        u.row(4.0, |r| { r.icon(Icon::Beaver, 22.0, INK); r.bold("4", 14.0, INK); }).tip(Tip::titled(Some((Icon::Beaver, INK)), "The family").line("Adults", "2").line("Young", "2"));
        u.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(22.0), ..default() })).bg(LINE);
        u.row(4.0, |r| { r.icon(Icon::Home, 18.0, ACCENT); r.keycap("LMB"); }).tip(Tip::name("Settle here").line("Pick the spot", ""));
    }))]
}

pub fn settle_tag(s: &sim::SimState, _flow: &crate::flow::Flow, cell: Cell, cur: Vec2) -> Vec<VNode> {
    let mut v = VNode::new(Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(5.0)), ..abs(Some(cur.x + 18.0), None, Some(cur.y + 14.0), None) });
    v.bg = Color::srgba(0.08, 0.07, 0.06, 0.88);
    let mut ui = Ui::default();
    let band = |x: f32| if x >= 0.5 { GOOD } else if x >= 0.3 { WARN } else { BAD };
    match crate::flow::can_settle(s, cell) {
        Ok(()) => {
            let h = crate::flow::site_hint(s, cell);
            let col = band(h.overall());
            ui.icon(Icon::Home, 14.0, col);
            for (i, x) in [(Icon::Drop, h.water), (resource_icon(sim::resources::ResourceKind::tree()), h.trees), (good_icon(Good::food()), h.food), (Icon::Health, h.safety), (Icon::Building, h.room)] {
                ui.icon(i, 12.0, MUTED);
                ui.bar(x, 22.0, 4.0, band(x));
            }
            ui.bold(h.verdict(), 12.0, col);
        }
        Err(e) => { ui.icon(Icon::Cross, 14.0, BAD); ui.bold(super::settle_error(e), 12.0, INK); }
    }
    v.children = ui.nodes;
    vec![v]
}

pub fn order_crew(s: &sim::SimState, hud: &HudState) -> Vec<sim::colony::BeaverId> {
    if !hud.selected.is_empty() { hud.selected.clone() } else { sim::hud::idle_beavers(s) }
}

pub fn cursor_tag(c: &Ctx) -> Vec<VNode> {
    let Some(cur) = c.cursor else { return Vec::new() };
    let tag_node = Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..abs(Some(cur.x + 20.0), None, Some(cur.y + 16.0), None) };
    let mut v = VNode::new(tag_node);
    let mut ui = Ui::default();
    match (c.hud.tool, c.hover) {
        (Tool::Harvest(h), _) => {
            let n = order_crew(c.s, c.hud).len();
            ui.icon(if h == Harvest::Cut { resource_icon(sim::resources::ResourceKind::tree()) } else { resource_icon(sim::resources::ResourceKind::food_plant(false)) }, 16.0, GOOD);
            ui.icon(Icon::Beaver, 16.0, if n == 0 { BAD } else { FUR });
            ui.bold(format!("{n}"), 14.0, if n == 0 { BAD } else { INK });
            v.skin(if n == 0 { Skin::PlateBad } else { Skin::Plate });
        }
        (Tool::Building(kind), Some(cell)) => {
            let pv = build_logic::site_preview(c.s, kind, cell);
            v.skin(if pv.verdict.is_ok() { Skin::Plate } else { Skin::PlateBad });
            match &pv.verdict {
                Ok(()) => {
                    ui.row(6.0, |r| for &(g, a) in &kind.spec().cost { r.row(2.0, |x| { x.icon(good_icon(g), 13.0, good_color(g)); x.bold(num(a), 12.0, INK); }); });
                    ui.icon(Icon::Check, 14.0, GOOD);
                }
                Err(e) => {
                    ui.icon(Icon::Cross, 14.0, BAD);
                    let w = place_error(e);
                    if !w.is_empty() { ui.bold(w, 12.0, INK); }
                }
            }
        }
        _ => return Vec::new(),
    }
    v.children = ui.nodes;
    vec![v]
}

fn bad_icon_badge(r: &mut Ui, icon: Icon, icon_col: Color, title_icon: Icon, why: &str) {
    r.node(Node { width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, |x| {
        x.icon(icon, 20.0, icon_col);
        x.icon(Icon::Cross, 12.0, BAD).style(|n| { n.position_type = PositionType::Absolute; n.right = Val::Px(-3.0); n.bottom = Val::Px(-3.0); });
    }).tip(Tip::titled(Some((title_icon, INK)), why));
}

/// Shared floating/pinned tag shell for both `dam_tag` and `canal_tag`.
#[allow(clippy::too_many_arguments)]
fn line_tag_shell(at: Vec2, pinned: bool, win: Vec2, width: (f32, f32), confirm_word: &str, refusal: Option<&str>, body: impl FnOnce(&mut Ui)) -> Vec<VNode> {
    let bad = refusal.is_some();
    let mut ui = Ui::default();
    let row = ui.row(9.0, |r| {
        body(r);
        if pinned {
            r.add(VNode::new(Node { width: Val::Px(1.0), height: Val::Px(24.0), ..default() })).bg(GROOVE);
            if let Some(why) = refusal {
                r.pebble_opt(Icon::Check, 15.0, None, Peb::Faded).tip(Tip::name(confirm_word).line(why, ""));
            } else {
                r.pebble(Icon::Check, 16.0, HudAction::ConfirmLine, Peb::Primary).tip(Tip::name(confirm_word).key("Enter"));
            }
            r.pebble(Icon::Cross, 12.0, HudAction::CancelTool, Peb::Idle).tip(Tip::name("Drop the line").key("Esc"));
        }
    });
    panel(row);
    row.node.padding = UiRect::axes(Val::Px(12.0), Val::Px(8.0));
    if bad { row.skin(Skin::PlateBad); }
    let w = if pinned { width.0 } else { width.1 };
    let (left, top) = if pinned {
        ((at.x - 26.0).clamp(8.0, (win.x - w - 8.0).max(8.0)), if at.y + 86.0 < win.y { at.y + 26.0 } else { at.y - 70.0 })
    } else {
        ((at.x + 20.0).min((win.x - w - 8.0).max(8.0)), at.y + 18.0)
    };
    let mut v = ui.nodes.pop().unwrap();
    v.node.position_type = PositionType::Absolute;
    (v.node.left, v.node.top) = (Val::Px(left), Val::Px(top));
    if !pinned { v.solid = false; }
    vec![v]
}

pub fn canal_tag(c: &Ctx, at: Vec2, pinned: bool, win: Vec2) -> Vec<VNode> {
    let plan = c.hud.canal_plan.as_ref().filter(|p| p.0 == c.hud.line).map(|p| &p.1);
    if plan.is_none() && c.hud.extending.is_none() { return Vec::new(); }
    let refusal = match plan { Some(Err(e)) => Some(e.clone()), Some(Ok(p)) => sim::canals::check(p).err(), None => None };
    line_tag_shell(at, pinned, win, (250.0, 170.0), "Dig", refusal.as_deref(), |r| match (&refusal, plan) {
        (Some(why), _) => {
            let ci = building_icon(BuildingKind::canal());
            bad_icon_badge(r, ci, BAD, ci, why);
            r.bold(why.clone(), 14.0, BAD);
        }
        (None, Some(Ok(p))) => {
            r.row(4.0, |x| { x.icon(Icon::Beaver, 17.0, FUR); x.bold(format!("{}d", p.labour_days().ceil().max(1.0) as i64), 14.0, INK); })
                .tip(Tip::titled(Some((Icon::Beaver, INK)), "Digging").line("Beaver-days", num(p.labour_days())));
            if let Some(g) = sim::canals::spoil_good() {
                r.row(4.0, |x| { x.icon(good_icon(g), 17.0, good_color(g)); x.bold(format!("+{}", p.mud().round() as i64), 14.0, INK); })
                    .tip(Tip::titled(Some((good_icon(g), good_color(g))), format!("{} from the spoil", g.name())));
            }
            r.row(4.0, |x| { x.icon(Icon::Logs, 17.0, WATER); x.bold(format!("{:.0}%", p.floatable_share() * 100.0), 14.0, INK); })
                .tip(Tip::titled(Some((Icon::Logs, WATER)), "Floats a load").line(if p.through { "Runs into other water" } else { "Ends on land" }, ""));
        }
        _ => { r.icon(Icon::Extend, 18.0, INK).tip(Tip::name("Extend")); }
    })
}

pub fn dam_tag(c: &Ctx, at: Vec2, pinned: bool, win: Vec2) -> Vec<VNode> {
    let Some(plan) = c.hud.plan.as_ref().filter(|p| p.line == c.hud.line) else { return Vec::new() };
    line_tag_shell(at, pinned, win, (230.0, 150.0), "Build", plan.refusal.as_ref().map(|r| r.1.as_str()), |r| match &plan.refusal {
        Some((icon, why)) => bad_icon_badge(r, *icon, if *icon == Icon::Drop { WATER } else { BAD }, building_icon(BuildingKind::dam()), why),
        None => {
            let g = sim::dams::material();
            r.row(4.0, |x| { x.icon(good_icon(g), 18.0, good_color(g)); x.bold(format!("{}", plan.sticks.round() as i64), 14.0, INK); })
                .tip(Tip::titled(Some((good_icon(g), good_color(g))), "For the first stage").line("Carried load by load", ""));
            let look = c.hud.dam_settled();
            let rise = look.map(|r| if r.water.rise < 0.05 { "+0".into() } else { format!("+{:.1} m", r.water.rise) });
            let mut tip = Tip::titled(Some((Icon::Drop, WATER)), "Water rises").with_value(rise.clone().unwrap_or_else(|| "…".into()));
            if let Some(r) = look { tip = tip.line("New pond cells", format!("{}", r.new_wet)); }
            if plan.preview.as_ref().is_some_and(|p| p.spill_at.is_some()) { tip = tip.line("Runs around the dam", ""); }
            r.row(4.0, |x| {
                x.icon(Icon::Drop, 17.0, WATER);
                x.bold(rise.as_deref().unwrap_or("…"), 14.0, if rise.is_some() { INK } else { MUTED });
            }).tip(tip);
            let drowned = c.hud.drowned.as_ref().filter(|d| d.0 == plan.line).map_or(&[][..], |d| &d.1[..]);
            if !drowned.is_empty() {
                let mut tip = Tip::titled(Some((Icon::Flood, BAD)), "Buildings flooded").with_value(format!("{}", drowned.len()));
                for b in drowned.iter().filter_map(|id| c.s.buildings.building(*id)) { tip.push(TipLine::new(b.kind.spec().name.as_str(), "").icon(building_icon(b.kind), INK).sub()); }
                r.row(3.0, |x| { x.icon(Icon::Warn, 15.0, BAD); x.icon(Icon::Building, 15.0, BAD); x.bold(format!("{}", drowned.len()), 14.0, BAD); }).tip(tip);
            }
            let trees = look.map_or(0, |r| r.trees.len());
            if trees > 0 {
                let ti = resource_icon(sim::resources::ResourceKind::tree());
                r.row(4.0, |x| { x.icon(ti, 16.0, WARN); x.bold(format!("{trees}"), 14.0, WARN); }).tip(Tip::titled(Some((ti, WARN)), "Trees drowned"));
            }
        }
    })
}

pub fn minimap(c: &Ctx) -> Vec<VNode> {
    let Some(map) = c.minimap else { return Vec::new() };
    const SIZE: f32 = 150.0;
    let at = |p: (f32, f32)| { let (u, v) = map.to_map(p); (u * SIZE, v * SIZE) };
    let (cx, cy) = at(c.view_cell);
    let markers: Vec<_> = c.problems.iter().filter_map(|p| {
        let f = match p.kind { ProblemKind::Wolf => sim::predators::in_sight(c.s), _ => p.focus.map(|f| (f.0 as f32 + 0.5, f.1 as f32 + 0.5)) }?;
        let (x, y) = at(f);
        let &(icon, col) = alert_face(c.s, p).0.first()?;
        ((0.0..=SIZE).contains(&x) && (0.0..=SIZE).contains(&y)).then_some((x, y, icon, col, if p.severity == Severity::Urgent { BAD } else { WARN }, p))
    }).collect();
    let mut out = vec![panel_box(Node { padding: UiRect::all(Val::Px(8.0)), ..abs(Some(12.0), None, None, Some(12.0)) }.with_col(0.0), |u| {
        u.image(map.image.clone(), Node { width: Val::Px(SIZE), height: Val::Px(SIZE), ..default() }).tag(Tag::Minimap).solid().children.extend({
            let mut m = Ui::default();
            m.add(VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px((cx - 7.0).clamp(0.0, SIZE - 14.0)), top: Val::Px((cy - 7.0).clamp(0.0, SIZE - 14.0)), width: Val::Px(14.0), height: Val::Px(14.0), border: UiRect::all(Val::Px(2.0)), ..default() })).border(BorderColor::all(SELECT));
            for &(x, y, icon, col, ring, p) in &markers {
                m.node(Node { position_type: PositionType::Absolute, left: Val::Px((x - 9.0).clamp(0.0, SIZE - 18.0)), top: Val::Px((y - 9.0).clamp(0.0, SIZE - 18.0)), width: Val::Px(18.0), height: Val::Px(18.0), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::all(Val::Px(9.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, |d| { d.icon(icon, 11.0, col); })
                    .bg(Color::srgba(0.1, 0.09, 0.08, 0.9)).border(BorderColor::all(ring)).tip(alert_tip(c, p)).click_through();
            }
            m.nodes
        });
    })];
    let leg_at = abs(Some(12.0), None, Some(LEGEND_TOP), None);
    out.extend(super::paths::legend(c.s, leg_at.clone()));
    out.extend(super::water_legend::legend(leg_at));
    out
}

pub fn status_line(s: &sim::SimState, id: BuildingId) -> Option<(Option<Icon>, String, Vec<Good>, Color)> {
    let b = s.buildings.building(id)?;
    let place = s.work.place(id);
    let (empty, slots) = place.map_or((0, 0), |pl| (pl.slots.iter().filter(|sl| sl.beaver.is_none()).count(), pl.slots.len()));
    if place.is_some_and(|p| p.paused) { return Some((Some(Icon::Pause), "Paused".into(), vec![], MUTED)); }
    if let Some(d) = b.dam.as_deref() {
        let h = sim::dams::health(s, d);
        let work = sim::dams::has_work_for(s, id, false);
        return Some(if work { (Some(Icon::Hammer), "Building".into(), vec![], GOOD) }
            else if h < 0.8 { (Some(Icon::Warn), "Worn".into(), vec![], WARN) }
            else { (Some(Icon::Drop), "Holding".into(), vec![], GOOD) });
    }
    if let Some(k) = b.canal.as_deref() {
        let work = sim::canals::has_work_for(s, id, false);
        let wet = k.status.as_ref().is_some_and(|st| st.wet > 0);
        return Some(if work { (Some(Icon::Hammer), "Digging".into(), vec![], GOOD) }
            else if wet { (Some(Icon::Drop), "Flowing".into(), vec![], GOOD) }
            else { (Some(Icon::Drop), "Dry".into(), vec![], WARN) });
    }
    if let Some(site) = place.and_then(|p| p.site.as_ref()) {
        let waiting: Vec<Good> = site.needed.iter().filter(|a| site.missing(a.0) > 0.0).map(|a| a.0).collect();
        let stage = match s.maintenance.jobs.get(&id).copied() {
            Some(sim::maintenance::Refit::Repair | sim::maintenance::Refit::ReinforcedRepair) => "Repairing",
            Some(sim::maintenance::Refit::Reinforce) => "Reinforcing",
            Some(sim::maintenance::Refit::Facing) => "Facing with pebbles",
            None => "Building",
        };
        return Some(if !waiting.is_empty() && !site.labour_open() { (None, "Waiting".into(), waiting, WARN) }
            else if sim::agents::site_workers(s, id) == 0 { (Some(job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build))), "No builders".into(), vec![], MUTED) }
            else { (Some(Icon::Hammer), stage.into(), vec![], GOOD) });
    }
    if b.is_complete() && !b.operational() { return Some((Some(Icon::Hammer), "Ruined".into(), vec![], BAD)); }
    Some(match b.status {
        Status::Complete if sim::buildings::flooded(b, &s.water.depth) => (Some(Icon::Flood), "Flooded".into(), vec![], BAD),
        Status::Complete if s.buildings.stranded(b, &s.water.depth) => (Some(Icon::Flood), "Dry".into(), vec![], WARN),
        Status::Complete if slots > 0 && empty == slots => (Some(Icon::Unstaffed), "Idle".into(), vec![], WARN),
        Status::Complete => (None, "Working".into(), vec![], if empty > 0 { WARN } else { GOOD }),
        _ => (None, "Planned".into(), vec![], MUTED),
    })
}
