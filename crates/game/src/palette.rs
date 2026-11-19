pub type Rgb = [f32; 3];

pub const fn rgb(h: u32) -> Rgb {
    [((h >> 16) & 0xff) as f32 / 255.0, ((h >> 8) & 0xff) as f32 / 255.0, (h & 0xff) as f32 / 255.0]
}

pub const MEADOW_LIGHT: Rgb = rgb(0xA8D878);
pub const MEADOW: Rgb = rgb(0x85C468);
pub const MEADOW_DEEP: Rgb = rgb(0x5FA85A);
pub const STRAW: Rgb = rgb(0xD9C27A);
pub const MUD: Rgb = rgb(0xA07F60);
pub const STONE: Rgb = rgb(0xB9AFA8);
pub const STONE_SHADE: Rgb = rgb(0x8C8290);
pub const ROCK_SLOPE_MIN: f32 = 0.9;
pub const ROCK_SLOPE_BLEND: f32 = 0.5;
pub const PLUM: Rgb = rgb(0x6B5A8A);
pub const FUR: Rgb = rgb(0xB9783F);
pub const FUR_DARK: Rgb = rgb(0x8F5A2E);
pub const FUR_LIGHT: Rgb = rgb(0xC98B52);
pub const INK: Rgb = rgb(0x4D3321);
pub const CUT_WOOD: Rgb = rgb(0xF2D9A8);
pub const WATER_SHALLOW: Rgb = rgb(0x6FD3E0);
pub const WATER_DEEP: Rgb = rgb(0x1F8AB8);
pub const FOAM: Rgb = rgb(0xF4FBF6);
pub const LILY: Rgb = rgb(0x4E9A52);
pub const FLOWER_WHITE: Rgb = rgb(0xFFFAEE);
pub const FLOWER_PINK: Rgb = rgb(0xF59AA6);
pub const FLOWER_YELLOW: Rgb = rgb(0xF2C94C);
pub const SKY: Rgb = rgb(0x9ED1F5);
pub const SKY_WINTER: Rgb = rgb(0xC8D4E8);
pub const SUN: Rgb = rgb(0xFFF5E0);
pub const SUN_AUTUMN: Rgb = rgb(0xFFDBB3);
pub const SHADOW_MIX: f32 = 0.40;

pub const AUTUMN_RUST: Rgb = rgb(0xD9814E);
pub const DAM_MUD: Rgb = rgb(0x8F6E52);

pub const fn pack(c: Rgb) -> u32 {
    (((c[0] * 255.0 + 0.5) as u32) << 16) | (((c[1] * 255.0 + 0.5) as u32) << 8) | ((c[2] * 255.0 + 0.5) as u32)
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn display_transform(c: [f32; 3]) -> [f32; 3] {
    let curve = |v: f32| 1.0 - (-v).exp();
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let cb = -0.1146 * c[0] - 0.3854 * c[1] + 0.5 * c[2];
    let cr = 0.5 * c[0] - 0.4542 * c[1] - 0.0458 * c[2];
    let bt = curve((cb * cb + cr * cr).sqrt() * 2.4);
    let desat = ((bt - 0.7) * 0.8).max(0.0).powi(2);
    let k = (curve(y) / y.max(1e-5)).max(0.0);
    let mut out = [0.0; 3];
    for i in 0..3 {
        let tm0 = c[i] * k;
        let tm1 = curve(c[i] + (y - c[i]) * desat);
        out[i] = (tm0 + (tm1 - tm0) * bt * bt) * 0.97;
    }
    out
}

pub fn undo_display_transform(d: [f32; 3]) -> [f32; 3] {
    let goal = d.map(|v| v.clamp(1e-4, 0.95));
    let mut x = goal;
    for _ in 0..8 {
        let shown = display_transform(x);
        for i in 0..3 {
            x[i] = x[i] * goal[i] / shown[i].max(1e-5);
        }
    }
    x
}

pub fn display_linear(c: Rgb) -> [f32; 3] {
    let ch = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    undo_display_transform([ch(c[0]), ch(c[1]), ch(c[2])])
}

