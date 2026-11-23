//! The colony ledger: the colony over time as charts, then pages of detail — every beaver in a
//! table, every dam's water and upkeep, every building's state. Numbers and icons; the words
//! are in tooltips.
use super::colony::{doing, token_px};
use super::panels::{status_line, Ctx};
use super::ui::*;
use super::words::*;
use super::{BeaverCol, HudAction, LedgerTab, Span};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingKind, Good};
use sim::colony::Beaver;

pub const LEDGER_W: f32 = 600.0;
/// Rows of a long list shown at once; the wheel scrolls the rest.
const ROWS: usize = 16;
/// The most bars a chart draws: longer spans are averaged into this many.
const BARS: usize = 60;

const TABS: [(LedgerTab, Icon, &str); 4] = [
    (LedgerTab::Charts, Icon::Ledger, "Charts"),
    (LedgerTab::Beavers, Icon::Beaver, "Beavers"),
    (LedgerTab::Dams, Icon::Dam, "Dams"),
    (LedgerTab::Buildings, Icon::Building, "Buildings"),
];

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let Some(tab) = c.hud.ledger else { return Vec::new() };
    let mut ui = Ui::default();
    ui.node(
        Node { position_type: PositionType::Absolute, left: Val::Px(76.0), width: Val::Px(LEDGER_W), top: Val::Px(84.0), max_height: Val::Percent(80.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), overflow: Overflow::clip(), ..default() },
        |u| {
            u.row(6.0, |r| {
                for (t, icon, name) in TABS {
                    r.pebble(icon, 16.0, HudAction::Ledger(Some(t)), if t == tab { Peb::On } else { Peb::Idle }).tip(Tip::name(name));
                }
                r.spacer();
                if tab == LedgerTab::Charts {
                    for (sp, label, name) in [(Span::Season, "90d", "Last season"), (Span::Year, "1y", "Last year"), (Span::All, "All", "Everything")] {
                        chip(r, label, c.hud.span == sp, HudAction::Span(sp)).tip(Tip::name(name));
                    }
                    r.add(VNode::new(Node { width: Val::Px(8.0), ..default() }));
                }
                r.pebble(Icon::Cross, 10.0, HudAction::Ledger(None), Peb::Bare).tip(Tip::name("Close").key("L"));
            });
            u.rule();
            match tab {
                LedgerTab::Charts => charts(c, u),
                LedgerTab::Beavers => beavers(c, u),
                LedgerTab::Dams => dams(c, u),
                LedgerTab::Buildings => buildings(c, u),
            }
        },
    );
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(14.0));
    vec![v]
}

/// A small button with a word on it.
fn chip<'a>(r: &'a mut Ui, label: &str, on: bool, a: HudAction) -> &'a mut VNode {
    let v = r.node(Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, |x| {
        x.bold(label.to_string(), 11.0, if on { INK } else { MUTED });
    });
    v.skin(if on { Skin::PebbleOn } else { Skin::Pebble });
    v.action(a)
}

// ---------------------------------------------------------------------------------------------
// Charts

/// One bar: `a` with `b` stacked on it above the line, `down` hanging below it.
#[derive(Clone, Copy, Default)]
struct Bar {
    a: f32,
    b: f32,
    down: f32,
    day0: u64,
    day1: u64,
}

/// The record's days in the span, averaged into at most `BARS` bars by `f`.
fn bars(c: &Ctx, f: impl Fn(&sim::ledger::ColonyDay) -> (f32, f32, f32)) -> Vec<Bar> {
    let days = &c.s.ledger.colony;
    let span = match c.hud.span {
        Span::Season => sim::calendar::SEASON_DAYS as usize,
        Span::Year => sim::calendar::YEAR_DAYS as usize,
        Span::All => days.len(),
    };
    let take = span.min(days.len());
    let from = days.len() - take;
    let each = take.div_ceil(BARS).max(1);
    let mut out = Vec::new();
    let mut i = from;
    while i < days.len() {
        let end = (i + each).min(days.len());
        let n = (end - i) as f32;
        let mut bar = Bar { day0: days[i].day, day1: days[end - 1].day, ..default() };
        for d in days.range(i..end) {
            let (a, b, down) = f(d);
            bar.a += a / n;
            bar.b += b / n;
            bar.down += down / n;
        }
        out.push(bar);
        i = end;
    }
    out
}

fn day_range(b: &Bar) -> String {
    let d0 = sim::calendar::Date::at(b.day0 as f64);
    if b.day0 == b.day1 {
        format!("Y{} {} {}", d0.year, d0.season.name(), d0.day)
    } else {
        let d1 = sim::calendar::Date::at(b.day1 as f64);
        format!("Y{} {} {} – {} {}", d0.year, d0.season.name(), d0.day, d1.season.name(), d1.day)
    }
}

const CHART_W: f32 = 272.0;
const CHART_H: f32 = 52.0;

/// A chart card: its icon and the value now, then the bars. `mirrored` hangs `down` below a
/// middle line; otherwise `b` stacks on `a`.
#[allow(clippy::too_many_arguments)]
fn chart(u: &mut Ui, icon: Icon, col: Color, col_b: Color, name: &str, value: String, value_col: Color, data: &[Bar], mirrored: bool, tip_of: impl Fn(&Bar) -> Tip) {
    u.col(4.0, |card| {
        card.row(6.0, |r| {
            r.icon(icon, 16.0, col);
            r.spacer();
            r.bold(value, 13.0, value_col);
        })
        .tip(Tip::name(name));
        let top = data.iter().map(|b| b.a + b.b).fold(0.0f32, f32::max).max(1e-3);
        let bottom = data.iter().map(|b| b.down).fold(0.0f32, f32::max).max(1e-3);
        let (up_h, down_h) = if mirrored { (CHART_H / 2.0, CHART_H / 2.0) } else { (CHART_H, 0.0) };
        let n = data.len().max(1) as f32;
        let gap = if n > 40.0 { 1.0 } else { 2.0 };
        let w = ((CHART_W - gap * (n - 1.0)) / n).max(1.0);
        card.node(Node { width: Val::Px(CHART_W), height: Val::Px(CHART_H), column_gap: Val::Px(gap), align_items: AlignItems::FlexStart, ..row_node(gap) }, |plot| {
            if data.is_empty() {
                plot.add(VNode::new(Node { width: Val::Px(CHART_W), height: Val::Px(1.0), margin: UiRect::top(Val::Px(up_h)), ..default() })).bg(LINE);
            }
            for b in data {
                let ha = (b.a / top * up_h).max(if b.a > 0.0 { 1.0 } else { 0.0 });
                let hb = (b.b / top * up_h).max(if b.b > 0.0 { 1.0 } else { 0.0 });
                let hd = (b.down / bottom * down_h).max(if b.down > 0.0 { 1.0 } else { 0.0 });
                plot.node(Node { width: Val::Px(w), height: Val::Px(CHART_H), flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, |bar| {
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px((up_h - ha - hb).max(0.0)), ..default() }));
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(hb), ..default() })).bg(col_b);
                    bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(ha), ..default() })).bg(col.with_alpha(0.85));
                    if mirrored {
                        bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(1.0), ..default() })).bg(LINE);
                        bar.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(hd), ..default() })).bg(BAD.with_alpha(0.75));
                    }
                })
                .tip(tip_of(b));
            }
        })
        .bg(Color::srgba(0.0, 0.0, 0.0, 0.18));
    });
}

fn vol(m3: f32) -> String {
    if m3 >= 10_000.0 { format!("{:.0}k", m3 / 1000.0) } else if m3 >= 1000.0 { format!("{:.1}k", m3 / 1000.0) } else { format!("{:.0}", m3) }
}

/// The colony over time, two charts a row: beavers, food, sticks, clay, water held, trees.
fn charts(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let last = s.ledger.colony.back();
    if last.is_none() {
        u.row(6.0, |r| {
            r.icon(Icon::Hourglass, 16.0, MUTED);
        })
        .tip(Tip::name("The first day is not over yet"));
        return;
    }
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(16.0), row_gap: Val::Px(12.0), ..default() }, |g| {
        // Beavers: adults, and the young stacked above them.
        let data = bars(c, |d| ((d.census.beavers - d.census.young) as f32, d.census.young as f32, 0.0));
        let now = s.colony.population();
        chart(g, Icon::Beaver, FUR, Color::srgb(0.9, 0.72, 0.5), "Beavers", format!("{now}"), INK, &data, false, |b| {
            Tip::titled(Some((Icon::Beaver, FUR)), day_range(b)).line("Adults", num(b.a)).line("Young", num(b.b))
        });
        for g_ in [Good::Food, Good::Sticks, Good::Clay] {
            let i = g_.index();
            let data = bars(c, |d| (d.produced[i], 0.0, d.consumed[i]));
            let (inn, out) = (s.ledger.in_per_day(g_), s.ledger.out_per_day(g_));
            let net = inn - out;
            let value = format!("{}/d", signed(net));
            chart(g, good_icon(g_), good_color(g_), good_color(g_), good_name(g_), value, if net < -0.05 { BAD } else if net > 0.05 { GOOD } else { INK }, &data, true, |b| {
                Tip::titled(Some((good_icon(g_), good_color(g_))), day_range(b)).line("Made per day", num(b.a)).line("Used per day", num(b.down))
            });
        }
        let data = bars(c, |d| (d.census.water, 0.0, 0.0));
        let cell_area = s.water.cell_m * s.water.cell_m;
        let now: f32 = s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum();
        chart(g, Icon::Drop, WATER, WATER, "Water held behind dams (m³)", format!("{} m³", vol(now)), INK, &data, false, |b| {
            Tip::titled(Some((Icon::Drop, WATER)), day_range(b)).line("Held", format!("{} m³", vol(b.a)))
        });
        // Trees standing, and the saplings planted in each bar stacked on top.
        let days = &s.ledger.colony;
        let planted_by = |day: u64| days.iter().find(|d| d.day == day).map_or(0, |d| d.census.planted);
        let data: Vec<Bar> = bars(c, |d| (d.census.trees as f32, 0.0, 0.0))
            .into_iter()
            .map(|mut b| {
                let before = if b.day0 == 0 { 0 } else { planted_by(b.day0 - 1) };
                b.b = planted_by(b.day1).saturating_sub(before) as f32;
                b
            })
            .collect();
        let trees = last.map_or(0, |d| d.census.trees);
        chart(g, Icon::Tree, GOOD, Color::srgb(0.75, 0.92, 0.55), "Trees standing, and planted", format!("{trees}  +{}", s.ledger.planted), INK, &data, false, |b| {
            Tip::titled(Some((Icon::Tree, GOOD)), day_range(b)).line("Standing", num(b.a)).line("Planted", format!("{}", b.b as i64)).line("Planted in all", format!("{}", s.ledger.planted))
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Beavers

/// The job a beaver's skill and pace are read for: the work it holds now.
fn job_of(s: &sim::SimState, b: &Beaver) -> Option<sim::colony::Job> {
    sim::jobs::role_of(s, b.id).and_then(sim::jobs::colony_job)
}

fn job_name(j: sim::colony::Job) -> &'static str {
    use sim::colony::Job::*;
    match j {
        Forager => "Foraging",
        Logger => "Logging",
        Builder => "Building",
        Digger => "Digging",
        Hauler => "Hauling",
    }
}

fn factor_col(x: f32) -> Color {
    if x < 0.8 { BAD } else if x < 0.95 { WARN } else if x > 1.05 { GOOD } else { INK }
}

fn times(x: f32) -> String {
    format!("×{:.2}", x)
}

const COLS: [(Option<BeaverCol>, Icon, &str, f32); 11] = [
    (None, Icon::Beaver, "", 22.0),
    (Some(BeaverCol::Name), Icon::Beaver, "Name", 92.0),
    (Some(BeaverCol::Age), Icon::Hourglass, "Age (years)", 34.0),
    (Some(BeaverCol::Family), Icon::Home, "Family", 78.0),
    (Some(BeaverCol::Job), Icon::Jobs, "Doing", 26.0),
    (Some(BeaverCol::Skill), Icon::Hammer, "Skill at this job", 64.0),
    (None, Icon::MoodGood, "Mood", 42.0),
    (None, Icon::Health, "Health", 42.0),
    (None, Icon::Idle, "Rest", 42.0),
    (None, Icon::Kit, "Age: the young work at a share", 42.0),
    (Some(BeaverCol::Pace), Icon::Fast2, "Pace: every factor together", 46.0),
];

/// Every beaver in a table: who, how old, which family, what it does, its skill at that and
/// what speeds or slows it. A column's icon sorts by it; the wheel scrolls.
fn beavers(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let (col, desc) = c.hud.sort;
    let family = |b: &Beaver| s.colony.family(b.family).map(|f| f.name.clone()).unwrap_or_default();
    let mut list: Vec<&Beaver> = s.colony.beavers.iter().collect();
    let key_f = |b: &Beaver| -> f32 {
        match col {
            BeaverCol::Age => b.age_days,
            BeaverCol::Skill => job_of(s, b).map_or(-1.0, |j| b.skills[j.index()]),
            BeaverCol::Pace => job_of(s, b).map_or(-1.0, |j| sim::agents::work_factors(b, j).total()),
            _ => 0.0,
        }
    };
    list.sort_by(|a, b| {
        let o = match col {
            BeaverCol::Name => a.name.cmp(&b.name),
            BeaverCol::Family => family(a).cmp(&family(b)).then(b.age_days.total_cmp(&a.age_days)),
            BeaverCol::Job => job_of(s, a).map(|j| j.index()).cmp(&job_of(s, b).map(|j| j.index())),
            _ => key_f(a).total_cmp(&key_f(b)),
        };
        let o = if desc { o.reverse() } else { o };
        o.then(a.id.0.cmp(&b.id.0))
    });
    let n = list.len();
    let first = c.hud.scroll.min(n.saturating_sub(ROWS));
    // The header: a column's icon, and the sort.
    u.row(4.0, |r| {
        for (sort, icon, name, w) in COLS {
            let v = r.node(Node { width: Val::Px(w), flex_shrink: 0.0, ..row_node(2.0) }, |h| {
                if !name.is_empty() {
                    h.icon(icon, 14.0, if sort == Some(col) { INK } else { MUTED });
                }
                if sort == Some(col) {
                    h.icon(if desc { Icon::Down } else { Icon::Up }, 9.0, ACCENT);
                }
            });
            if !name.is_empty() {
                v.tip(Tip::name(name));
            }
            if let Some(k) = sort {
                v.action(HudAction::Sort(k));
            }
        }
    });
    u.rule();
    u.node(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() }, |body| {
        body.col(2.0, |rows| {
            for b in list.iter().skip(first).take(ROWS) {
                beaver_row(c, rows, b, &family(b));
            }
        });
        // A scroll bar when the list runs past the panel.
        if n > ROWS {
            let h = ROWS as f32 * 24.0;
            let thumb = (ROWS as f32 / n as f32 * h).max(16.0);
            let at = first as f32 / (n - ROWS) as f32 * (h - thumb);
            body.node(Node { width: Val::Px(4.0), height: Val::Px(h), flex_direction: FlexDirection::Column, border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, |bar| {
                bar.add(VNode::new(Node { height: Val::Px(at), ..default() }));
                bar.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px(thumb), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() })).bg(MUTED.with_alpha(0.7));
            })
            .bg(FAINT);
        }
    })
    .tag(Tag::Scroll)
    .solid();
    u.rule();
    u.row(10.0, |r| {
        r.icon(Icon::Beaver, 14.0, MUTED);
        r.bold(format!("{n}"), 12.0, MUTED);
        r.spacer();
        if n > ROWS {
            r.pebble(Icon::Up, 9.0, HudAction::Scroll(-(ROWS as i32)), if first == 0 { Peb::Faded } else { Peb::Idle });
            r.pebble(Icon::Down, 9.0, HudAction::Scroll(ROWS as i32), if first + ROWS >= n { Peb::Faded } else { Peb::Idle });
        }
    });
}

fn beaver_row(c: &Ctx, u: &mut Ui, b: &Beaver, family: &str) {
    let s = c.s;
    let w = |i: usize| COLS[i].3;
    let cell = |w: f32| Node { width: Val::Px(w), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(3.0) };
    let job = job_of(s, b);
    let (icon, icon_col, word) = super::inspect::activity(s, b);
    let wf = sim::agents::work_factors(b, job.unwrap_or(sim::colony::Job::Hauler));
    let row = u.row(4.0, |r| {
        r.node(Node { justify_content: JustifyContent::Center, ..cell(w(0)) }, |t| {
            t.token(Icon::Beaver, token_px(b.stage) * 0.75, Some(doing(s, b)));
        });
        r.node(cell(w(1)), |x| {
            x.bold(b.name.clone(), 12.0, INK);
        });
        r.node(cell(w(2)), |x| {
            x.text(format!("{:.1}", b.age_years()), 12.0, INK);
        });
        r.node(cell(w(3)), |x| {
            x.text(family.to_string(), 12.0, MUTED);
        });
        r.node(cell(w(4)), |x| {
            x.icon(icon, 15.0, icon_col);
        })
        .tip(Tip::name(word));
        // Skill at the job held: notches and the share; every skill in the tooltip.
        let mut skills = Tip::name("Skills");
        for j in sim::colony::Job::ALL {
            skills.push(TipLine::new(job_name(j), format!("{:.0}%", b.skills[j.index()] * 100.0)).color(if Some(j) == job { ACCENT } else { INK }));
        }
        r.node(cell(w(5)), |x| match job {
            Some(_) => {
                x.notches(wf.skill, 5, 34.0, 8.0, ACCENT);
                x.text(format!("{:.0}", wf.skill * 100.0), 11.0, INK);
            }
            None => {
                x.text("-", 12.0, MUTED);
            }
        })
        .tip(skills);
        for (i, f, name) in [(6, wf.mood_x, "Mood"), (7, wf.health_x, "Health"), (8, wf.rest_x, "Rest"), (9, wf.age_x, "Age")] {
            // Kits too young to work have no pace to speak of.
            let v = r.node(cell(w(i)), |x| {
                if b.can_work() {
                    x.text(times(f), 11.0, factor_col(f));
                } else {
                    x.text("-", 12.0, MUTED);
                }
            });
            if b.can_work() {
                v.tip(Tip::name(name).with_value(times(f)));
            }
        }
        r.node(cell(w(10)), |x| match job {
            Some(_) => {
                x.bold(times(wf.total()), 12.0, factor_col(wf.total()));
            }
            None => {
                x.text("-", 12.0, MUTED);
            }
        })
        .tip(
            Tip::name("Pace")
                .with_value(if job.is_some() { times(wf.total()) } else { "-".into() })
                .line("Skill", times(wf.skill_x))
                .line("Mood", times(wf.mood_x))
                .line("Health", times(wf.health_x))
                .line("Rest", times(wf.rest_x))
                .line("Age", times(wf.age_x)),
        );
    });
    row.node.height = Val::Px(22.0);
    row.peek(b.id);
    row.action(HudAction::SelectBeaver(b.id));
}

// ---------------------------------------------------------------------------------------------
// Dams

/// The water level just downstream of a dam: the face its pond stands above.
fn tail_level(s: &sim::SimState, d: &sim::dams::Dam) -> Option<f32> {
    let n = s.height.width() as i32;
    let on: std::collections::HashSet<(usize, usize)> = d.cells.iter().map(|c| c.cell).collect();
    let levels: Vec<f32> = d
        .cells
        .iter()
        .filter_map(|c| {
            let (x, y) = (c.cell.0 as i32 + d.downstream.0, c.cell.1 as i32 + d.downstream.1);
            if x < 0 || y < 0 || x >= n || y >= n || on.contains(&(x as usize, y as usize)) {
                return None;
            }
            let (x, y) = (x as usize, y as usize);
            Some(s.height.get(x, y).copied().unwrap_or(0.0) + s.water.depth.get(x, y).copied().unwrap_or(0.0))
        })
        .collect();
    (!levels.is_empty()).then(|| levels.iter().copied().fold(f32::INFINITY, f32::min))
}

fn stat(r: &mut Ui, icon: Icon, col: Color, value: String, value_col: Color, tip: Tip) {
    r.row(4.0, |x| {
        x.icon(icon, 15.0, col);
        x.bold(value, 12.0, value_col);
    })
    .style(|n| n.min_width = Val::Px(76.0))
    .tip(tip);
}

/// Each dam: what it holds and how high it lifts the water, how full its pond is and whether
/// it is rising, and its upkeep.
fn dams(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let cell_area = s.water.cell_m * s.water.cell_m;
    let list: Vec<_> = s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dam && b.dam.is_some()).collect();
    if list.is_empty() {
        u.row(6.0, |r| {
            r.icon(Icon::Dam, 20.0, MUTED);
            r.bold("0", 14.0, MUTED);
        })
        .tip(Tip::name("No dams yet").key("Z"));
        return;
    }
    let total: f32 = list.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum();
    u.row(10.0, |r| {
        r.icon(Icon::Drop, 16.0, WATER);
        r.bold(format!("{} m³", vol(total)), 14.0, INK);
    })
    .tip(Tip::name("Water held behind every dam"));
    let first = c.hud.scroll.min(list.len().saturating_sub(4));
    u.col(8.0, |cards| {
        for (i, b) in list.iter().enumerate().skip(first).take(4) {
            let d = b.dam.as_deref().expect("dam");
            let health = sim::dams::health(s, d);
            let built: Vec<_> = d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0).collect();
            let weak = built.iter().filter(|c| sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count();
            let lowest = built.iter().map(|c| sim::dams::integrity(s, c.cell)).fold(1.0f32, f32::min);
            let height = if built.is_empty() { 0.0 } else { built.iter().map(|c| sim::dams::built(s, c.cell)).sum::<f32>() / built.len() as f32 };
            let target = if d.cells.is_empty() { 0.0 } else { d.cells.iter().map(|c| c.target).sum::<f32>() / d.cells.len() as f32 };
            cards.col(6.0, |k| {
                k.row(10.0, |r| {
                    r.icon(Icon::Dam, 18.0, MUD);
                    r.bold(format!("{}", i + 1), 14.0, INK);
                    r.pips(health, if health < 0.5 { BAD } else if health < sim::dams::REPAIR_BELOW { WARN } else { GOOD }).tip(Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Weakest cell", format!("{:.0}%", lowest * 100.0)).line("Cells to repair", format!("{weak}")));
                    r.spacer();
                    let left = sim::dams::loads_left(s, b.id);
                    if left > 0 {
                        stat(r, good_icon(Good::Sticks), good_color(Good::Sticks), format!("{left}"), INK, Tip::name("Loads still to carry"));
                    }
                    let word = if d.maintain { "Kept in repair" } else { "Not kept in repair" };
                    r.pebble(Icon::Hammer, 11.0, HudAction::DamMaintain(b.id, !d.maintain), if d.maintain { Peb::Good } else { Peb::Idle }).tip(Tip::name(word).line(if d.maintain { "Stop" } else { "Keep in repair" }, ""));
                    r.pebble(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), Peb::Idle).tip(Tip::name("Open"));
                });
                k.row(8.0, |r| {
                    match d.pond.as_ref() {
                        Some(p) => {
                            let held = p.volume * cell_area;
                            stat(r, Icon::Drop, WATER, vol(held), INK, Tip::name("Water held (m³)").line("Pond cells", format!("{}", p.area)));
                            let rise = tail_level(s, d).map(|t| (p.level - t).max(0.0));
                            stat(r, Icon::Rise, WATER, rise.map_or("-".into(), |r| format!("+{:.1} m", r)), INK, Tip::name("Level rise").line("Above the water below the dam", ""));
                            let fill = ((p.level - p.bottom) / (p.full_level - p.bottom).max(0.01)).clamp(0.0, 1.0);
                            r.row(4.0, |x| {
                                x.icon(Icon::Gauge, 15.0, WATER);
                                x.notches(fill, 6, 48.0, 10.0, WATER);
                            })
                            .tip(Tip::name("Pond").with_value(format!("{:.0}%", fill * 100.0)).line("Deepest", format!("{:.1} m", p.max_depth)).line("Full in", if p.days_to_full <= 0.0 { "Full".to_string() } else if p.days_to_full.is_finite() { days(p.days_to_full) } else { "Never".to_string() }));
                            // The level over the last week.
                            let h = &d.history;
                            let week = h.len().min(7);
                            if week >= 2 {
                                let dl = h[h.len() - 1].level - h[h.len() - week].level;
                                let col = if dl < -0.02 { BAD } else if dl > 0.02 { GOOD } else { INK };
                                stat(r, if dl < 0.0 { Icon::Down } else { Icon::Up }, col, format!("{}{:.2}", if dl >= 0.0 { "+" } else { "" }, dl), col, Tip::name(format!("Level over {week} days (m)")));
                            }
                            let (icon, word) = match p.spill {
                                sim::dams::Spill::None => (Icon::Check, "Holding"),
                                sim::dams::Spill::Over(_) => (Icon::Crest, "Pouring over the crest"),
                                sim::dams::Spill::Around(_) => (Icon::Flood, "Running round an end"),
                                sim::dams::Spill::Gate(_) => (Icon::Lock, "Let out by a gate"),
                            };
                            r.icon(icon, 15.0, if matches!(p.spill, sim::dams::Spill::Around(_)) { WARN } else { MUTED }).tip(Tip::name(word));
                        }
                        None => {
                            r.icon(Icon::Drop, 15.0, MUTED).tip(Tip::name("No pond yet"));
                        }
                    }
                    r.spacer();
                    stat(r, Icon::Crest, MUD, format!("{:.1}/{:.1}", height, target), INK, Tip::name("Wall height (m)").line("Built / planned", "").line("Cells", format!("{}", d.cells.len())).line("Gates", format!("{}", d.gates.len())));
                });
                // The pond level over the record, as a strip.
                let h = &d.history;
                if h.len() >= 2 {
                    let lo = h.iter().map(|x| x.level).fold(f32::INFINITY, f32::min);
                    let hi = h.iter().map(|x| x.level).fold(f32::NEG_INFINITY, f32::max);
                    let span = (hi - lo).max(0.05);
                    let w = ((LEDGER_W - 28.0) / h.len() as f32 - 1.0).max(1.0);
                    k.node(Node { height: Val::Px(18.0), align_items: AlignItems::FlexEnd, ..row_node(1.0) }, |sp| {
                        for x in h.iter() {
                            sp.add(VNode::new(Node { width: Val::Px(w), height: Val::Px(((x.level - lo) / span * 16.0 + 2.0).max(1.0)), ..default() })).bg(WATER.with_alpha(0.6));
                        }
                    })
                    .tip(Tip::name("Pond level").line("Days", format!("{}", h.len())).line("Range", format!("{:.2} m", hi - lo)));
                }
            })
            .style(|n| n.padding = UiRect::all(Val::Px(8.0)))
            .bg(Color::srgba(0.0, 0.0, 0.0, 0.16));
        }
    })
    .tag(Tag::Scroll)
    .solid();
    if list.len() > 4 {
        u.row(8.0, |r| {
            r.spacer();
            r.pebble(Icon::Up, 9.0, HudAction::Scroll(-1), if first == 0 { Peb::Faded } else { Peb::Idle });
            r.pebble(Icon::Down, 9.0, HudAction::Scroll(1), if first + 4 >= list.len() { Peb::Faded } else { Peb::Idle });
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Buildings

/// Every building: its state, condition, the water it stands in against what it can, and its
/// workers.
fn buildings(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let mut list: Vec<_> = s.buildings.buildings.iter().filter(|b| !b.kind.is_waterworks()).collect();
    list.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    if list.is_empty() {
        u.row(6.0, |r| {
            r.icon(Icon::Building, 20.0, MUTED);
            r.bold("0", 14.0, MUTED);
        });
        return;
    }
    let max = sim::buildings::MAX_BUILDING_DEPTH;
    let n = list.len();
    let first = c.hud.scroll.min(n.saturating_sub(ROWS));
    u.row(8.0, |r| {
        for (icon, name, w) in [(Icon::Building, "Building", 150.0), (Icon::Check, "State", 110.0), (Icon::Health, "Condition", 80.0), (Icon::Drop, "Water it stands in, of the most it can", 140.0), (Icon::Beaver, "Workers", 60.0)] {
            r.node(Node { width: Val::Px(w), flex_shrink: 0.0, ..row_node(2.0) }, |h| {
                h.icon(icon, 14.0, MUTED);
            })
            .tip(Tip::name(name));
        }
    });
    u.rule();
    u.col(2.0, |rows| {
        for b in list.iter().skip(first).take(ROWS) {
            let depth = sim::buildings::flood_depth(b, &s.water.depth);
            let flooded = sim::buildings::flooded(b, &s.water.depth);
            let dcol = if flooded { BAD } else if depth > 0.75 * max { WARN } else { WATER };
            let row = rows.row(8.0, |r| {
                r.node(Node { width: Val::Px(150.0), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(6.0) }, |x| {
                    x.icon(building_icon(b.kind), 16.0, INK);
                    x.text(b.kind.spec().name, 12.0, INK);
                });
                r.node(Node { width: Val::Px(110.0), flex_shrink: 0.0, overflow: Overflow::clip(), ..row_node(4.0) }, |x| {
                    if let Some((icon, word, goods, col)) = status_line(s, b.id) {
                        if let Some(i) = icon {
                            x.icon(i, 13.0, col);
                        }
                        x.text(word, 11.0, col);
                        for g in goods {
                            x.icon(good_icon(g), 12.0, good_color(g));
                        }
                    }
                });
                r.node(Node { width: Val::Px(80.0), flex_shrink: 0.0, ..row_node(2.0) }, |x| {
                    if b.is_complete() {
                        x.pips(b.condition, if b.condition < 0.35 { BAD } else if b.condition < 0.6 { WARN } else { GOOD });
                    }
                })
                .tip(Tip::name("Condition").with_value(format!("{:.0}%", b.condition * 100.0)));
                r.node(Node { width: Val::Px(140.0), flex_shrink: 0.0, ..row_node(4.0) }, |x| {
                    x.notches(depth / max, 8, 80.0, 8.0, dcol);
                    x.text(format!("{:.2} m", depth), 11.0, dcol);
                })
                .tip(Tip::name(if flooded { "Flooded: out of use, wearing fast" } else { "Water" }).line("Here", format!("{:.2} m", depth)).line("At most", format!("{:.1} m", max)));
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
    })
    .tag(Tag::Scroll)
    .solid();
    if n > ROWS {
        u.row(8.0, |r| {
            r.icon(Icon::Building, 14.0, MUTED);
            r.bold(format!("{n}"), 12.0, MUTED);
            r.spacer();
            r.pebble(Icon::Up, 9.0, HudAction::Scroll(-(ROWS as i32)), if first == 0 { Peb::Faded } else { Peb::Idle });
            r.pebble(Icon::Down, 9.0, HudAction::Scroll(ROWS as i32), if first + ROWS >= n { Peb::Faded } else { Peb::Idle });
        });
    }
}
