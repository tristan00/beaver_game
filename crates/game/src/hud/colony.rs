//! The colony drawer: a side panel beside the map, never covering the pond.
//! Every row is icons, gauges and numbers; names and units live in tooltips.
use super::inspect::mood_icon;
use super::panels::{food_days, stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingKind, Good};

pub const DRAWER_W: f32 = 430.0;

const TABS: [(ColonyTab, Icon, &str); 6] = [
    (ColonyTab::Population, Icon::Beaver, "Families"),
    (ColonyTab::Jobs, Icon::Jobs, "Work"),
    (ColonyTab::Goods, Icon::Colony, "Stores"),
    (ColonyTab::Water, Icon::Drop, "Water"),
    (ColonyTab::Land, Icon::Tree, "Land"),
    (ColonyTab::Log, Icon::Log, "Journal"),
];

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let Some(tab) = c.hud.colony else { return Vec::new() };
    let tab = if tab == ColonyTab::Buildings { ColonyTab::Population } else { tab };
    let mut ui = Ui::default();
    ui.node(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            width: Val::Px(DRAWER_W),
            top: Val::Px(58.0),
            max_height: Val::Percent(88.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            overflow: Overflow::clip(),
            ..default()
        },
        |u| {
            u.row(6.0, |r| {
                for (t, icon, name) in TABS {
                    let on = t == tab;
                    r.icon_button(icon, 18.0, HudAction::Colony(Some(t)), FAINT, on).tip(Tip::name(name).key(if t == ColonyTab::Jobs { "J" } else { "" }));
                }
                r.spacer();
                r.icon_button(Icon::Cross, 11.0, HudAction::Colony(None), FAINT, false).tip(Tip::name("Close").key("Tab"));
            });
            u.rule();
            match tab {
                ColonyTab::Population | ColonyTab::Buildings => population(c, u),
                ColonyTab::Jobs => jobs(c, u),
                ColonyTab::Goods => goods(c, u),
                ColonyTab::Water => water(c, u),
                ColonyTab::Land => land(c, u),
                ColonyTab::Log => log(c, u),
            }
        },
    );
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    vec![v]
}

fn head(u: &mut Ui, icon: Icon, name: &str, w: f32, sort: Option<u8>, active: bool) {
    let v = u.node(Node { width: Val::Px(w), justify_content: JustifyContent::Center, ..row_node(2.0) }, |x| {
        x.icon(icon, 13.0, if active { INK } else { MUTED });
        if active {
            x.icon(Icon::Down, 7.0, ACCENT);
        }
    });
    v.tip(Tip::name(name));
    if let Some(k) = sort {
        v.action(HudAction::SortPop(k));
    }
}

fn gauge(u: &mut Ui, v: f32, w: f32) {
    let col = if v < 0.35 { BAD } else if v < 0.6 { WARN } else { GOOD };
    u.bar(v, w, 6.0, col);
}

fn history_bars(u: &mut Ui, icon: Icon, col: Color, values: &[f32], name: &str) {
    let top = values.iter().copied().fold(1.0f32, f32::max);
    u.row(8.0, |r| {
        r.icon(icon, 16.0, col);
        r.node(Node { height: Val::Px(26.0), align_items: AlignItems::FlexEnd, column_gap: Val::Px(1.0), ..row_node(1.0) }, |b| {
            for v in values.iter().rev().take(72).rev() {
                b.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px((v / top * 26.0).max(1.0)), border_radius: BorderRadius::top(Val::Px(1.0)), ..default() })).bg(col);
            }
        });
        r.bold(num(top), 11.0, MUTED);
    })
    .tip(Tip::name(name).line("Every", "5d"));
}

fn population(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let pages = s.colony.population().div_ceil(18).max(1);
    let page = c.hud.pop_page.min(pages - 1);
    u.row(12.0, |r| {
        r.row(4.0, |x| {
            x.icon(Icon::Home, 15.0, INK);
            x.bold(format!("{}", s.colony.families.len()), 13.0, INK);
        })
        .tip(Tip::name("Families"));
        let expecting = s.colony.beavers.iter().filter(|b| b.pregnancy.is_some()).count();
        if expecting > 0 {
            r.row(4.0, |x| {
                x.icon(Icon::Kit, 14.0, GOOD);
                x.bold(format!("{expecting}"), 13.0, GOOD);
            })
            .tip(Tip::name("Litters on the way"));
        }
        r.spacer();
        if pages > 1 {
            r.icon_button(Icon::Up, 12.0, HudAction::PopPage(-1), FAINT, false).tip(Tip::name("Previous page"));
            r.bold(format!("{}/{pages}", page + 1), 12.0, MUTED);
            r.icon_button(Icon::Down, 12.0, HudAction::PopPage(1), FAINT, false).tip(Tip::name("Next page"));
        }
    });
    if c.history.len() > 1 {
        let pop: Vec<f32> = c.history.iter().map(|h| h.1 as f32).collect();
        history_bars(u, Icon::Beaver, INK, &pop, "Beavers");
    }
    u.rule();
    let mut rows: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().collect();
    let key = c.hud.pop_sort;
    rows.sort_by(|a, b| {
        let v = |x: &sim::colony::Beaver| -> f32 {
            match key {
                1 => x.age_days,
                2 => sim::jobs::role_of(s, x.id).map(|r| s.work.role_order.iter().position(|o| *o == r).unwrap_or(50) as f32).unwrap_or(99.0),
                3 => x.mood,
                4 => x.health,
                5 => -(x.family.0 as f32),
                _ => 0.0,
            }
        };
        if key == 0 { a.name.cmp(&b.name) } else { v(b).total_cmp(&v(a)) }
    });
    u.row(8.0, |r| {
        head(r, Icon::Beaver, "Name", 132.0, Some(0), key == 0);
        head(r, Icon::Hourglass, "Age", 34.0, Some(1), key == 1);
        head(r, Icon::Jobs, "Work", 28.0, Some(2), key == 2);
        head(r, Icon::Home, "Family", 28.0, Some(5), key == 5);
        head(r, Icon::MoodGood, "Mood", 76.0, Some(3), key == 3);
        head(r, Icon::Health, "Health", 76.0, Some(4), key == 4);
    });
    for b in rows.into_iter().skip(page * 18).take(18) {
        let (mi, mc) = mood_icon(b.mood);
        let family = s.colony.family(b.family).map_or("-".to_string(), |f| f.name.clone());
        u.row(8.0, |r| {
            r.row(5.0, |x| {
                x.icon(stage_icon(b.stage), 14.0, MUTED);
                x.text(b.name.clone(), 12.5, INK);
            })
            .style(|n| n.width = Val::Px(132.0));
            r.node(Node { width: Val::Px(34.0), justify_content: JustifyContent::Center, ..default() }, |x| {
                x.bold(format!("{:.0}", b.age_years().floor()), 12.0, MUTED);
            });
            r.node(Node { width: Val::Px(28.0), justify_content: JustifyContent::Center, ..default() }, |x| match sim::jobs::role_of(s, b.id) {
                Some(role) => {
                    x.icon(role_kind_icon(role), 15.0, INK).tip(Tip::name(role_kind_name(role)));
                }
                None => {
                    x.icon(Icon::Idle, 15.0, if b.stage == sim::colony::Stage::Kit { MUTED } else { WARN }).tip(Tip::name("Free"));
                }
            });
            r.node(Node { width: Val::Px(28.0), justify_content: JustifyContent::Center, ..default() }, |x| {
                x.icon(if b.lodge.is_some() { Icon::Home } else { Icon::Cross }, 14.0, if b.lodge.is_some() { MUTED } else { WARN });
            });
            r.row(4.0, |x| {
                x.icon(mi, 12.0, mc);
                gauge(x, b.mood, 56.0);
            })
            .style(|n| n.width = Val::Px(76.0));
            r.row(4.0, |x| {
                x.icon(Icon::Health, 12.0, MUTED);
                gauge(x, b.health, 56.0);
            })
            .style(|n| n.width = Val::Px(76.0));
        })
        .action(HudAction::SelectBeaver(b.id))
        .tip(Tip::name(b.name.clone()).line("Family", family).line("Age", format!("{:.1}", b.age_years())).line("Mood", format!("{:.0}%", b.mood * 100.0)).line("Health", format!("{:.0}%", b.health * 100.0)));
    }
}

fn pegs(u: &mut Ui, filled: usize, slots: usize) {
    u.row(3.0, |r| {
        for i in 0..slots.min(10) {
            r.add(VNode::new(Node { width: Val::Px(8.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }))
                .bg(if i < filled { GOOD } else { Color::srgba(1.0, 1.0, 1.0, 0.1) });
        }
        if slots > 10 {
            r.bold(format!("+{}", slots - 10), 11.0, MUTED);
        }
    });
}

pub fn jobs(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let h = sim::jobs::headcount(s);
    u.row(14.0, |r| {
        for (icon, n, name, col) in [
            (Icon::Jobs, h.working, "Working", INK),
            (Icon::Idle, h.free, "Free", if h.free > 0 { WARN } else { INK }),
            (Icon::Kit, h.kits, "Kits", MUTED),
        ] {
            r.row(4.0, |x| {
                x.icon(icon, 16.0, col);
                x.bold(format!("{n}"), 13.0, col);
            })
            .tip(Tip::name(name));
        }
        r.spacer();
        let auto = s.work.auto_organise;
        r.icon_button(Icon::Jobs, 16.0, HudAction::AutoStaff(!auto), if auto { GOOD.with_alpha(0.25) } else { FAINT }, auto)
            .tip(Tip::name("Auto-staffing").line(if auto { "On" } else { "Off" }, "").line("Free beavers fill jobs", ""));
    });
    u.rule();
    for row in sim::jobs::jobs_board(s) {
        let role = row.role;
        let share = s.work.role_weights[role as usize];
        u.row(8.0, |r| {
            r.icon_button(Icon::PriorityUp, 9.0, HudAction::RoleUp(role), Color::NONE, false).tip(Tip::name("Higher priority"));
            r.icon(role_kind_icon(role), 20.0, INK).tip(Tip::name(role_kind_name(role)).line("Priority", format!("{}", row.rank + 1)));
            pegs(r, row.filled, row.slots);
            r.spacer();
            r.row(6.0, |x| {
                for (g, v) in &row.output_per_day {
                    x.row(2.0, |y| {
                        y.icon(good_icon(*g), 13.0, good_color(*g));
                        y.bold(num(*v), 11.5, GOOD);
                    });
                }
            })
            .tip(Tip::name("Made per day"));
            let tip = match sim::jobs::role_add_preview(s, role) {
                Some(p) => {
                    let who = s.colony.beaver(p.who).map(|b| b.name.clone()).unwrap_or_default();
                    match p.from {
                        Some((_, from)) => Tip::name("Add one").row(TipLine::new(who, format!("from {}", role_kind_name(from).to_lowercase())).icon(role_kind_icon(from), MUTED)),
                        None => Tip::name("Add one").row(TipLine::new(who, "Free").icon(Icon::Idle, MUTED)),
                    }
                }
                None => Tip::name("No one to add"),
            };
            r.icon_button(Icon::Minus, 10.0, HudAction::RoleAdjust(role, -1), FAINT, false).tip(Tip::name("Free one"));
            r.icon_button(Icon::Plus, 10.0, HudAction::RoleAdjust(role, 1), FAINT, false).tip(tip);
            r.row(2.0, |x| {
                x.icon_button(Icon::Minus, 8.0, HudAction::RoleWeight(role, (share - 0.25).max(0.1)), Color::NONE, false).tip(Tip::name("Fewer automatic workers"));
                x.bar(share / 6.0, 30.0, 5.0, ACCENT);
                x.icon_button(Icon::Plus, 8.0, HudAction::RoleWeight(role, (share + 0.25).min(20.0)), Color::NONE, false).tip(Tip::name("More automatic workers"));
            })
            .tip(Tip::name("Automatic share").with_value(format!("{share:.1}")));
        });
    }
}

fn goods(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let st = &s.buildings.stores;
    for g in Good::ALL {
        let (have, cap, inn, out) = (st.amount(g), st.capacity(g), s.ledger.in_per_day(g), s.ledger.out_per_day(g));
        let lasts = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, have) };
        u.row(8.0, |r| {
            r.icon(good_icon(g), 22.0, good_color(g));
            r.col(3.0, |x| {
                x.row(6.0, |y| {
                    y.bold(format!("{}", have.round() as i64), 14.0, INK);
                    y.text(format!("/ {}", cap.round() as i64), 11.0, MUTED);
                    y.spacer();
                    if inn > 0.01 {
                        y.icon(Icon::Up, 9.0, GOOD);
                        y.bold(num(inn), 11.5, GOOD);
                    }
                    if out > 0.01 {
                        y.icon(Icon::Down, 9.0, BAD);
                        y.bold(num(out), 11.5, BAD);
                    }
                    if let Some(d) = lasts {
                        y.icon(Icon::Hourglass, 11.0, if d < 20.0 { WARN } else { MUTED });
                        y.bold(days(d), 11.5, if d < 20.0 { WARN } else { MUTED });
                    }
                });
                x.bar(if cap > 0.0 { have / cap } else { 0.0 }, DRAWER_W - 70.0, 7.0, good_color(g));
                let hist: Vec<f32> = s.ledger.days.iter().map(|d| d.stock[g.index()]).collect();
                let top = hist.iter().copied().fold(1.0f32, f32::max);
                x.node(Node { height: Val::Px(16.0), align_items: AlignItems::FlexEnd, column_gap: Val::Px(1.0), ..row_node(1.0) }, |sp| {
                    for v in &hist {
                        sp.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px((v / top * 16.0).max(1.0)), ..default() })).bg(good_color(g).with_alpha(0.6));
                    }
                });
            })
            .style(|n| n.flex_grow = 1.0);
        })
        .tip(super::panels::good_tip(s, g));
        u.rule();
    }
    let stores: Vec<&sim::buildings::Building> = s.buildings.buildings.iter().filter(|b| b.is_complete() && b.kind == BuildingKind::Warehouse).collect();
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
            r.icon_button(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), FAINT, false).tip(Tip::name("Open"));
        });
    }
}

fn water(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let ponds = sim::hud::ponds(s);
    let season = sim::calendar::SEASON_DAYS as f32;
    u.row(16.0, |r| {
        let cover = ponds.summer_cover_days();
        for (icon, v, name) in [
            (Icon::Gauge, if ponds.dammed { num(ponds.volume) } else { "-".into() }, "Stored water"),
            (Icon::Area, format!("{}", ponds.cells), "Pond cells"),
            (Icon::Summer, format!("{}/d", num(ponds.summer_loss_per_day)), "Summer loss"),
            (Icon::Hourglass, cover.map_or("-".to_string(), |d| format!("{} / {}", days(d.min(999.0)), days(season))), "Lasts through summer"),
        ] {
            r.row(4.0, |x| {
                x.icon(icon, 16.0, WATER);
                x.bold(v, 13.0, INK);
            })
            .tip(Tip::name(name));
        }
    });
    u.rule();
    for b in s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dam) {
        let Some(d) = b.dam.as_deref() else { continue };
        let health = sim::dams::health(s, d);
        let fill = d.pond.as_ref().map_or(0.0, |p| ((p.level - p.bottom) / (p.full_level - p.bottom).max(0.01)).clamp(0.0, 1.0));
        u.row(10.0, |r| {
            r.icon(Icon::Dam, 18.0, INK);
            r.row(3.0, |x| {
                x.icon(Icon::Hammer, 12.0, MUTED);
                gauge(x, health, 60.0);
            })
            .tip(Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)));
            r.row(3.0, |x| {
                x.icon(Icon::Gauge, 13.0, WATER);
                x.bar(fill, 80.0, 6.0, WATER);
            })
            .tip(Tip::name("Pond").with_value(format!("{:.0}%", fill * 100.0)));
            let left = sim::dams::loads_left(s, b.id);
            if left > 0 {
                r.row(3.0, |x| {
                    x.icon(Icon::Site, 12.0, MUTED);
                    x.bold(format!("{left}"), 12.0, MUTED);
                })
                .tip(Tip::name("Loads still to carry"));
            }
            r.spacer();
            r.icon_button(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), FAINT, false).tip(Tip::name("Open"));
        });
    }
}

fn land(c: &Ctx, u: &mut Ui) {
    let e = &c.s.ecology;
    let sum = |g: &sim::grid::Grid<f32>| g.as_slice().iter().sum::<f32>();
    let trees = c.s.resources.items.iter().filter(|r| r.kind == sim::resources::ResourceKind::Tree).count();
    let plants = c.s.resources.items.iter().filter(|r| r.kind == sim::resources::ResourceKind::Plant).count();
    let items = [
        (Icon::Tree, GOOD, format!("{trees}"), "Trees"),
        (Icon::Plant, GOOD, format!("{plants}"), "Food plants"),
        (Icon::ViewForest, GOOD, num(sum(&e.forest)), "Forest cover"),
        (Icon::ViewVegetation, GOOD, num(sum(&e.meadow)), "Meadow"),
        (Icon::Flood, WATER, num(sum(&e.wetland)), "Wetland"),
        (Icon::Drop, WATER, num(sum(&e.fish)), "Fish"),
    ];
    for pair in items.chunks(3) {
        u.row(18.0, |r| {
            for (icon, col, v, name) in pair {
                r.row(5.0, |x| {
                    x.icon(*icon, 18.0, *col);
                    x.bold(v.clone(), 13.0, INK);
                })
                .style(|n| n.width = Val::Px(120.0))
                .tip(Tip::name(*name));
            }
        });
    }
}

fn log(c: &Ctx, u: &mut Ui) {
    if c.log.is_empty() {
        u.icon(Icon::Log, 14.0, MUTED);
    }
    for line in c.log.iter().rev().take(24) {
        u.text(line.clone(), 12.0, INK);
    }
}
