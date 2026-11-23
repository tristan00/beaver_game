//! Minimap image updates and navigation. The map shows only explored ground, framed to fit it.
use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;

/// Pixels along each side of the minimap image.
const RES: usize = 160;
/// Fewest cells the minimap frames, so a small explored patch is not blown up to a few pixels.
const MIN_SPAN: f32 = 48.0;
/// Cells of margin around the explored ground.
const MARGIN: f32 = 4.0;

#[derive(Resource)]
pub(super) struct MinimapImage {
    pub(super) image: Handle<Image>,
    /// The square of cells the image shows: left, top and side.
    pub(super) view: (f32, f32, f32),
}

impl MinimapImage {
    /// Where a cell falls on the minimap, 0..1 each way.
    pub(super) fn to_map(&self, c: (f32, f32)) -> (f32, f32) {
        ((c.0 - self.view.0) / self.view.2, (c.1 - self.view.1) / self.view.2)
    }
}

pub(super) fn setup_minimap(mut commands: Commands, sim: Res<Sim>, mut images: ResMut<Assets<Image>>) {
    let r = RES as u32;
    let img = Image::new_fill(Extent3d { width: r, height: r, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    let n = sim.0.world.size as f32;
    commands.insert_resource(MinimapImage { image: images.add(img), view: (0.0, 0.0, n) });
}

/// The square around everything explored, with a margin, kept on the map.
fn explored_view(s: &sim::SimState) -> Option<(f32, f32, f32)> {
    let n = s.world.size;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..n {
        for x in 0..n {
            if s.fog.explored((x, y)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        return None;
    }
    let nf = n as f32;
    let span = ((x1 - x0).max(y1 - y0) as f32 + 1.0 + 2.0 * MARGIN).max(MIN_SPAN).min(nf);
    let (cx, cy) = ((x0 + x1) as f32 / 2.0 + 0.5, (y0 + y1) as f32 / 2.0 + 0.5);
    let left = (cx - span / 2.0).clamp(0.0, nf - span);
    let top = (cy - span / 2.0).clamp(0.0, nf - span);
    Some((left, top, span))
}

pub(super) fn update_minimap(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, img: Option<ResMut<MinimapImage>>, mut images: ResMut<Assets<Image>>) {
    let _p = ::sim::prof::scope("game/update minimap");
    let Some(mut img) = img else { return };
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let s = &sim.0;
    let Some(view) = explored_view(s) else { return };
    img.view = view;
    let n = s.world.size;
    let maxh = s.height.as_slice().iter().copied().fold(1.0f32, f32::max);
    let px = view.2 / RES as f32;
    let cell_at = |x: f32, y: f32| -> Option<(usize, usize)> {
        let (cx, cy) = (x.floor(), y.floor());
        (cx >= 0.0 && cy >= 0.0 && (cx as usize) < n && (cy as usize) < n).then_some((cx as usize, cy as usize))
    };
    let mut buf = vec![0u8; RES * RES * 4];
    for py in 0..RES {
        for qx in 0..RES {
            let Some((x, y)) = cell_at(view.0 + (qx as f32 + 0.5) * px, view.1 + (py as f32 + 0.5) * px) else { continue };
            if !s.fog.explored((x, y)) {
                continue;
            }
            let i = y * n + x;
            let h = s.height.as_slice()[i] / maxh;
            let d = s.water.depth.as_slice()[i];
            let c: [f32; 3] = if d > sim::buildings::WET_DEPTH {
                let k = (d / 3.0).clamp(0.0, 1.0).sqrt();
                [0.42 - 0.34 * k, 0.70 - 0.36 * k, 0.92 - 0.26 * k]
            } else {
                [0.30 + 0.45 * h, 0.40 + 0.38 * h, 0.26 + 0.40 * h]
            };
            let o = (py * RES + qx) * 4;
            buf[o..o + 4].copy_from_slice(&[(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
        }
    }
    let mut dot = |c: (f32, f32), r: i32, col: [u8; 3]| {
        let (u, v) = (((c.0 - view.0) / px) as i32, ((c.1 - view.1) / px) as i32);
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (u + dx, v + dy);
                if x >= 0 && y >= 0 && (x as usize) < RES && (y as usize) < RES {
                    let o = (y as usize * RES + x as usize) * 4;
                    buf[o..o + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                }
            }
        }
    };
    let big = i32::from(px < 0.6);
    for b in &s.buildings.buildings {
        for c in b.cells().filter(|c| s.fog.explored(*c)) {
            dot((c.0 as f32 + 0.5, c.1 as f32 + 0.5), big, [245, 245, 245]);
        }
    }
    for bv in &s.colony.beavers {
        dot((bv.pos.0, bv.pos.1), big, [232, 132, 58]);
    }
    // Like everything else on the map, a wolf shows only on ground the colony has explored.
    if let Some(w) = s.predators.wolf.as_ref().filter(|w| s.fog.explored(w.cell())) {
        dot(w.pos, big + 1, [220, 30, 30]);
    }
    if let Some(mut image) = images.get_mut(&img.image) {
        image.data = Some(buf);
    }
}

pub(super) fn minimap_click(q: Query<(&Interaction, &RelativeCursorPosition, &TagComp)>, map: Option<Res<MinimapImage>>, sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>) {
    let Some(map) = map else { return };
    for (i, rel, tag) in &q {
        if tag.0 != Tag::Minimap || *i != Interaction::Pressed {
            continue;
        }
        let Some(n) = rel.normalized else { continue };
        let size = sim.0.world.size as f32;
        let (x0, y0, span) = map.view;
        let cell = ((x0 + (n.x + 0.5) * span).clamp(0.0, size - 1.0), (y0 + (n.y + 0.5) * span).clamp(0.0, size - 1.0));
        orbit.target = cell_world(&sim.0.height, cell);
    }
}
