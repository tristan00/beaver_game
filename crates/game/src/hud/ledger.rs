//! The territory (L) and colony-wide tables: charts, dams, canals, buildings, beavers, and run stats.
use super::colony::{doing, token_px};
use super::panels::{status_line, Ctx};
use super::ui::*;
use super::words::*;
use super::{BeaverCol, HudAction, LedgerTab, Span};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingKind, Good};
use sim::colony::Beaver;

pub const LEDGER_W: f32 = 640.0;
const ROWS: usize = 16;
const BARS: usize = 60;
const CHART_W: f32 = 272.0;
const CHART_H: f32 = 52.0;

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let Some(tab) = c.hud.ledger else { return Vec::new() };
    let tabs = [
        (LedgerTab::Charts, Icon::Ledger, "Charts"),
        (LedgerTab::Dams, building_icon(BuildingKind::dam()), "Dams"),
        (LedgerTab::Canals, building_icon(BuildingKind::canal()), "Canals"),
        (LedgerTab::Buildings, Icon::Building, "Buildings"),
        (LedgerTab::Stores, building_icon(BuildingKind::store_for(sim::dams::material()).unwrap_or(BuildingKind::home())), "Stores"),
        (LedgerTab::Upkeep, Icon::Hammer, "Upkeep"),
    ];
    Ui::book_shell(
        LEDGER_W,
        |r| {
            for (t, icon, name) in tabs { r.pebble(icon, 16.0, HudAction::Ledger(Some(t)), if t == tab { Peb::On } else { Peb::Idle }).tip(Tip::name(name)); }
            r.spacer();
            if matches!(tab, LedgerTab::Charts | LedgerTab::Dams | LedgerTab::Canals) { span_chips(c, r); }
            r.close_button(HudAction::Ledger(None), true);
        },
        |body| match tab {
            LedgerTab::Charts => charts(c, body),
            LedgerTab::Dams => dams(c, body),
            LedgerTab::Canals => canals(c, body),
            LedgerTab::Buildings => buildings(c, body),
            LedgerTab::Stores => super::colony::stores(c, body),
            LedgerTab::Upkeep => super::costs::upkeep_tab(c, body),
        },
    )
}

pub(super) fn chip<'a>(r: &'a mut Ui, label: &str, on: bool, a: HudAction) -> &'a mut VNode {
    let v = r.node(Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, |x| {
        x.bold(label, 11.0, if on { INK } else { MUTED });
    });
    v.skin(if on { Skin::PebbleOn } else { Skin::Pebble }).action(a)
}

#[derive(Clone, Copy, Default)]
pub(super) struct Bar { pub(super) a: f32, pub(super) b: f32, pub(super) down: f32, pub(super) day0: u64, pub(super) day1: u64 }

fn bars(c: &Ctx, f: impl Fn(&sim::ledger::ColonyDay) -> (f32, f32, f32)) -> Vec<Bar> {
    let days = &c.s.ledger.colony;
    let span = match c.hud.span { Span::Season => sim::calendar::SEASON_DAYS as usize, Span::Year => sim::calendar::YEAR_DAYS as usize, Span::All => days.len() };
    let take = span.min(days.len());
    let (mut i, each, mut out) = (days.len() - take, take.div_ceil(BARS).max(1), Vec::new());
    while i < days.len() {
        let end = (i + each).min(days.len());
        let n = (end - i) as f32;
        let mut bar = Bar { day0: days[i].day, day1: days[end - 1].day, ..default() };
        for d in days.range(i..end) {
            let (a, b, down) = f(d);
            (bar.a, bar.b, bar.down) = (bar.a + a / n, bar.b + b / n, bar.down + down / n);
        }
        out.push(bar);
        i = end;
    }
    out
}

fn day_range(b: &Bar) -> String {
    if b.day0 == b.day1 { date(b.day0 as f64) } else { format!("{} – {}", date(b.day0 as f64), date(b.day1 as f64)) }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn chart(u: &mut Ui, icon: Icon, col: Color, col_b: Color, name: &str, value: String, value_col: Color, data: &[Bar], mirrored: bool, tip_of: impl Fn(&Bar) -> Tip) {
    u.col(4.0, |card| {
        card.row(6.0, |r| { r.icon(icon, 16.0, col); r.spacer(); r.bold(value, 13.0, value_col); }).tip(Tip::name(name));
        let top = data.iter().map(|b| b.a + b.b).fold(0.0f32, f32::max).max(1e-3);
        let bottom = data.iter().map(|b| b.down).fold(0.0f32, f32::max).max(1e-3);
        let (up_h, down_h) = if mirrored { (CHART_H / 2.0, CHART_H / 2.0) } else { (CHART_H, 0.0) };
        let n = data.len().max(1) as f32;
        let gap = if n > 40.0 { 1.0 } else { 2.0 };
        let w = ((CHART_W - gap * (n - 1.0)) / n).max(1.0);
        card.node(Node { width: Val::Px(CHART_W), height: Val::Px(CHART_H), align_items: AlignItems::FlexStart, ..row_node(gap) }, |plot| {
            if data.is_empty() { plot.add(VNode::new(Node { width: Val::Px(CHART_W), height: Val::Px(1.0), margin: UiRect::top(Val::Px(up_h)), ..default() })).bg(LINE); }
            for b in data {
                let (ha, hb, hd) = ((b.a / top * up_h).max((b.a > 0.0) as u8 as f32), (b.b / top * up_h).max((b.b > 0.0) as u8 as f32), (b.down / bottom * down_h).max((b.down > 0.0) as u8 as f32));
                plot.node(Node { width: Val::Px(w), height: Val::Px(CHART_H), flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, |bar| {
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px((up_h - ha - hb).max(0.0)), ..default() }));
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(hb), ..default() })).bg(col_b);
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(ha), ..default() })).bg(col.with_alpha(0.85));
                    if mirrored {
                        bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(1.0), ..default() })).bg(LINE);
                        bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(hd), ..default() })).bg(BAD.with_alpha(0.75));
                    }
                }).tip(tip_of(b));
            }
        }).bg(Color::srgba(0.0, 0.0, 0.0, 0.18));
    });
}

fn vol(m3: f32) -> String {
    let m3 = m3.max(0.0);
    if m3 >= 10_000.0 { format!("{:.0}k", m3 / 1000.0) } else if m3 >= 1000.0 { format!("{:.1}k", m3 / 1000.0) } else { format!("{m3:.0}") }
}

fn wrap_grid(u: &mut Ui, col_gap: f32, row_gap: f32, f: impl FnOnce(&mut Ui)) {
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(col_gap), row_gap: Val::Px(row_gap), ..default() }, f);
}

fn charts(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let Some(last) = s.ledger.colony.back() else {
        u.row(6.0, |r| { r.icon(Icon::Hourglass, 16.0, MUTED); }).tip(Tip::name("The first day is not over yet"));
        return;
    };
    wrap_grid(u, 16.0, 12.0, |g| {
        for g_ in Good::all().filter(|&g| super::panels::staple(g)) {
            let net = s.ledger.in_per_day(g_) - s.ledger.out_per_day(g_);
            chart(g, good_icon(g_), good_color(g_), good_color(g_), g_.name(), format!("{}/d", signed(net)), if net < -0.05 { BAD } else if net > 0.05 { GOOD } else { INK }, &bars(c, |d| (d.produced[g_], 0.0, d.consumed[g_])), true, |b| {
                Tip::titled(Some((good_icon(g_), good_color(g_))), day_range(b)).line("Made per day", num(b.a)).line("Used per day", num(b.down))
            });
        }
        let cell_area = s.water.cell_m * s.water.cell_m;
        let now: f32 = s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum();
        chart(g, Icon::Drop, WATER, WATER, "Water held behind dams (m³)", format!("{} m³", vol(now)), INK, &bars(c, |d| (d.census.water, 0.0, 0.0)), false, |b| {
            Tip::titled(Some((Icon::Drop, WATER)), day_range(b)).line("Held", format!("{} m³", vol(b.a)))
        });
        let planted_by = |day: u64| s.ledger.colony.iter().find(|d| d.day == day).map_or(0, |d| d.census.planted);
        let tree_data: Vec<Bar> = bars(c, |d| (d.census.trees as f32, 0.0, 0.0)).into_iter().map(|mut b| {
            b.b = planted_by(b.day1).saturating_sub(if b.day0 == 0 { 0 } else { planted_by(b.day0 - 1) }) as f32;
            b
        }).collect();
        let (ti, total_planted) = (resource_icon(sim::resources::ResourceKind::tree()), s.ledger.planted.iter().sum::<u32>());
        chart(g, ti, GOOD, Color::srgb(0.75, 0.92, 0.55), "Trees standing, and planted", format!("{}  +{total_planted}", last.census.trees), INK, &tree_data, false, |b| {
            Tip::titled(Some((ti, GOOD)), day_range(b)).line("Standing", num(b.a)).line("Planted", format!("{}", b.b as i64)).line("Planted in all", format!("{total_planted}"))
        });
    });
    u.rule();
    wrap_grid(u, 16.0, 12.0, |g| {
        for g_ in Good::all() {
            let stores = &s.buildings.stores;
            chart(g, good_icon(g_), good_color(g_), good_color(g_), &format!("{} in stores", g_.name()), format!("{}/{}", vol(stores.amount(g_)), vol(stores.capacity(g_))), INK, &bars(c, |d| (d.stock[g_], 0.0, 0.0)), false, |b| {
                Tip::titled(Some((good_icon(g_), good_color(g_))), day_range(b)).line("Stored", num(b.a))
            });
        }
    });
}

fn tile(u: &mut Ui, icon: Icon, col: Color, value: String, name: &str) {
    u.col(3.0, |t| { t.icon(icon, 20.0, col); t.bold(value, 14.0, INK); })
        .style(|n| { n.width = Val::Px(62.0); n.align_items = AlignItems::Center; })
        .tip(Tip::titled(Some((icon, col)), name));
}

pub(super) fn run(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    if !s.ledger.colony.is_empty() {
        wrap_grid(u, 16.0, 12.0, |g| {
            chart(g, Icon::Beaver, FUR, Color::srgb(0.9, 0.72, 0.5), "Beavers", format!("{}", s.colony.population()), INK, &bars(c, |d| ((d.census.beavers - d.census.young) as f32, d.census.young as f32, 0.0)), false, |b| {
                Tip::titled(Some((Icon::Beaver, FUR)), day_range(b)).line("Adults", num(b.a)).line("Young", num(b.b))
            });
        });
        u.rule();
    }
    let (st, run) = (&s.colony.stats, &s.run);
    wrap_grid(u, 8.0, 10.0, |r| {
        for (i, col, v, name) in [(Icon::Hourglass, MUTED, span(s.time_days), "Time"), (Icon::Kit, FUR, format!("{}", st.births), "Born in the colony"), (Icon::Walk, FUR, format!("{}", st.immigrants), "Came to join"), (Icon::Beaver, FUR, format!("{}", st.peak_population), "Most beavers at once"), (Icon::MoodBad, BAD, format!("{}", st.deaths), "Died"), (Icon::Back, MUTED, format!("{}", st.departures), "Left")] {
            tile(r, i, col, v, name);
        }
    });
    u.rule();
    wrap_grid(u, 8.0, 10.0, |r| {
        for g in Good::all() { tile(r, good_icon(g), good_color(g), vol(s.buildings.stores.produced_total[g]), &format!("{} gathered", g.name())); }
        tile(r, good_icon(Good::seedling()), GOOD, format!("{}", s.ledger.planted.iter().sum::<u32>()), "Seedlings planted");
    });
    u.rule();
    wrap_grid(u, 8.0, 10.0, |r| {
        tile(r, Icon::Raft, WATER, format!("{}", run.raft_trips), "Raft loads delivered");
        for g in Good::all().filter(|&g| run.raftd[g] > 0.0) { tile(r, good_icon(g), good_color(g), vol(run.raftd[g]), &format!("{} moved by raft", g.name())); }
    });
    u.rule();
    wrap_grid(u, 8.0, 10.0, |r| {
        for k in BuildingKind::all() { tile(r, building_icon(k), INK, format!("{}", run.built_of(k)), &format!("{} built", k.spec().name.as_str())); }
        tile(r, Icon::Lock, INK, format!("{}", run.locks), "Locks built");
        tile(r, Icon::Drop, WATER, vol(run.peak_water), "Most water held behind dams (m³)");
    });
}

pub fn scroll_max(s: &sim::SimState, tab: LedgerTab) -> usize {
    match tab {
        LedgerTab::Dams => s.buildings.buildings.iter().filter(|b| b.kind.is_dam() && b.dam.is_some()).count().saturating_sub(4),
        LedgerTab::Canals => s.buildings.buildings.iter().filter(|b| b.canal.is_some()).count().saturating_sub(4),
        LedgerTab::Buildings => s.buildings.buildings.iter().filter(|b| !b.kind.is_waterworks()).count().saturating_sub(ROWS),
        LedgerTab::Upkeep => super::costs::scroll_max(s),
        LedgerTab::Charts | LedgerTab::Stores => 0,
    }
}

pub fn beavers_scroll_max(s: &sim::SimState) -> usize { s.colony.beavers.len().saturating_sub(ROWS) }

pub(super) fn span_chips(c: &Ctx, r: &mut Ui) {
    for (sp, label, name) in [(Span::Season, "90d", "Last season"), (Span::Year, "1y", "Last year"), (Span::All, "All", "Everything")] {
        chip(r, label, c.hud.span == sp, HudAction::Span(sp)).tip(Tip::name(name));
    }
    r.add(VNode::new(Node { width: Val::Px(8.0), ..default() }));
}

fn factor_col(x: f32) -> Color { if x < 0.8 { BAD } else if x < 0.95 { WARN } else if x > 1.05 { GOOD } else { INK } }
fn times(x: f32) -> String { format!("×{x:.2}") }

fn paced_job(s: &sim::SimState, b: &Beaver) -> Option<sim::colony::Job> {
    sim::jobs::role_of(s, b.id).filter(|j| !matches!(j.duty(), sim::content::Duty::Haul | sim::content::Duty::Watch)).filter(|_| b.can_work())
}

pub(super) fn pace(s: &sim::SimState, b: &Beaver) -> Option<(f32, Tip)> {
    let wf = sim::agents::work_factors(b, paced_job(s, b)?);
    let mut tip = Tip::name("Pace").with_value(times(wf.total()));
    for (name, f) in [("Skill", wf.skill_x), ("Mood", wf.mood_x), ("Health", wf.health_x), ("Rest", wf.rest_x), ("Age", wf.age_x)] {
        if (f - 1.0).abs() > 0.005 { tip = tip.line(name, times(f)); }
    }
    Some((wf.total(), tip))
}

const COLS: [(Option<BeaverCol>, Icon, &str, f32); 12] = [
    (None, Icon::Beaver, "", 22.0), (Some(BeaverCol::Name), Icon::Beaver, "Name", 140.0), (Some(BeaverCol::Age), Icon::Hourglass, "Age (years)", 34.0),
    (Some(BeaverCol::Family), Icon::Home, "Family", 120.0), (Some(BeaverCol::Job), Icon::Jobs, "Job", 26.0), (Some(BeaverCol::Doing), Icon::Walk, "Doing now", 26.0),
    (Some(BeaverCol::Skill), Icon::Hammer, "Skill at this job", 64.0), (None, Icon::MoodGood, "Mood", 42.0), (None, Icon::Health, "Health", 42.0),
    (None, Icon::Idle, "Rest", 42.0), (None, Icon::Kit, "Age: the young work at a share", 42.0), (Some(BeaverCol::Pace), Icon::Fast2, "Pace: every factor together", 46.0),
];

pub(super) fn beavers(c: &Ctx, u: &mut Ui) {
    let (s, (col, desc)) = (c.s, c.hud.sort);
    let family = |b: &Beaver| s.colony.family(b.family).map(|f| f.name.clone()).unwrap_or_default();
    let role_word = |b: &Beaver| sim::jobs::role_of(s, b.id).map_or("Free", |j| j.name());
    let key_f = |b: &Beaver| match col {
        BeaverCol::Age => b.age_days,
        BeaverCol::Skill => paced_job(s, b).map_or(-1.0, |j| b.skills[j]),
        BeaverCol::Pace => paced_job(s, b).map_or(-1.0, |j| sim::agents::work_factors(b, j).total()),
        _ => 0.0,
    };
    let mut list: Vec<&Beaver> = s.colony.beavers.iter().collect();
    list.sort_by(|a, b| {
        let o = match col {
            BeaverCol::Name => a.name.cmp(&b.name),
            BeaverCol::Family => family(a).cmp(&family(b)).then(b.age_days.total_cmp(&a.age_days)),
            BeaverCol::Job => role_word(a).cmp(role_word(b)),
            BeaverCol::Doing => super::inspect::activity(s, a).2.cmp(super::inspect::activity(s, b).2),
            _ => key_f(a).total_cmp(&key_f(b)),
        };
        (if desc { o.reverse() } else { o }).then(a.id.0.cmp(&b.id.0))
    });
    let (n, first) = (list.len(), c.hud.scroll.min(list.len().saturating_sub(ROWS)));
    u.row(4.0, |r| {
        for (sort, icon, name, w) in COLS {
            let v = r.node(Node { width: Val::Px(w), flex_shrink: 0.0, ..row_node(2.0) }, |h| {
                if !name.is_empty() { h.icon(icon, 14.0, if sort == Some(col) { INK } else { MUTED }); }
                if sort == Some(col) { h.icon(if desc { Icon::Down } else { Icon::Up }, 9.0, ACCENT); }
            });
            if !name.is_empty() { v.tip(Tip::name(name)); }
            if let Some(k) = sort { v.action(HudAction::Sort(k)); }
        }
    });
    u.rule();
    u.paged_list(first, ROWS, n, ROWS as i32, ROWS as f32 * 24.0, 2.0, Some((Some(Icon::Beaver), true)), |rows| {
        for b in list.iter().skip(first).take(ROWS) { beaver_row(c, rows, b, &family(b)); }
    });
}

fn beaver_row(c: &Ctx, u: &mut Ui, b: &Beaver, family: &str) {
    let s = c.s;
    let cell = |i: usize| Node { width: Val::Px(COLS[i].3), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(3.0) };
    let (role, (icon, icon_col, word), paced, wf) = (sim::jobs::role_of(s, b.id), super::inspect::activity(s, b), pace(s, b), paced_job(s, b).map(|j| sim::agents::work_factors(b, j)));
    let row = u.row(4.0, |r| {
        r.node(Node { justify_content: JustifyContent::Center, ..cell(0) }, |t| { t.token(Icon::Beaver, token_px(b.stage) * 0.75, Some(doing(s, b))); });
        r.node(cell(1), |x| { x.bold(full_name(s, b.id, &b.name), 12.0, INK); });
        r.node(cell(2), |x| { x.text(format!("{:.1}", b.age_years()), 12.0, INK); });
        r.node(cell(3), |x| { x.text(family, 12.0, MUTED); });
        r.node(cell(4), |x| { x.icon(role.map_or(Icon::Idle, job_icon), 15.0, if role.is_some() { INK } else { ACCENT }); }).tip(Tip::name("Job").line(role.map_or("Free", |j| j.name()), ""));
        r.node(cell(5), |x| { x.icon(icon, 15.0, icon_col); }).tip(Tip::name("Doing now").line(word, ""));
        let mut skills = Tip::name("Skills");
        for j in sim::colony::Job::all() { skills.push(TipLine::new(j.row().skill.as_str(), format!("{:.0}%", b.skills[j] * 100.0)).color(if Some(j) == role { ACCENT } else { INK })); }
        r.node(cell(6), |x| match wf {
            Some(wf) => { x.ticks(wf.skill, 5, 34.0, 8.0, ACCENT); x.text(format!("{:.0}", wf.skill * 100.0), 11.0, INK); }
            None => { x.text("-", 12.0, MUTED); }
        }).tip(skills);
        let factors = wf.map(|wf| [wf.mood_x, wf.health_x, wf.rest_x, wf.age_x]);
        for (k, name) in ["Mood", "Health", "Rest", "Age"].into_iter().enumerate() {
            let f = factors.map(|f| f[k]);
            let v = r.node(cell(7 + k), |x| match f { Some(f) => { x.text(times(f), 11.0, factor_col(f)); } None => { x.text("-", 12.0, MUTED); } });
            if let Some(f) = f { v.tip(Tip::name(name).with_value(times(f))); }
        }
        let v = r.node(cell(11), |x| match &paced { Some((p, _)) => { x.bold(times(*p), 12.0, factor_col(*p)); } None => { x.text("-", 12.0, MUTED); } });
        if let Some((_, tip)) = paced { v.tip(tip); }
    });
    row.node.height = Val::Px(22.0);
    row.peek(b.id).action(HudAction::SelectBeaver(b.id));
}

fn stat(r: &mut Ui, icon: Icon, col: Color, value: String, value_col: Color, tip: Tip) {
    r.row(4.0, |x| { x.icon(icon, 15.0, col); x.bold(value, 12.0, value_col); }).style(|n| n.min_width = Val::Px(76.0)).tip(tip);
}

fn empty_badge(u: &mut Ui, icon: Icon, tip: Option<Tip>) {
    let r = u.row(6.0, |r| { r.icon(icon, 20.0, MUTED); r.bold("0", 14.0, MUTED); });
    if let Some(t) = tip { r.tip(t); }
}

fn dams(c: &Ctx, u: &mut Ui) {
    let (s, cell_area) = (c.s, c.s.water.cell_m * c.s.water.cell_m);
    let list: Vec<_> = s.buildings.buildings.iter().filter(|b| b.kind.is_dam() && b.dam.is_some()).collect();
    if list.is_empty() { empty_badge(u, building_icon(BuildingKind::dam()), Some(Tip::name("No dams yet").key("Z"))); return; }
    let total: f32 = list.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum();
    u.row(10.0, |r| { r.icon(Icon::Drop, 16.0, WATER); r.bold(format!("{} m³", vol(total)), 14.0, INK); }).tip(Tip::name("Water held behind every dam"));
    let first = c.hud.scroll.min(list.len().saturating_sub(4));
    u.paged_list(first, 4, list.len(), 1, 320.0, 8.0, Some((None, false)), |cards| {
        for (i, b) in list.iter().enumerate().skip(first).take(4) {
            let d = b.dam.as_deref().expect("dam");
            let (health, built) = (sim::dams::health(s, d), d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0).collect::<Vec<_>>());
            let weak = built.iter().filter(|c| sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count();
            let lowest = built.iter().map(|c| sim::dams::integrity(s, c.cell)).fold(1.0f32, f32::min);
            let height = if built.is_empty() { 0.0 } else { built.iter().map(|c| sim::dams::built(s, c.cell)).sum::<f32>() / built.len() as f32 };
            let target = if d.cells.is_empty() { 0.0 } else { d.cells.iter().map(|c| c.target).sum::<f32>() / d.cells.len() as f32 };
            cards.col(6.0, |k| {
                k.row(10.0, |r| {
                    r.icon(building_icon(BuildingKind::dam()), 18.0, MUD);
                    r.bold(sim::names::pond_name(s, b.id).map_or_else(|| format!("Dam {}", i + 1), |n| n.to_string()), 14.0, INK);
                    r.pips(health, super::works::condition_colour(health)).tip(Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Weakest cell", format!("{:.0}%", lowest * 100.0)).line("Cells to repair", format!("{weak}")));
                    r.spacer();
                    let left = sim::dams::loads_left(s, b.id);
                    if left > 0 { stat(r, good_icon(sim::dams::material()), good_color(sim::dams::material()), format!("{left}"), INK, Tip::name("Loads still to carry")); }
                    r.pebble(Icon::Hammer, 11.0, HudAction::DamUpkeep(b.id, !d.upkeep), if d.upkeep { Peb::Good } else { Peb::Idle }).tip(Tip::name(if d.upkeep { "Kept in repair" } else { "Not kept in repair" }).line(if d.upkeep { "Stop" } else { "Keep in repair" }, ""));
                    super::costs::page_button(c, r, b.id);
                    r.pebble(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), Peb::Idle).tip(Tip::name("Open"));
                });
                super::costs::upkeep_over(c, k, b.id, c.hud.span);
                k.row(8.0, |r| {
                    match d.pond.as_ref() {
                        Some(p) => {
                            stat(r, Icon::Drop, WATER, vol(p.volume * cell_area), INK, Tip::name("Water held (m³)").line("Pond cells", format!("{}", p.area)));
                            stat(r, Icon::Rise, WATER, sim::dams::drop(s, d).map_or("-".into(), |r| format!("+{r:.1} m")), INK, Tip::name("Level rise").line("Above the water below the dam", ""));
                            let fill = ((p.level - p.bottom) / (p.full_level - p.bottom).max(0.01)).clamp(0.0, 1.0);
                            r.row(4.0, |x| { x.icon(Icon::Gauge, 15.0, WATER); x.ticks(fill, 6, 48.0, 10.0, WATER); })
                                .tip(Tip::name(sim::names::pond_name(s, b.id).unwrap_or("Pond")).with_value(format!("{:.0}%", fill * 100.0)).line("Deepest", format!("{:.1} m", p.max_depth)).line("Full in", if p.days_to_full <= 0.0 { "Full".into() } else if p.days_to_full.is_finite() { days(p.days_to_full) } else { "Never".into() }));
                            let week = d.history.len().min(7);
                            if week >= 2 {
                                let dl = d.history[d.history.len() - 1].level - d.history[d.history.len() - week].level;
                                let col = if dl < -0.02 { BAD } else if dl > 0.02 { GOOD } else { INK };
                                stat(r, if dl < 0.0 { Icon::Down } else { Icon::Up }, col, format!("{dl:+.2}"), col, Tip::name(format!("Level over {week} days (m)")));
                            }
                            let (icon, word) = match p.spill {
                                sim::dams::Spill::None => (Icon::Check, "Holding"),
                                sim::dams::Spill::Over(_) => (Icon::Crest, "Pouring over the crest"),
                                sim::dams::Spill::Around(_) => (Icon::Flood, "Running round an end"),
                                sim::dams::Spill::Gate(_) => (Icon::Lock, "Let out by a gate"),
                            };
                            r.icon(icon, 15.0, if matches!(p.spill, sim::dams::Spill::Around(_)) { WARN } else { MUTED }).tip(Tip::name(word));
                        }
                        None => { r.icon(Icon::Drop, 15.0, MUTED).tip(Tip::name("No pond yet")); }
                    }
                    r.spacer();
                    stat(r, Icon::Crest, MUD, format!("{height:.1}/{target:.1}"), INK, Tip::name("Wall height (m)").line("Built / planned", "").line("Cells", format!("{}", d.cells.len())).line("Gates", format!("{}", d.gates.len())));
                });
                if d.history.len() >= 2 {
                    let lo = d.history.iter().map(|x| x.level).fold(f32::INFINITY, f32::min);
                    let hi = d.history.iter().map(|x| x.level).fold(f32::NEG_INFINITY, f32::max);
                    let span = (hi - lo).max(0.05);
                    k.sparkline(d.history.iter().map(|x| (x.level - lo) / span), ((LEDGER_W - 28.0) / d.history.len() as f32 - 1.0).max(1.0), 18.0, WATER.with_alpha(0.6))
                        .tip(Tip::name("Pond level").line("Days", format!("{}", d.history.len())).line("Range", format!("{:.2} m", hi - lo)));
                }
            }).style(|n| n.padding = UiRect::all(Val::Px(8.0))).bg(Color::srgba(0.0, 0.0, 0.0, 0.16));
        }
    });
}

fn canals(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let list: Vec<_> = s.buildings.buildings.iter().filter(|b| b.canal.is_some()).collect();
    if list.is_empty() { empty_badge(u, building_icon(BuildingKind::canal()), Some(Tip::name("No canals yet"))); return; }
    let first = c.hud.scroll.min(list.len().saturating_sub(4));
    u.paged_list(first, 4, list.len(), 1, 320.0, 8.0, Some((None, false)), |cards| {
        for (i, b) in list.iter().enumerate().skip(first).take(4) {
            let k = b.canal.as_ref().expect("canal");
            let (health, st) = (sim::canals::health(s, k), k.status.as_ref());
            cards.col(6.0, |card| {
                card.row(10.0, |r| {
                    r.icon(building_icon(BuildingKind::canal()), 18.0, MUD);
                    r.bold(format!("Canal {}", i + 1), 14.0, INK);
                    r.pips(health, super::works::condition_colour(health)).tip(Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Silted cells", format!("{}", st.map_or(0, |x| x.silted))));
                    r.spacer();
                    r.pebble(Icon::Hammer, 11.0, HudAction::CanalUpkeep(b.id, !k.upkeep), if k.upkeep { Peb::Good } else { Peb::Idle }).tip(Tip::name(if k.upkeep { "Kept in repair" } else { "Not kept in repair" }).line(if k.upkeep { "Stop" } else { "Keep in repair" }, ""));
                    super::costs::page_button(c, r, b.id);
                    r.pebble(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), Peb::Idle).tip(Tip::name("Open"));
                });
                super::costs::upkeep_over(c, card, b.id, c.hud.span);
                if let Some(st) = st {
                    card.row(8.0, |r| {
                        stat(r, Icon::Raft, WATER, format!("{}/{}", st.floatable, st.cells), INK, Tip::name("Cells that float a raft"));
                        stat(r, Icon::Down, if st.silted > 0 { WARN } else { MUTED }, format!("{}", st.silted), INK, Tip::name("Silted cells"));
                        stat(r, Icon::Upstream, WATER, format!("{}/d", num(st.inflow)), INK, Tip::name("Water in (m³ a day)"));
                        stat(r, Icon::Push, WARN, format!("{}/d", num(st.outflow)), INK, Tip::name("Water out (m³ a day)"));
                    });
                }
            }).style(|n| n.padding = UiRect::all(Val::Px(8.0))).bg(Color::srgba(0.0, 0.0, 0.0, 0.16));
        }
    });
}

fn buildings(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let mut list: Vec<_> = s.buildings.buildings.iter().filter(|b| !b.kind.is_waterworks()).collect();
    list.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    if list.is_empty() { empty_badge(u, Icon::Building, None); return; }
    let (max, n, first) = (sim::buildings::MAX_BUILDING_DEPTH, list.len(), c.hud.scroll.min(list.len().saturating_sub(ROWS)));
    u.row(8.0, |r| {
        for (icon, name, w) in [(Icon::Building, "Building", 150.0), (Icon::Check, "State", 110.0), (Icon::Health, "Condition", 80.0), (Icon::Drop, "Water it stands in, of the most it can", 140.0), (Icon::Beaver, "Beavers", 60.0)] {
            r.node(Node { width: Val::Px(w), flex_shrink: 0.0, ..row_node(2.0) }, |h| { h.icon(icon, 14.0, MUTED); }).tip(Tip::name(name));
        }
    });
    u.rule();
    u.paged_list(first, ROWS, n, ROWS as i32, ROWS as f32 * 24.0, 2.0, Some((Some(Icon::Building), false)), |rows| {
        for b in list.iter().skip(first).take(ROWS) {
            let (depth, flooded) = (sim::buildings::flood_depth(b, &s.water.depth), sim::buildings::flooded(b, &s.water.depth));
            let dcol = if flooded { BAD } else if depth > 0.75 * max { WARN } else { WATER };
            let row = rows.row(8.0, |r| {
                r.node(Node { width: Val::Px(150.0), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(6.0) }, |x| { x.icon(building_icon(b.kind), 16.0, INK); x.text(b.kind.spec().name.as_str(), 12.0, INK); });
                r.node(Node { width: Val::Px(110.0), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(4.0) }, |x| {
                    if let Some((icon, word, goods, col)) = status_line(s, b.id) {
                        if let Some(i) = icon { x.icon(i, 13.0, col); }
                        x.text(word, 11.0, col);
                        for g in goods { x.icon(good_icon(g), 12.0, good_color(g)); }
                    }
                });
                r.node(Node { width: Val::Px(80.0), flex_shrink: 0.0, ..row_node(2.0) }, |x| { if b.is_complete() { x.pips(b.condition, level_color(b.condition)); } })
                    .tip(Tip::name("Condition").with_value(format!("{:.0}%", b.condition * 100.0)));
                r.node(Node { width: Val::Px(140.0), flex_shrink: 0.0, ..row_node(4.0) }, |x| { x.ticks(depth / max, 8, 80.0, 8.0, dcol); x.text(format!("{depth:.2} m"), 11.0, dcol); })
                    .tip(Tip::name(if flooded { "Flooded: out of use, wearing fast" } else { "Water" }).line("Here", format!("{depth:.2} m")).line("At most", format!("{max:.1} m")));
                r.node(Node { width: Val::Px(60.0), flex_shrink: 0.0, ..row_node(3.0) }, |x| {
                    if let Some(pl) = s.work.place(b.id).filter(|p| !p.slots.is_empty()) {
                        x.icon(Icon::Beaver, 13.0, MUTED);
                        let seats = if pl.open_ended() { String::new() } else { format!("/{}", pl.slots.len()) };
                        x.text(format!("{}{seats}", pl.filled()), 11.0, if pl.filled() == 0 { WARN } else { INK });
                    }
                });
            });
            row.node.height = Val::Px(22.0);
            row.action(HudAction::SelectBuilding(b.id));
        }
    });
}
