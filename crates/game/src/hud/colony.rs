//! The Colony screen (spec §4.4, Tab): docked tables that leave a quarter of the map live.
use super::inspect::mood_icon;
use super::panels::{food_days, gap_line, stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingKind, Good};
use sim::work::Role;

const TABS: [(ColonyTab, Icon, &str); 7] = [
    (ColonyTab::Population, Icon::Beaver, "Population"),
    (ColonyTab::Jobs, Icon::Jobs, "Jobs"),
    (ColonyTab::Goods, Icon::Colony, "Goods"),
    (ColonyTab::Water, Icon::Drop, "Water"),
    (ColonyTab::Land, Icon::Tree, "Land"),
    (ColonyTab::Research, Icon::Journal, "Research"),
    (ColonyTab::Log, Icon::Log, "Log"),
];

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let Some(tab) = c.hud.colony else { return Vec::new() };
    let mut ui = Ui::default();
    ui.node(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Percent(25.0),
            top: Val::Px(58.0),
            bottom: Val::Px(80.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(12.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        |u| {
            u.row(6.0, |r| {
                for (t, icon, name) in TABS {
                    let on = t == tab;
                    r.icon_button(icon, 18.0, HudAction::Colony(Some(t)), if on { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }, on).tip(Tip::name(name).key(if t == ColonyTab::Jobs { "J" } else { "" }));
                }
                r.spacer();
                r.bold(TABS.iter().find(|x| x.0 == tab).map(|x| x.2).unwrap_or(""), 14.0, INK);
                r.icon_button(Icon::Cross, 11.0, HudAction::Colony(None), FAINT, false).tip(Tip::name("Close").key("Tab"));
            });
            u.rule();
            match tab {
                ColonyTab::Population => population(c, u),
                ColonyTab::Jobs => jobs(c, u),
                ColonyTab::Goods => goods(c, u),
                ColonyTab::Water => water(c, u),
                ColonyTab::Land => land(c, u),
                ColonyTab::Research => research(c, u),
                ColonyTab::Log => log(c, u),
            }
        },
    );
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    vec![v]
}

/// A header cell: an icon (with a tooltip) in a fixed width, clickable to sort.
fn head(u: &mut Ui, icon: Option<Icon>, name: &str, w: f32, sort: Option<u8>, active: bool) {
    let v = u.node(Node { width: Val::Px(w), justify_content: JustifyContent::FlexEnd, column_gap: Val::Px(2.0), ..row_node(2.0) }, |x| {
        match icon {
            Some(i) => {
                x.icon(i, 14.0, if active { INK } else { MUTED });
            }
            None => {
                x.bold(name, 11.0, if active { INK } else { MUTED });
            }
        }
        if active {
            x.icon(Icon::Down, 7.0, INK);
        }
    });
    v.tip(Tip::name(name));
    if let Some(k) = sort {
        v.action(HudAction::SortPop(k));
    }
}

fn cell_num(u: &mut Ui, s: impl Into<String>, w: f32, col: Color) {
    u.num(s, 12.0, col, w);
}

// ───────────────────────────── population ─────────────────────────────

fn population(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let mut rows: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().collect();
    let key = c.hud.pop_sort;
    rows.sort_by(|a, b| {
        let v = |x: &sim::colony::Beaver| -> f32 {
            match key {
                1 => x.age_days,
                2 => s.work.slot_of(x.id).map(|(_, _, r)| role_rank(r) as f32).unwrap_or(99.0),
                3 => x.mood,
                4 => x.health,
                _ => 0.0,
            }
        };
        if key == 0 { a.name.cmp(&b.name) } else { v(b).total_cmp(&v(a)) }
    });
    u.row(8.0, |r| {
        head(r, None, "Name", 130.0, Some(0), key == 0);
        head(r, Some(Icon::Beaver), "Age", 56.0, Some(1), key == 1);
        head(r, Some(Icon::Jobs), "Job", 64.0, Some(2), key == 2);
        head(r, Some(Icon::Home), "Home", 44.0, None, false);
        head(r, Some(Icon::MoodGood), "Mood", 56.0, Some(3), key == 3);
        head(r, Some(Icon::Health), "Health", 56.0, Some(4), key == 4);
        head(r, Some(Icon::Hammer), "Best skill", 60.0, None, false);
    });
    for b in rows.into_iter().take(40) {
        let (mi, mc) = mood_icon(b.mood);
        u.row(8.0, |r| {
            r.row(4.0, |x| {
                x.icon(stage_icon(b.stage), 14.0, MUTED);
                x.text(b.name.clone(), 12.0, INK);
            })
            .style(|n| n.width = Val::Px(130.0));
            cell_num(r, format!("{:.1}", b.age_years()), 56.0, INK);
            r.node(Node { width: Val::Px(64.0), justify_content: JustifyContent::FlexEnd, ..default() }, |x| match s.work.slot_of(b.id) {
                Some((_, _, role)) => {
                    x.icon(role_icon(role), 15.0, INK).tip(Tip::name(role_name(role)));
                }
                None => {
                    x.icon(Icon::Idle, 15.0, WARN).tip(Tip::name("Free"));
                }
            });
            r.node(Node { width: Val::Px(44.0), justify_content: JustifyContent::FlexEnd, ..default() }, |x| {
                x.icon(if b.lodge.is_some() { Icon::Home } else { Icon::Cross }, 14.0, if b.lodge.is_some() { MUTED } else { WARN });
            });
            r.node(Node { width: Val::Px(56.0), justify_content: JustifyContent::FlexEnd, column_gap: Val::Px(3.0), ..row_node(3.0) }, |x| {
                x.icon(mi, 13.0, mc);
                x.bold(format!("{:.0}", b.mood * 100.0), 12.0, INK);
            });
            cell_num(r, format!("{:.0}", b.health * 100.0), 56.0, if b.health < 0.5 { BAD } else { INK });
            let best = sim::colony::Job::ALL.into_iter().max_by(|x, y| b.skills[x.index()].total_cmp(&b.skills[y.index()]));
            r.node(Node { width: Val::Px(60.0), justify_content: JustifyContent::FlexEnd, ..default() }, |x| {
                if let Some(j) = best {
                    x.text(format!("{:.0}", b.skills[j.index()] * 100.0), 12.0, MUTED).tip(Tip::name(format!("{j:?}")));
                }
            });
        })
        .action(HudAction::SelectBeaver(b.id));
    }
}

fn role_rank(r: Role) -> usize {
    ROLES.iter().position(|x| *x == r).unwrap_or(99)
}

/// Roles in board order (until the sim's role priority list lands).
const ROLES: [Role; 8] = [
    Role::Build,
    Role::Collect(sim::resources::ResourceKind::Plant),
    Role::Collect(sim::resources::ResourceKind::Tree),
    Role::Haul,
    Role::Collect(sim::resources::ResourceKind::MudBank),
    Role::Collect(sim::resources::ResourceKind::StoneOutcrop),
    Role::Craft,
    Role::Study,
];

// ───────────────────────────── jobs ─────────────────────────────

pub fn jobs(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let idle = sim::hud::idle_beavers(s).len();
    let working = s.colony.beavers.iter().filter(|b| s.work.slot_of(b.id).is_some()).count();
    u.row(14.0, |r| {
        for (icon, n, name, col) in [
            (Icon::Jobs, working, "Working", INK),
            (Icon::Idle, idle, "Free", if idle > 0 { WARN } else { INK }),
            (Icon::Kit, s.colony.count_stage(sim::colony::Stage::Kit), "Kits", MUTED),
            (Icon::ElderBeaver, s.colony.count_stage(sim::colony::Stage::Elder), "Elders", MUTED),
        ] {
            r.row(4.0, |x| {
                x.icon(icon, 16.0, col);
                x.bold(format!("{n}"), 13.0, col);
            })
            .tip(Tip::name(name));
        }
    });
    u.rule();
    u.row(8.0, |r| {
        r.add(VNode::new(Node { width: Val::Px(28.0), ..default() }));
        head(r, None, "Filled", 60.0, None, false);
        r.add(VNode::new(Node { width: Val::Px(64.0), ..default() }));
        head(r, Some(Icon::Up), "Output per day", 120.0, None, false);
    });
    for role in ROLES {
        let (mut filled, mut slots) = (0, 0);
        let mut output: Vec<(Good, f32)> = Vec::new();
        for p in &s.work.places {
            for sl in p.slots.iter().filter(|sl| sl.role == role) {
                slots += 1;
                if sl.worker.is_some() {
                    filled += 1;
                }
            }
            if p.slots.iter().any(|sl| sl.role == role) && p.site.is_none() {
                if let Some(f) = s.output.of(p.building) {
                    for (g, r) in f.rates() {
                        match output.iter_mut().find(|o| o.0 == g) {
                            Some(o) => o.1 += r,
                            None => output.push((g, r)),
                        }
                    }
                }
            }
        }
        if slots == 0 && filled == 0 {
            continue;
        }
        u.row(8.0, |r| {
            r.icon(role_icon(role), 20.0, INK).tip(Tip::name(role_name(role)));
            r.node(Node { width: Val::Px(60.0), justify_content: JustifyContent::FlexEnd, ..default() }, |x| {
                x.bold(format!("{filled}/{slots}"), 13.0, if filled < slots { WARN } else { INK });
            });
            r.icon_button(Icon::Minus, 11.0, HudAction::RoleAdjust(role, -1), FAINT, false).tip(Tip::name("Free one"));
            r.icon_button(Icon::Plus, 11.0, HudAction::RoleAdjust(role, 1), if filled < slots { FAINT } else { Color::NONE }, false).tip(Tip::name(if idle > 0 { "Add a free beaver" } else { "No one free" }));
            r.row(6.0, |x| {
                for (g, v) in &output {
                    x.row(2.0, |y| {
                        y.icon(good_icon(*g), 14.0, good_color(*g));
                        y.bold(num(*v), 12.0, GOOD);
                    });
                }
            })
            .style(|n| n.min_width = Val::Px(120.0));
        });
    }
}

// ───────────────────────────── goods ─────────────────────────────

fn goods(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let w = [40.0, 58.0, 58.0, 58.0, 58.0, 58.0];
    u.row(6.0, |r| {
        r.add(VNode::new(Node { width: Val::Px(w[0]), ..default() }));
        head(r, None, "Stock", w[1], None, false);
        head(r, None, "In", w[2], None, false);
        head(r, None, "Out", w[3], None, false);
        head(r, None, "Net", w[4], None, false);
        head(r, Some(Icon::Hourglass), "Lasts", w[5], None, false);
        head(r, None, "60d", 130.0, None, false);
    });
    let st = &s.buildings.stores;
    for g in Good::ALL {
        let (have, inn, out) = (st.amount(g), s.ledger.in_per_day(g), s.ledger.out_per_day(g));
        if have < 0.05 && inn < 0.01 && out < 0.01 {
            continue;
        }
        let lasts = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, have) };
        u.row(6.0, |r| {
            r.node(Node { width: Val::Px(w[0]), ..default() }, |x| {
                x.icon(good_icon(g), 18.0, good_color(g)).tip(Tip::name(good_name(g)));
            });
            cell_num(r, format!("{}", have.round() as i64), w[1], INK);
            cell_num(r, num(inn), w[2], if inn > 0.0 { GOOD } else { MUTED });
            cell_num(r, num(out), w[3], if out > 0.0 { BAD } else { MUTED });
            let net = inn - out;
            cell_num(r, signed(net), w[4], if net > 0.05 { GOOD } else if net < -0.05 { BAD } else { MUTED });
            cell_num(r, lasts.map_or("-".to_string(), days), w[5], if lasts.is_some_and(|d| d < 20.0) { WARN } else { MUTED });
            let hist: Vec<f32> = s.ledger.days.iter().map(|d| d.stock[g.index()]).collect();
            let top = hist.iter().copied().fold(1.0f32, f32::max);
            r.node(Node { height: Val::Px(18.0), align_items: AlignItems::FlexEnd, column_gap: Val::Px(1.0), margin: UiRect::left(Val::Px(8.0)), ..row_node(1.0) }, |sp| {
                for v in &hist {
                    sp.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px((v / top * 18.0).max(1.0)), ..default() })).bg(good_color(g));
                }
            });
        })
        .tip(super::panels::good_tip(s, g));
    }
    u.rule();
    // Sources: who makes what.
    let mut makers: Vec<(sim::buildings::BuildingId, sim::output::Flow)> = s.output.by.iter().map(|(b, f)| (*b, *f)).filter(|(_, f)| f.sum_per_day() > 0.01).collect();
    makers.sort_by(|a, b| b.1.sum_per_day().total_cmp(&a.1.sum_per_day()));
    u.row(6.0, |r| {
        r.bold("Sources", 12.0, MUTED);
        if makers.is_empty() {
            r.text("-", 12.0, MUTED);
        }
    });
    for (id, f) in makers.into_iter().take(10) {
        let Some(b) = s.buildings.building(id) else { continue };
        u.row(8.0, |r| {
            r.icon(building_icon(b.kind), 18.0, INK).tip(Tip::name(b.kind.spec().name));
            for (g, v) in f.rates() {
                r.row(2.0, |x| {
                    x.icon(good_icon(g), 14.0, good_color(g));
                    x.bold(num(v), 12.0, GOOD);
                });
            }
            r.spacer();
            r.icon_button(Icon::Focus, 11.0, HudAction::Focus(b.pos), FAINT, false).tip(Tip::name("Show"));
        });
    }
    // Stores: stock held in each.
    let stores: Vec<&sim::buildings::Building> = s.buildings.buildings.iter().filter(|b| b.is_complete() && matches!(b.kind, BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::FoodCache)).collect();
    if !stores.is_empty() {
        u.rule();
        for b in stores {
            u.row(8.0, |r| {
                r.icon(building_icon(b.kind), 18.0, INK).tip(Tip::name(b.kind.spec().name));
                for g in Good::ALL {
                    let have = s.work.stock_of(b.id, g);
                    if have > 0.05 {
                        r.row(2.0, |x| {
                            x.icon(good_icon(g), 14.0, good_color(g));
                            x.bold(format!("{}", have.round() as i64), 12.0, INK);
                        });
                    }
                }
                r.spacer();
                r.icon_button(Icon::Focus, 11.0, HudAction::Focus(b.pos), FAINT, false).tip(Tip::name("Show"));
            });
        }
    }
}

// ───────────────────────────── water ─────────────────────────────

fn water(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let ponds = sim::hud::ponds(s);
    let season = sim::calendar::SEASON_DAYS as f32;
    u.row(16.0, |r| {
        let cover = ponds.summer_cover_days();
        for (icon, v, name) in [
            (Icon::Gauge, if ponds.dammed { num(ponds.volume) } else { "-".into() }, "Stored"),
            (Icon::Flood, format!("{}", ponds.cells), "Pond cells"),
            (Icon::Summer, format!("{}/d", num(ponds.summer_loss_per_day)), "Summer loss"),
            (Icon::Hourglass, cover.map_or("-".to_string(), |d| format!("{} / {}", days(d.min(999.0)), days(season))), "Cover"),
        ] {
            r.row(4.0, |x| {
                x.icon(icon, 16.0, WATER);
                x.bold(v, 13.0, INK);
            })
            .tip(Tip::name(name));
        }
    });
    u.rule();
    let dams: Vec<&sim::buildings::Building> = s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dam).collect();
    if dams.is_empty() {
        u.text("-", 12.0, MUTED);
    }
    for d in dams {
        let crest = d.cells().filter_map(|(x, y)| s.water.dam_height.get(x, y).copied()).fold(0.0f32, f32::max);
        let up = d.cells().filter_map(|(x, y)| s.water.depth.get(x, y).copied()).fold(0.0f32, f32::max);
        u.row(10.0, |r| {
            r.icon(Icon::Dam, 18.0, INK);
            r.row(3.0, |x| {
                x.icon(Icon::Crest, 14.0, MUTED);
                x.bold(num(crest), 12.0, INK);
            })
            .tip(Tip::name("Crest"));
            r.row(3.0, |x| {
                x.icon(Icon::Drop, 14.0, WATER);
                x.bold(format!("{:.2}", up), 12.0, INK);
            })
            .tip(Tip::name("Depth at dam"));
            r.row(3.0, |x| {
                x.icon(Icon::Hammer, 14.0, MUTED);
                x.bar(if d.is_complete() { 1.0 } else { 0.5 }, 40.0, 5.0, if d.is_complete() { GOOD } else { WARN });
            })
            .tip(Tip::name(if d.is_complete() { "Built" } else { "Building" }));
            r.spacer();
            r.icon_button(Icon::Focus, 11.0, HudAction::Focus(d.pos), FAINT, false).tip(Tip::name("Show"));
        });
    }
}

// ───────────────────────────── land ─────────────────────────────

fn land(c: &Ctx, u: &mut Ui) {
    let e = &c.s.ecology;
    let sum = |g: &sim::grid::Grid<f32>| g.as_slice().iter().sum::<f32>();
    let trees = c.s.resources.items.iter().filter(|r| r.kind == sim::resources::ResourceKind::Tree).count();
    let plants = c.s.resources.items.iter().filter(|r| r.kind == sim::resources::ResourceKind::Plant).count();
    for (icon, col, v, name) in [
        (Icon::Tree, GOOD, format!("{trees}"), "Trees"),
        (Icon::Plant, GOOD, format!("{plants}"), "Food plants"),
        (Icon::ViewForest, GOOD, num(sum(&e.forest)), "Forest cover"),
        (Icon::ViewVegetation, GOOD, num(sum(&e.meadow)), "Meadow"),
        (Icon::Flood, WATER, num(sum(&e.wetland)), "Wetland"),
        (Icon::Drop, WATER, num(sum(&e.fish)), "Fish"),
    ] {
        u.row(8.0, |r| {
            r.icon(icon, 18.0, col);
            r.text(name, 12.0, MUTED);
            r.spacer();
            r.bold(v, 13.0, INK);
        })
        .style(|n| n.max_width = Val::Px(320.0));
    }
}

// ───────────────────────────── research ─────────────────────────────

fn research(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    for (i, t) in sim::tech::TECHS.iter().enumerate() {
        let id = sim::tech::TechId(i);
        let known = s.tech.is_unlocked(id);
        let gaps = s.tech.needs(id, &s.colony);
        let unlocks: Vec<BuildingKind> = t.effects.iter().filter_map(|e| if let sim::tech::Effect::Unlock(k) = e { Some(*k) } else { None }).collect();
        let mut tip = Tip::titled(Some((if known { Icon::Check } else { Icon::Padlock }, if known { GOOD } else { MUTED })), t.name);
        for g in &gaps {
            tip.push(gap_line(g));
        }
        u.row(8.0, |r| {
            r.icon(if known { Icon::Check } else { Icon::Padlock }, 14.0, if known { GOOD } else { MUTED });
            r.text(t.name, 12.0, if known { INK } else { MUTED }).style(|n| n.width = Val::Px(140.0));
            for k in &unlocks {
                r.icon(building_icon(*k), 16.0, if known { INK } else { MUTED }).tip(Tip::name(k.spec().name));
            }
            r.spacer();
            if !known {
                if let Some(sim::tech::Gap::Experience { have, need, .. }) = gaps.iter().find(|g| matches!(g, sim::tech::Gap::Experience { .. })) {
                    r.bar(have / need, 80.0, 5.0, INK);
                }
                if let Some(first) = gaps.first() {
                    let l = gap_line(first);
                    if let Some((i, col)) = l.icon {
                        r.icon(i, 14.0, col);
                    }
                    if !l.value.is_empty() {
                        r.bold(l.value, 12.0, MUTED);
                    }
                }
            }
        })
        .tip(tip);
    }
}

// ───────────────────────────── log ─────────────────────────────

fn log(c: &Ctx, u: &mut Ui) {
    if c.log.is_empty() {
        u.text("-", 12.0, MUTED);
    }
    for line in c.log.iter().rev().take(24) {
        u.text(line.clone(), 12.0, INK);
    }
}
