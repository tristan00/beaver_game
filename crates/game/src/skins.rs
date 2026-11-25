//! The HUD's few materials, drawn once at startup: carved plates, pebble
//! buttons, alert tokens and the season ring. Each is a small texture that the
//! UI stretches as a nine-slice, so corners keep their cut at any size.
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Skin {
    /// The panel every piece of the HUD sits on.
    Plate,
    /// A plate with a red edge: something is not allowed or is failing.
    PlateBad,
    /// A plate that marks the selection or the suggested thing.
    PlateAccent,
    /// A button at rest.
    Pebble,
    /// A button that is on.
    PebbleOn,
    /// The one main action: solid amber.
    PebblePrimary,
    /// A button whose action keeps something sound (repair, upkeep).
    PebbleGood,
    /// An alert that needs the player now.
    TokenBad,
    /// An alert that will need the player soon.
    TokenWarn,
    /// The dark frame behind a portrait.
    Portrait,
    /// The year as four coloured quarters, spring at the top.
    SeasonRing,
}

impl Skin {
    pub const ALL: [Skin; 11] = [
        Skin::Plate,
        Skin::PlateBad,
        Skin::PlateAccent,
        Skin::Pebble,
        Skin::PebbleOn,
        Skin::PebblePrimary,
        Skin::PebbleGood,
        Skin::TokenBad,
        Skin::TokenWarn,
        Skin::Portrait,
        Skin::SeasonRing,
    ];

    /// Pixels of the texture kept unstretched at each edge.
    pub fn slice(self) -> f32 {
        match self {
            Skin::Plate | Skin::PlateBad | Skin::PlateAccent => 12.0,
            Skin::Pebble | Skin::PebbleOn | Skin::PebblePrimary | Skin::PebbleGood => 11.0,
            Skin::TokenBad | Skin::TokenWarn => 12.0,
            Skin::Portrait => 9.0,
            Skin::SeasonRing => 0.0,
        }
    }
}

type Rgba = [f32; 4];

const PANEL: Rgba = [0.106, 0.09, 0.075, 0.95];
const PEB: Rgba = [0.227, 0.18, 0.145, 0.96];
const BARK: Rgba = [0.59, 0.41, 0.24, 1.0];
const ACCENT: Rgba = [0.91, 0.52, 0.23, 1.0];
const ACCENT_DARK: Rgba = [0.55, 0.28, 0.1, 1.0];
const GOOD: Rgba = [0.39, 0.79, 0.55, 1.0];
const BAD: Rgba = [0.94, 0.45, 0.38, 1.0];
const WARN: Rgba = [0.94, 0.75, 0.29, 1.0];
const PORTRAIT: Rgba = [0.165, 0.141, 0.125, 1.0];
const CARVE: Rgba = [1.0, 1.0, 1.0, 0.07];

/// Signed distance to a rectangle with its corners cut at 45 degrees.
fn chamfer(p: Vec2, size: Vec2, cut: f32) -> f32 {
    let q = (p - size * 0.5).abs();
    let h = size * 0.5;
    let d = q - h;
    let boxd = d.max(Vec2::ZERO).length() + d.x.max(d.y).min(0.0);
    let cutd = (q.x + q.y - (h.x + h.y - cut)) * FRAC_1_SQRT_2;
    boxd.max(cutd)
}

fn over(dst: Rgba, src: Rgba, a: f32) -> Rgba {
    let sa = src[3] * a.clamp(0.0, 1.0);
    let oa = sa + dst[3] * (1.0 - sa);
    if oa <= 1e-5 {
        return [0.0; 4];
    }
    let mix = |i: usize| (src[i] * sa + dst[i] * dst[3] * (1.0 - sa)) / oa;
    [mix(0), mix(1), mix(2), oa]
}

/// A cut-corner shape: a fill, an edge of `edge` pixels, and an optional carved line inside.
fn carved(size: u32, cut: f32, fill: Rgba, edge_col: Rgba, edge: f32, carve: bool) -> Vec<Rgba> {
    let s = Vec2::splat(size as f32);
    let mut out = vec![[0.0; 4]; (size * size) as usize];
    for y in 0..size {
        for x in 0..size {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let d = chamfer(p, s - Vec2::splat(1.0), cut);
            let inside = (0.5 - d).clamp(0.0, 1.0);
            if inside <= 0.0 {
                continue;
            }
            let mut c = [0.0, 0.0, 0.0, 0.0];
            c = over(c, edge_col, inside);
            c = over(c, fill, (0.5 - (d + edge)).clamp(0.0, 1.0));
            if carve {
                let line = (1.0 - (d + edge + 3.0).abs()).clamp(0.0, 1.0);
                c = over(c, CARVE, line);
                let shade = (1.0 - (d + edge + 0.5).abs()).clamp(0.0, 1.0);
                c = over(c, [0.0, 0.0, 0.0, 0.35], shade);
            }
            out[(y * size + x) as usize] = c;
        }
    }
    out
}

fn season_ring(size: u32) -> Vec<Rgba> {
    let cols: [Rgba; 4] = [[0.61, 0.8, 0.42, 1.0], [0.92, 0.78, 0.35, 1.0], [0.85, 0.54, 0.29, 1.0], [0.74, 0.84, 0.91, 1.0]];
    let c = Vec2::splat(size as f32 * 0.5);
    let (r_out, r_in) = (size as f32 * 0.5 - 1.0, size as f32 * 0.5 - 1.0 - size as f32 * 0.14);
    let mut out = vec![[0.0; 4]; (size * size) as usize];
    for y in 0..size {
        for x in 0..size {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - c;
            let r = p.length();
            let a = ((r_out - r).min(r - r_in) + 0.5).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            // Clockwise from the top, as the year runs.
            let ang = (p.x.atan2(-p.y) + TAU) % TAU;
            let q = ((ang / (PI * 0.5)) as usize).min(3);
            let gap = {
                let into = ang - q as f32 * PI * 0.5;
                ((into.min(PI * 0.5 - into)) * r - 0.8).clamp(0.0, 1.0)
            };
            let mut col = cols[q];
            col[3] = a * gap;
            out[(y * size + x) as usize] = col;
        }
    }
    out
}

fn texture(skin: Skin) -> (u32, Vec<Rgba>) {
    match skin {
        Skin::Plate => (40, carved(40, 8.0, PANEL, BARK, 2.0, true)),
        Skin::PlateBad => (40, carved(40, 8.0, PANEL, BAD, 2.0, true)),
        Skin::PlateAccent => (40, carved(40, 8.0, PANEL, ACCENT, 2.0, true)),
        Skin::Pebble => (32, carved(32, 9.0, PEB, BARK, 1.5, false)),
        Skin::PebbleOn => (32, carved(32, 9.0, PEB, ACCENT, 2.5, false)),
        Skin::PebblePrimary => (32, carved(32, 9.0, ACCENT, ACCENT_DARK, 1.5, false)),
        Skin::PebbleGood => (32, carved(32, 9.0, PEB, GOOD, 2.0, false)),
        Skin::TokenBad => (36, carved(36, 10.0, PANEL, BAD, 2.5, false)),
        Skin::TokenWarn => (36, carved(36, 10.0, PANEL, WARN, 2.5, false)),
        Skin::Portrait => (28, carved(28, 7.0, PORTRAIT, [1.0, 1.0, 1.0, 0.12], 1.0, false)),
        Skin::SeasonRing => (96, season_ring(96)),
    }
}

pub fn image(skin: Skin) -> Image {
    let (n, px) = texture(skin);
    let mut data = Vec::with_capacity(px.len() * 4);
    for c in px {
        // Straight alpha in sRGB bytes.
        data.extend(c.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
    }
    let mut img = Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plates_are_cut_at_the_corners_and_solid_inside() {
        let (n, px) = texture(Skin::Plate);
        assert!(px[0][3] < 0.01, "the corner is cut away");
        let mid = px[((n / 2) * n + n / 2) as usize];
        assert!(mid[3] > 0.9, "the middle is solid");
        let edge = px[((n / 2) * n + 1) as usize];
        assert!(edge[0] > 0.4, "the edge is bark");
    }
}
