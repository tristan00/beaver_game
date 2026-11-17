//! Shared colour constants from `docs/STYLE.md` §2 (terrain, growth, wood, water, light).
//! Everything is sRGB `[r, g, b]` built from the guide's hex values, so code names a colour
//! instead of copying digits. UI colours stay in `ui/kit.rs`.
pub type Rgb = [f32; 3];

/// sRGB triple from `0xRRGGBB`.
pub const fn rgb(h: u32) -> Rgb {
    [((h >> 16) & 0xff) as f32 / 255.0, ((h >> 8) & 0xff) as f32 / 255.0, (h & 0xff) as f32 / 255.0]
}

pub const MEADOW_LIGHT: Rgb = rgb(0xA8D878);
pub const MEADOW: Rgb = rgb(0x85C468);
pub const MEADOW_DEEP: Rgb = rgb(0x5FA85A);
pub const LEAF_HIGHLIGHT: Rgb = rgb(0x8CD070);
pub const STRAW: Rgb = rgb(0xD9C27A);
pub const MUD: Rgb = rgb(0xA07F60);
pub const STONE: Rgb = rgb(0xB9AFA8);
pub const STONE_SHADE: Rgb = rgb(0x8C8290);
pub const PLUM: Rgb = rgb(0x6B5A8A);
pub const FUR: Rgb = rgb(0xB9783F);
pub const FUR_DARK: Rgb = rgb(0x8F5A2E);
pub const CUT_WOOD: Rgb = rgb(0xF2D9A8);
pub const BARK: Rgb = rgb(0x7A4A2B);
pub const WATER_SHALLOW: Rgb = rgb(0x6FD3E0);
pub const WATER_MID_GLOW: Rgb = rgb(0x8FE0E6);
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
pub const AMBIENT: Rgb = rgb(0xCCE6FF);

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
