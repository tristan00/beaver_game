use super::kin::{self, name_of};
use super::life::about;
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::BuildingId;
use sim::colony::BeaverId;
use std::collections::BTreeSet;

const CHIP_W: f32 = 92.0;
const CHIP_H: f32 = 24.0;
const LINK: f32 = 16.0;
const GAP: f32 = 14.0;
const COL_GAP: f32 = 6.0;
const STACK: f32 = 4.0;
const DROP: f32 = 34.0;
const TREE_W: f32 = 1080.0;
const WIRE: f32 = 2.0;
const WIRE_COL: Color = Color::srgba(0.78, 0.66, 0.5, 0.55);

struct Unit {
    id: BeaverId,
    partners: Vec<BeaverId>,
    kids: Vec<Unit>,
    leaves: Vec<BeaverId>,
}

fn residents(s: &sim::SimState, lodge: BuildingId) -> Vec<BeaverId> {
    let mut v: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.lodge == Some(lodge)).collect();
    v.sort_by_key(|b| (std::cmp::Reverse(b.stage), b.id.0));
    v.into_iter().map(|b| b.id).collect()
}

fn forest(s: &sim::SimState, home: &[BeaverId]) -> Vec<Unit> {
    let all = kin::everyone(s);
    let up = |ids: &BTreeSet<BeaverId>| -> BTreeSet<BeaverId> { ids.iter().flat_map(|&i| kin::parents(s, i)).flatten().collect() };
    let mut near: BTreeSet<BeaverId> = home.iter().copied().collect();
    let parents = up(&near);
    let grand = up(&parents);
    near.extend(parents);
    near.extend(grand);
    let mut roots: Vec<BeaverId> = near.iter().copied().filter(|&i| kin::parents(s, i).iter().flatten().all(|p| !near.contains(p))).collect();
    roots.sort_by(|a, b| kin::born(s, *a).unwrap_or(f64::MIN).total_cmp(&kin::born(s, *b).unwrap_or(f64::MIN)).then(a.cmp(b)));
    let mut seen = BTreeSet::new();
    roots.into_iter().filter_map(|r| unit(s, &all, r, 0, &mut seen)).collect()
}

fn unit(s: &sim::SimState, all: &[BeaverId], id: BeaverId, depth: usize, seen: &mut BTreeSet<BeaverId>) -> Option<Unit> {
    if !seen.insert(id) {
        return None;
    }
    let kits: Vec<BeaverId> = kin::children(s, all, id).into_iter().filter(|k| !seen.contains(k)).collect();
    let mut partners: Vec<BeaverId> = kin::partner_of(s, id).into_iter().filter(|p| !seen.contains(p)).collect();
    for &k in &kits {
        for p in kin::parents(s, k).into_iter().flatten().filter(|&p| p != id && !seen.contains(&p)) {
            if !partners.contains(&p) {
                partners.push(p);
            }
        }
    }
    seen.extend(partners.iter().copied());
    let (mut kids, mut leaves) = (Vec::new(), Vec::new());
    for k in kits {
        let family = kin::partner_of(s, k).is_some() || !kin::children(s, all, k).is_empty();
        if family && depth < 12 {
            kids.extend(unit(s, all, k, depth + 1, seen));
        } else if seen.insert(k) {
            leaves.push(k);
        }
    }
    Some(Unit { id, partners, kids, leaves })
}

#[derive(Default)]
struct Drawing {
    chips: Vec<(BeaverId, Vec2)>,
    wires: Vec<Rect>,
    size: Vec2,
}

struct Layout {
    per: usize,
}

impl Layout {
    fn couple_w(u: &Unit) -> f32 {
        let n = u.partners.len() as f32;
        CHIP_W * (1.0 + n) + LINK * n
    }
    fn cols(&self, n: usize) -> usize {
        n.div_ceil(self.per)
    }
    fn leaves_w(&self, n: usize) -> f32 {
        let c = self.cols(n) as f32;
        if c == 0.0 { 0.0 } else { c * CHIP_W + (c - 1.0) * COL_GAP }
    }
    fn kids_w(&self, u: &Unit) -> f32 {
        let items = u.kids.len() + usize::from(!u.leaves.is_empty());
        let w: f32 = u.kids.iter().map(|k| self.width(k)).sum::<f32>() + self.leaves_w(u.leaves.len());
        w + GAP * items.saturating_sub(1) as f32
    }
    fn width(&self, u: &Unit) -> f32 {
        Self::couple_w(u).max(self.kids_w(u))
    }
    fn top(&self, d: usize) -> f32 {
        d as f32 * (CHIP_H + DROP)
    }
    fn place(&self, u: &Unit, x: f32, d: usize, out: &mut Drawing) -> f32 {
        let w = self.width(u);
        let y = self.top(d);
        let mut anchors = Vec::new();
        let mut cx = x + (w - self.kids_w(u)) / 2.0;
        for k in &u.kids {
            anchors.push(self.place(k, cx, d + 1, out));
            cx += self.width(k) + GAP;
        }
        let ky = self.top(d + 1);
        for (i, &leaf) in u.leaves.iter().enumerate() {
            let (col, row) = (i / self.per, i % self.per);
            let at = Vec2::new(cx + col as f32 * (CHIP_W + COL_GAP), ky + row as f32 * (CHIP_H + STACK));
            out.chips.push((leaf, at));
            if row == 0 {
                anchors.push(at.x + CHIP_W / 2.0);
            }
        }
        let cw = Self::couple_w(u);
        let hang_off = if u.partners.is_empty() { CHIP_W / 2.0 } else { CHIP_W + LINK / 2.0 };
        let mid = if anchors.is_empty() { x + w / 2.0 } else { (anchors[0] + anchors[anchors.len() - 1]) / 2.0 };
        let bx = (mid - hang_off).clamp(x, x + w - cw);
        out.chips.push((u.id, Vec2::new(bx, y)));
        for (i, &p) in u.partners.iter().enumerate() {
            let px = bx + (i + 1) as f32 * (CHIP_W + LINK);
            out.chips.push((p, Vec2::new(px, y)));
            out.wires.push(Rect::new(px - LINK, y + CHIP_H / 2.0 - WIRE / 2.0, px, y + CHIP_H / 2.0 + WIRE / 2.0));
        }
        if !anchors.is_empty() {
            let hang = bx + hang_off;
            let from = if u.partners.is_empty() { y + CHIP_H } else { y + CHIP_H / 2.0 };
            let bus = ky - DROP / 2.0;
            let wire = |x: f32, y0: f32, y1: f32| Rect::new(x - WIRE / 2.0, y0, x + WIRE / 2.0, y1);
            out.wires.push(wire(hang, from, bus + WIRE / 2.0));
            let (l, r) = anchors.iter().fold((hang, hang), |(l, r), &a| (l.min(a), r.max(a)));
            out.wires.push(Rect::new(l - WIRE / 2.0, bus - WIRE / 2.0, r + WIRE / 2.0, bus + WIRE / 2.0));
            for &a in &anchors {
                out.wires.push(wire(a, bus, ky));
            }
        }
        bx + CHIP_W / 2.0
    }
}

fn draw(s: &sim::SimState, home: &[BeaverId]) -> Drawing {
    let trees = forest(s, home);
    let mut lay = Layout { per: 4 };
    let total = |lay: &Layout| trees.iter().map(|t| lay.width(t)).sum::<f32>() + GAP * 2.0 * trees.len().saturating_sub(1) as f32;
    for per in [4, 6, 8, 12, 16, 24] {
        lay.per = per;
        if total(&lay) <= TREE_W {
            break;
        }
    }
    let mut out = Drawing::default();
    let mut x = 0.0;
    for t in &trees {
        lay.place(t, x, 0, &mut out);
        x += lay.width(t) + GAP * 2.0;
    }
    let bottom = out.chips.iter().map(|c| c.1.y + CHIP_H).fold(0.0, f32::max);
    out.size = Vec2::new(total(&lay).max(0.0), bottom);
    out
}

fn chip(u: &mut Ui, s: &sim::SimState, id: BeaverId, at: Vec2, home: &[BeaverId]) {
    let b = s.colony.beaver(id);
    let here = home.contains(&id);
    let node = Node {
        position_type: PositionType::Absolute,
        left: Val::Px(at.x),
        top: Val::Px(at.y),
        width: Val::Px(CHIP_W),
        height: Val::Px(CHIP_H),
        padding: UiRect::horizontal(Val::Px(6.0)),
        column_gap: Val::Px(5.0),
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(if here { 2.0 } else { 1.0 })),
        border_radius: BorderRadius::all(Val::Px(5.0)),
        overflow: Overflow::clip(),
        ..default()
    };
    let v = u.node(node, |x| {
        match b {
            Some(b) => {
                x.icon(stage_icon(b.stage), 13.0, if here { ACCENT } else { FUR });
                x.bold(name_of(s, id), 12.0, INK);
            }
            None => {
                x.icon(Icon::Cross, 10.0, MUTED);
                x.text(name_of(s, id), 12.0, MUTED);
            }
        }
    });
    match (b.is_some(), here) {
        (true, true) => v.bg(Color::srgba(0.91, 0.52, 0.23, 0.16)).border(BorderColor::all(ACCENT)),
        (true, false) => v.bg(Color::srgba(1.0, 1.0, 1.0, 0.08)).border(BorderColor::all(LINE)),
        (false, _) => v.border(BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.1))),
    };
    v.action(HudAction::Overlay(Some(super::Overlay::Page(super::Page::Beaver(id))))).tip(Tip::name(name_of(s, id)).line(about(s, id), "").line("Open their page", ""));
}

pub fn screen(c: &Ctx, lodge: BuildingId) -> Vec<VNode> {
    let s = c.s;
    let l = s.buildings.building(lodge);
    let title = s.colony.lodge_family(lodge).and_then(|f| s.colony.family(f))
        .map_or_else(|| sim::buildings::BuildingKind::home().name().to_string(), |f| format!("{} family", f.name));
    let home = residents(s, lodge);
    let tree = draw(s, &home);
    Ui::full_page_shell(
        |r| {
            r.portrait(crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), MUD);
            r.col(6.0, |x| {
                x.bold(title, 20.0, INK);
                if let Some(l) = l {
                    let cond = l.home_condition(&s.water.depth);
                    x.pips(cond, level_color(cond));
                }
            });
            r.spacer();
            super::life::page_controls(c, r, HudAction::Overlay(None));
        },
        |u| {
            u.row(28.0, |cols| {
                cols.node(Node { width: Val::Px(300.0), height: Val::Percent(100.0), min_height: Val::Px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), flex_shrink: 0.0, ..default() }, |u| {
                    u.row(8.0, |r| {
                        r.icon(crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), 16.0, MUD);
                        r.bold(format!("Living here · {} / {}", home.len(), sim::colony::LODGE_CAPACITY), 14.0, INK);
                    });
                    for &id in &home {
                        let Some(b) = s.colony.beaver(id) else { continue };
                        u.row(8.0, |r| {
                            r.icon(stage_icon(b.stage), 15.0, INK);
                            r.bold(b.name.clone(), 12.0, INK);
                            r.text(about(s, id), 11.0, MUTED);
                            r.spacer();
                            if let Some(j) = sim::jobs::role_of(s, id) {
                                r.icon(job_icon(j), 13.0, INK).tip(Tip::name(j.name()));
                            }
                            r.icon(Icon::Next, 10.0, MUTED);
                        })
                        .action(HudAction::Overlay(Some(super::Overlay::Page(super::Page::Beaver(id)))))
                        .tip(Tip::name(b.name.clone()).line("Open their page", ""));
                    }
                    if home.is_empty() {
                        u.text("Nobody lives here".to_string(), 12.0, MUTED);
                    }
                })
                .scroll_y();
                cols.node(Node { flex_grow: 1.0, align_self: AlignSelf::Stretch, flex_direction: FlexDirection::Column, row_gap: Val::Px(12.0), overflow: Overflow::clip(), ..default() }, |u| {
                    u.row(8.0, |r| {
                        r.icon(Icon::Beaver, 16.0, FUR);
                        r.bold("Family tree".to_string(), 14.0, INK);
                    });
                    if tree.chips.is_empty() {
                        u.text("No family known".to_string(), 12.0, MUTED);
                        return;
                    }
                    let canvas = Node {
                        width: Val::Px(tree.size.x),
                        height: Val::Px(tree.size.y),
                        align_self: AlignSelf::Center,
                        flex_shrink: 0.0,
                        ..default()
                    };
                    u.node(
                        Node { width: Val::Percent(100.0), flex_grow: 1.0, min_height: Val::Px(0.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
                        |wrap| {
                            wrap.node(canvas, |t| {
                                for w in &tree.wires {
                                    t.add(VNode::new(Node {
                                        position_type: PositionType::Absolute,
                                        left: Val::Px(w.min.x),
                                        top: Val::Px(w.min.y),
                                        width: Val::Px(w.width()),
                                        height: Val::Px(w.height()),
                                        ..default()
                                    }))
                                    .bg(WIRE_COL);
                                }
                                for &(id, at) in &tree.chips {
                                    chip(t, s, id, at, &home);
                                }
                            });
                        },
                    )
                    .scroll_y();
                });
            })
            .style(|n| {
                n.align_items = AlignItems::FlexStart;
                n.flex_grow = 1.0;
                n.min_height = Val::Px(0.0);
            });
        },
    )
}
