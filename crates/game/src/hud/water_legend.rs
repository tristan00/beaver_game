use super::ui::*;
use super::words::num;
use crate::palette::Rgb;
use crate::icons::Icon;
use crate::terrain_render as tr;
use bevy::prelude::*;

fn rgb(c: Rgb) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

fn swatch(u: &mut Ui, c: Color) {
    u.add(VNode::new(Node { width: Val::Px(14.0), height: Val::Px(10.0), ..default() })).bg(c);
}

/// Over the lens rail while the water view is on (or a dam or canal is being drawn): its depth
/// bands, up to the depths that flood buildings, and the current's colours, in one row.
pub fn legend(active: super::MapLens, node: Node) -> Option<VNode> {
    if active != super::MapLens::Water {
        return None;
    }
    let mut ui = Ui::default();
    ui.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(12.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), ..node }, |u| {
        u.icon(Icon::ViewWater, 14.0, WATER).tip(Tip::name("Water").key("F1"));
        let row = |u: &mut Ui, f: &dyn Fn(&mut Ui), label: String, tip: Option<&str>| {
            let r = u.row(4.0, |x| {
                f(x);
                x.text(label, 11.0, INK);
            });
            if let Some(t) = tip {
                r.tip(Tip::name(t));
            }
        };
        // Each band up to its deepest edge; the last, flooded, from the flood depth on.
        let (near, max) = tr::flood_depths();
        let n = tr::DEPTH_BANDS.len() as u8;
        for band in 0..=n + 2 {
            let edge = match band {
                0 => Some(sim::raft_landing::RAFT_DEPTH),
                b if b <= n => Some(tr::DEPTH_BANDS[b as usize - 1].0.min(near)),
                b if b == n + 1 => Some(max),
                _ => None,
            };
            let label = edge.map_or_else(|| format!("> {} m", num(max)), |e| format!("< {} m", num(e)));
            let (mark, tip) = match band {
                0 => (None, Some("Too shallow for rafts")),
                b if b == n + 1 => (Some(Icon::Warn), Some("Buildings near flooding")),
                b if b == n + 2 => (Some(Icon::Flood), Some("Buildings flood")),
                _ => (None, None),
            };
            let c = rgb(tr::band_color(band));
            row(u, &|x| {
                swatch(x, c);
                if let Some(m) = mark {
                    x.icon(m, 12.0, c);
                }
            }, label, tip);
        }
        row(
            u,
            &|x| {
                x.row(2.0, |r| {
                    for v in [0.0, 0.3, 1.0] {
                        r.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px(10.0), ..default() })).bg(super::world::current_colour(v));
                    }
                });
            },
            "Current".into(),
            Some("Slow, steady, fast"),
        );
    });
    let mut v = ui.nodes.pop()?;
    panel(&mut v);
    Some(v)
}
