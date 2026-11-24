//! A lodge's family page, full screen, opened from the lodge card or from a beaver's page: who
//! lives there, and the family tree from the residents' grandparents down, each couple with
//! their litters in order and each kit under its litter. Every beaver is a link to its page.
use super::kin::{self, name_of};
use super::life::{about, when};
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::colony::{BeaverId, LodgeId};
use std::collections::BTreeSet;

/// Tree lines shown at once (the wheel scrolls through the rest).
const ROWS: usize = 26;
/// Indent per generation.
const STEP: f32 = 18.0;

enum Line {
    /// A beaver at this depth, with its partners (current, or those it had kits with).
    Beaver { depth: usize, id: BeaverId, partners: Vec<BeaverId> },
    /// One of a couple's litters.
    Litter { depth: usize, number: usize, year: i64, kits: usize },
}

fn residents(c: &Ctx, lodge: LodgeId) -> Vec<BeaverId> {
    let mut v: Vec<&sim::colony::Beaver> = c.s.colony.beavers.iter().filter(|b| b.lodge == Some(lodge)).collect();
    v.sort_by_key(|b| (std::cmp::Reverse(b.stage), b.id.0));
    v.into_iter().map(|b| b.id).collect()
}

/// The family tree as lines: from the residents' oldest known forebears (up to their
/// grandparents) down through every litter.
fn tree(s: &sim::SimState, home: &[BeaverId]) -> Vec<Line> {
    let all = kin::everyone(s);
    let up = |ids: &BTreeSet<BeaverId>| -> BTreeSet<BeaverId> { ids.iter().flat_map(|&i| kin::parents(s, i)).flatten().collect() };
    let mut near: BTreeSet<BeaverId> = home.iter().copied().collect();
    let parents = up(&near);
    let grand = up(&parents);
    near.extend(parents);
    near.extend(grand);
    let mut roots: Vec<BeaverId> = near.iter().copied().filter(|&i| kin::parents(s, i).iter().flatten().all(|p| !near.contains(p))).collect();
    roots.sort_by(|a, b| kin::born(s, *a).unwrap_or(f64::MIN).total_cmp(&kin::born(s, *b).unwrap_or(f64::MIN)).then(a.cmp(b)));
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for r in roots {
        descend(s, &all, r, 0, &mut seen, &mut out);
    }
    out
}

fn descend(s: &sim::SimState, all: &[BeaverId], id: BeaverId, depth: usize, seen: &mut BTreeSet<BeaverId>, out: &mut Vec<Line>) {
    if !seen.insert(id) || depth > 12 {
        return;
    }
    let kits = kin::children(s, all, id);
    // Partners: its mate, and anyone it had kits with.
    let mut partners: Vec<BeaverId> = kin::partner_of(s, id).into_iter().collect();
    for &k in &kits {
        for p in kin::parents(s, k).into_iter().flatten().filter(|&p| p != id) {
            if !partners.contains(&p) {
                partners.push(p);
            }
        }
    }
    // A partner shown here is not shown again as a root of its own.
    for p in &partners {
        seen.insert(*p);
    }
    out.push(Line::Beaver { depth, id, partners });
    // Litters, oldest first: kits with the same other parent born the same year.
    let mut litters: Vec<(i64, Option<BeaverId>, Vec<BeaverId>)> = Vec::new();
    for k in kits {
        let other = kin::parents(s, k).into_iter().flatten().find(|&p| p != id);
        let year = kin::born_year(s, k).unwrap_or(0);
        match litters.iter_mut().find(|l| l.0 == year && l.1 == other) {
            Some(l) => l.2.push(k),
            None => litters.push((year, other, vec![k])),
        }
    }
    for (year, _, ks) in litters {
        let number = kin::litter(s, all, ks[0]).map_or(0, |l| l.number);
        out.push(Line::Litter { depth: depth + 1, number, year, kits: ks.len() });
        for k in ks {
            descend(s, all, k, depth + 2, seen, out);
        }
    }
}

/// How many tree lines a lodge's page has, for scrolling.
pub fn tree_len(s: &sim::SimState, lodge: LodgeId) -> usize {
    let home: Vec<BeaverId> = s.colony.beavers.iter().filter(|b| b.lodge == Some(lodge)).map(|b| b.id).collect();
    tree(s, &home).len()
}

/// Lines down the left of a tree line, one per generation above it.
fn guides(r: &mut Ui, depth: usize) {
    for _ in 0..depth {
        r.add(VNode::new(Node { width: Val::Px(STEP), align_self: AlignSelf::Stretch, border: UiRect::left(Val::Px(1.0)), flex_shrink: 0.0, ..default() }))
            .border(BorderColor::all(Color::srgba(0.6, 0.55, 0.48, 0.45)));
    }
}

/// A beaver in the tree: a link to its page, marked when it lives in this lodge.
fn chip(r: &mut Ui, s: &sim::SimState, id: BeaverId, home: &[BeaverId]) {
    let b = s.colony.beaver(id);
    let here = home.contains(&id);
    r.row(5.0, |x| {
        match b {
            Some(b) => {
                x.icon(stage_icon(b.stage), 13.0, if here { INK } else { MUTED });
            }
            None => {
                x.icon(Icon::Cross, 11.0, MUTED);
            }
        }
        x.bold(name_of(s, id), 12.0, if b.is_none() { MUTED } else { INK });
        if here {
            x.icon(Icon::Lodge, 11.0, MUD);
        }
    })
    .action(HudAction::BeaverDetails(Some(id)))
    .tip(Tip::name(name_of(s, id)).line(about(s, id), "").line("Open their page", ""));
}

pub fn screen(c: &Ctx, lodge: LodgeId) -> Vec<VNode> {
    let s = c.s;
    let l = s.colony.lodges.iter().find(|l| l.id == lodge);
    let fam = l.and_then(|l| s.colony.family(l.family)).map_or_else(|| "Lodge".to_string(), |f| f.name.clone());
    let home = residents(c, lodge);
    let lines = tree(s, &home);
    let mut ui = Ui::default();
    ui.node(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(76.0),
            right: Val::Px(20.0),
            top: Val::Px(70.0),
            bottom: Val::Px(96.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            overflow: Overflow::clip(),
            ..default()
        },
        |u| {
            u.row(14.0, |r| {
                r.portrait(Icon::Lodge, MUD);
                r.col(6.0, |x| {
                    x.bold(fam.clone(), 20.0, INK);
                    if let Some(l) = l {
                        x.pips(l.condition, if l.condition < 0.35 { BAD } else if l.condition < 0.65 { WARN } else { GOOD });
                    }
                });
                r.spacer();
                r.pebble(Icon::Cross, 11.0, HudAction::LodgePage(None), Peb::Bare).tip(Tip::name("Close").key("Esc"));
            });
            u.rule();
            u.row(28.0, |cols| {
                // Who lives here.
                cols.node(Node { width: Val::Px(300.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), flex_shrink: 0.0, ..default() }, |u| {
                    u.row(8.0, |r| {
                        r.icon(Icon::Lodge, 16.0, MUD);
                        r.bold(format!("Living here · {} / {}", home.len(), sim::colony::LODGE_CAPACITY), 14.0, INK);
                    });
                    for &id in &home {
                        let Some(b) = s.colony.beaver(id) else { continue };
                        u.row(8.0, |r| {
                            r.icon(stage_icon(b.stage), 15.0, INK);
                            r.bold(b.name.clone(), 12.0, INK);
                            r.text(about(s, id), 11.0, MUTED);
                            r.spacer();
                            if let Some(j) = sim::lives::job_of(s, id) {
                                r.icon(role_kind_icon(j), 13.0, INK).tip(Tip::name(role_kind_name(j)));
                            }
                        })
                        .action(HudAction::BeaverDetails(Some(id)))
                        .tip(Tip::name(b.name.clone()).line("Open their page", ""));
                    }
                    if home.is_empty() {
                        u.text("Nobody lives here".to_string(), 12.0, MUTED);
                    }
                });
                // The family tree.
                cols.node(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(0.0), ..default() }, |u| {
                    u.row(8.0, |r| {
                        r.icon(Icon::Beaver, 16.0, FUR);
                        r.bold("Family tree".to_string(), 14.0, INK);
                    });
                    let total = lines.len();
                    let skip = c.hud.scroll.min(total.saturating_sub(1));
                    for line in lines.iter().skip(skip).take(ROWS) {
                        u.row(0.0, |r| match line {
                            Line::Beaver { depth, id, partners } => {
                                guides(r, *depth);
                                r.row(6.0, |x| {
                                    chip(x, s, *id, &home);
                                    for p in partners {
                                        x.text("&".to_string(), 11.0, MUTED);
                                        chip(x, s, *p, &home);
                                    }
                                    if let Some(d) = kin::born(s, *id) {
                                        x.text(format!("b. {}", when(d)), 10.0, MUTED);
                                    }
                                })
                                .style(|n| n.padding = UiRect::axes(Val::Px(4.0), Val::Px(3.0)));
                            }
                            Line::Litter { depth, number, year, kits } => {
                                guides(r, *depth);
                                r.row(5.0, |x| {
                                    x.icon(Icon::Kit, 12.0, GOOD);
                                    let n = if *number > 0 { format!("Litter {number}") } else { "Litter".to_string() };
                                    x.text(format!("{n} · {} · {kits}", kin::year_word(*year)), 11.0, MUTED);
                                })
                                .style(|n| n.padding = UiRect::axes(Val::Px(4.0), Val::Px(2.0)));
                            }
                        });
                    }
                    if total > ROWS {
                        u.text(format!("{}–{} of {} · scroll for more", skip + 1, (skip + ROWS).min(total), total), 11.0, MUTED);
                    }
                    if total == 0 {
                        u.text("No family known".to_string(), 12.0, MUTED);
                    }
                });
            })
            .style(|n| n.align_items = AlignItems::FlexStart);
        },
    )
    .tag(Tag::Scroll);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(18.0));
    vec![v]
}

