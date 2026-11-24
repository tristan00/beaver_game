//! The journal, bottom right: what happened, a line each — births and deaths, buildings
//! finished, a wolf seen. Problems that want the player stay as badges at the top.
use super::panels::Ctx;
use super::ui::*;
use super::words::*;
use super::{HudAction, NOTICE_SECS};
use crate::icons::Icon;
use bevy::prelude::*;

const W: f32 = 270.0;
const ROW: f32 = 20.0;
const FEW: usize = 10;
const MANY: usize = 24;

fn shown(c: &Ctx) -> usize {
    c.journal.len().min(if c.hud.journal_open { MANY } else { FEW })
}

/// How tall the journal stands, so the inspectors above it stop short of it.
pub fn height(c: &Ctx) -> f32 {
    if !c.s.is_running() {
        return 0.0;
    }
    let n = shown(c);
    if n == 0 { 36.0 } else { 16.0 + 24.0 + n as f32 * (ROW + 2.0) }
}

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let mut ui = Ui::default();
    let open = c.hud.journal_open;
    let n = shown(c);
    let v = ui.node(Node { position_type: PositionType::Absolute, right: Val::Px(12.0), bottom: Val::Px(12.0), width: if n == 0 { Val::Auto } else { Val::Px(W) }, flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() }, |u| {
        u.row(6.0, |r| {
            r.icon(Icon::Journal, 16.0, MUTED);
            r.spacer();
            if c.journal.len() > FEW {
                r.pebble(if open { Icon::Down } else { Icon::Up }, 9.0, HudAction::Journal(!open), Peb::Bare).tip(Tip::name(if open { "Fewer" } else { "More" }).key("J"));
            }
        })
        .tip(Tip::name("Journal").key("J"));
        for e in c.journal.iter().rev().take(n) {
            let fresh = c.now - e.born < NOTICE_SECS;
            let d = sim::calendar::Date::at(e.day);
            let row = u.row(5.0, |r| {
                r.node(Node { width: Val::Px(34.0), flex_shrink: 0.0, ..row_node(2.0) }, |x| {
                    x.icon(season_icon(d.season), 10.0, MUTED);
                    x.text(format!("{}", d.day), 10.0, MUTED);
                });
                for (i, col) in &e.icons {
                    r.icon(*i, 13.0, *col);
                }
                r.text(e.text.clone(), 12.0, if fresh { INK } else { MUTED }).style(|n| n.overflow = Overflow::clip());
            });
            row.node.height = Val::Px(ROW);
            row.node.overflow = Overflow::clip();
            if fresh {
                row.bg(Color::srgba(1.0, 1.0, 1.0, 0.05));
            }
            let mut tip = Tip::titled(e.icons.first().copied(), e.text.clone()).line(d.season.name(), format!("Year {} · {}", d.year, d.day));
            if e.focus.is_some() {
                tip = tip.line("Go there", "");
            }
            row.tip(tip);
            if let Some(at) = e.focus {
                row.action(HudAction::GoTo(at));
            }
        }
    });
    panel(v);
    v.node.padding = UiRect::axes(Val::Px(10.0), Val::Px(8.0));
    ui.nodes
}
