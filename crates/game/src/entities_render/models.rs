use super::meshgen::{hex, mix, Mb, Rng};
use super::{mesh_assets, structures};
use super::scene::{Pond, Species, StructKind};
use bevy::prelude::*;
use std::f32::consts::TAU;

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

pub const HEAD_PIVOT: Vec3 = Vec3::new(0.0, 0.85, 0.62);
pub const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 0.36, -0.6);
pub const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.3, 0.34, 0.4), Vec3::new(0.3, 0.34, 0.4), Vec3::new(-0.33, 0.36, -0.28), Vec3::new(0.33, 0.36, -0.28)];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Season(u8);

impl Season {
    // Shared meshes sampled twice per month; individual trees stagger their changes.
    pub const COUNT: usize = 24;
    pub fn all() -> impl Iterator<Item = Self> { (0..Self::COUNT as u8).map(Self) }
    pub fn at(phase: f32) -> Self { Self(((phase.rem_euclid(1.0) * Self::COUNT as f32).round() as usize % Self::COUNT) as u8) }
    pub fn index(self) -> usize { self.0 as usize }
}

pub use mesh_assets::load as asset;

pub fn tree_mesh(sp: Species, season: Season, lod: u8) -> Mesh {
    asset(&format!("tree-{sp:?}-{}-{lod}", season.index()))
}
pub fn tree_cutaway_mesh(sp: Species) -> Mesh { asset(&format!("cutaway-{sp:?}")) }
pub fn load_mesh(good: sim::buildings::Good) -> Mesh { asset(&format!("load-{good:?}")) }
pub fn felled_mesh(logs: u8, branches: bool) -> Mesh { asset(&format!("felled-{logs}-{branches}")) }
pub fn reinforcement_mesh(kind: StructKind) -> Mesh { asset(&format!("reinforcement-{kind:?}")) }

pub const DRY_STAGE: u8 = 4;

pub fn has_footing(kind: StructKind) -> bool {
    use StructKind::*;
    matches!(kind, Lodge | Burrow | FoodCache | ScentMound | MudPit | ForagerHut |
        LoggingCamp | Workshop | Warehouse | Storehouse | Mill)
}

pub fn struct_mesh(kind: StructKind, len: f32, stage: u8) -> Mesh {
    let mut mb = Mb::new();
    let progress = stage.min(3);
    match kind {
        StructKind::Dam => dam(&mut mb, len, progress),
        StructKind::Canal => canal(&mut mb, len, progress),
        StructKind::Dock => dock(&mut mb, len, progress),
        StructKind::Barge => barge(&mut mb, len, progress, true),
        StructKind::BargeEmpty => barge(&mut mb, len, 3, false),
        _ => {
            let stage = if stage == DRY_STAGE { stage } else { progress };
            return asset(&format!("structure-{kind:?}-{stage}"));
        }
    }
    mb.build()
}

pub fn pond_surface(p: &Pond) -> Mesh {
    let rim: Vec<Vec3> = (0..96)
        .map(|i| {
            let a = i as f32 / 96.0 * TAU;
            let r = p.radius(a) + 0.9;
            Vec3::new(a.sin() * r, 0.0, a.cos() * r)
        })
        .collect();
    let mut mb = Mb::new();
    mb.fan(Vec3::ZERO, &rim, [1.0; 3]);
    mb.build()
}

pub fn stage_of(progress: f32) -> u8 {
    if progress >= 1.0 { 3 } else { ((progress.max(0.0) * 3.0) as u8).min(2) }
}

pub(super) fn stake(mb: &mut Mb, p: Vec3, h: f32, flag: bool) {
    mb.cyl(p + v(0.0, -0.4, 0.0), p + v(0.0, h, 0.0), 0.06, 0.05, hex(0xf3e7c9), hex(0xf3e7c9), 5);
    mb.ellipsoid(p + v(0.0, h + 0.02, 0.0), v(0.08, 0.06, 0.08), hex(0xf3e7c9));
    if flag {
        mb.cuboid(p + v(0.22, h - 0.12, 0.0), v(0.2, 0.12, 0.015), Quat::from_rotation_z(-0.08), hex(0xff7a1a), hex(0xffa040));
    }
}

pub(super) fn stick(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, k: f32) {
    let bark = mix(hex(0x8b5f33), hex(0xb98a52), k);
    mb.log(a, b, r, bark, hex(0xe9cf9b), 5);
}

pub(super) fn stick_c(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, col: [f32; 3]) {
    mb.log(a, b, r, col, hex(0xe9cf9b), 5);
}

pub(super) fn stick_pile(mb: &mut Mb, c: Vec3, yaw: f32, n: usize, seed: u32) {
    let mut r = Rng(seed);
    for i in 0..n {
        let layer = (i / 3) as f32;
        let a = yaw + r.range(-0.35, 0.35);
        let off = v(r.range(-0.3, 0.3), 0.1 + layer * 0.18, r.range(-0.25, 0.25));
        let (d, l) = (v(a.cos(), r.range(-0.05, 0.12), -a.sin()), r.range(0.9, 1.4));
        stick(mb, c + off - d * l * 0.5, c + off + d * l * 0.5, 0.075, r.next());
    }
}

pub(super) fn mud_lump(mb: &mut Mb, c: Vec3, s: f32) {
    mb.ellipsoid_g(c, v(0.55 * s, 0.26 * s, 0.45 * s), hex(0x4d3520), hex(0x77573a), 8, 5);
}

fn dam(mb: &mut Mb, len: f32, stage: u8) {
    if stage == 3 {
        structures::dam_cell(mb, 1.4, len, 0, false);
    } else {
        structures::dam_cell(mb, [0.25, 0.6, 1.0][stage as usize], len, 1, true);
    }
}

fn canal(mb: &mut Mb, len: f32, stage: u8) {
    let hl = len * 0.5;
    let n = ((len / 1.0).ceil() as i32).max(2);
    for s in [-1.0f32, 1.0] {
        for i in 0..=n {
            let x = -hl + len * i as f32 / n as f32;
            mb.ellipsoid_g(v(x, 0.1, 1.35 * s), v(0.85, 0.5, 0.75), hex(0x5c4128), hex(0x7aa84a), 8, 5);
        }
    }
    if stage == 3 {
        mb.cuboid(v(0.0, 0.0, 0.0), v(hl + 0.05, 0.14, 1.0), Quat::IDENTITY, hex(0x2f7fb0), hex(0x58b8e6));
        for k in 0..3 {
            mb.ellipsoid(v(-hl * 0.6 + k as f32 * hl * 0.6, 0.15, 0.2 * (k as f32 - 1.0)), v(0.35, 0.02, 0.12), hex(0xd8f1ff));
        }
    } else {
        mb.cuboid(v(0.0, -0.05, 0.0), v(hl + 0.05, 0.1, 1.0), Quat::IDENTITY, hex(0x3d2a1a), hex(0x6a4a2e));
        mud_lump(mb, v(-hl * 0.4, 0.35, 2.4), 1.3);
        mb.cyl(v(0.7, 0.0, 2.0), v(1.0, 1.4, 2.1), 0.04, 0.04, hex(0xb98a52), hex(0xb98a52), 5);
        mb.cuboid(v(0.68, 0.1, 1.98), v(0.12, 0.17, 0.02), Quat::from_rotation_z(0.2), hex(0x9aa0a8), hex(0xb8bec6));
        stake(mb, v(hl - 0.3, 0.0, -1.9), 1.2, true);
        stake(mb, v(hl - 0.3, 0.0, 1.9), 1.2, true);
    }
}

fn dock(mb: &mut Mb, len: f32, stage: u8) {
    let (w, y) = (1.15f32, 0.5f32);
    let planks = (len / 0.5) as i32;
    let shown = if stage == 3 { planks } else { (planks * (stage as i32 + 1)) / 4 };
    for i in 0..shown {
        let z = 0.25 + i as f32 * 0.5;
        let (a, b) = if i % 2 == 0 { (hex(0xb98650), hex(0xd1a266)) } else { (hex(0xa97a46), hex(0xc4965c)) };
        mb.cuboid(v(0.0, y, z), v(w, 0.07, 0.23), Quat::IDENTITY, a, b);
    }
    for s in [-0.7f32, 0.7] {
        mb.cuboid(v(s, y - 0.13, len * 0.5), v(0.1, 0.07, len * 0.5), Quat::IDENTITY, hex(0x6a4528), hex(0x7d5430));
    }
    let mut z = 0.5;
    while z <= len + 0.01 {
        for s in [-1.0f32, 1.0] {
            mb.cyl(v((w + 0.1) * s, -1.3, z), v((w + 0.1) * s, y + 0.55, z), 0.13, 0.13, hex(0x5a3a22), hex(0x7d5430), 7);
            mb.ellipsoid(v((w + 0.1) * s, y + 0.56, z), v(0.14, 0.09, 0.14), hex(0x8f6038));
        }
        z += (len - 0.5).max(1.0);
    }
    if stage == 3 {
        mb.cyl(v(w + 0.1, y + 0.4, 0.5), v(w + 0.1, y + 0.4, len), 0.025, 0.025, hex(0xe9d9aa), hex(0xe9d9aa), 4);
        mb.cyl(v(-w - 0.1, y + 0.4, len), v(-w - 0.1, y + 0.9, len), 0.03, 0.03, hex(0x333333), hex(0x333333), 4);
        mb.ellipsoid(v(-w - 0.1, y + 1.05, len), v(0.13, 0.17, 0.13), hex(0xffe08a));
        mb.cuboid(v(-0.45, y + 0.35, len * 0.5), v(0.32, 0.28, 0.32), Quat::from_rotation_y(0.3), hex(0xa97a46), hex(0xd1a266));
        mb.lathe(Mat4::from_translation(v(0.55, y + 0.07, len * 0.38)), &[(0.0, 0.0), (0.28, 0.0), (0.34, 0.3), (0.3, 0.6), (0.0, 0.6)], 10, hex(0x7d5430), hex(0xa97a46), true);
    } else {
        stake(mb, v(w + 0.1, y, len), 1.1, true);
        stake(mb, v(-w - 0.1, y, len), 1.1, true);
        for i in 0..3 {
            mb.cuboid(v(-1.9, 0.12 + i as f32 * 0.09, 1.2), v(0.2, 0.045, 0.8), Quat::from_rotation_y(0.1 * i as f32), hex(0xcda56a), hex(0xe2bb7c));
        }
    }
}

fn barge(mb: &mut Mb, len: f32, stage: u8, cargo: bool) {
    let hl = len * 0.5;
    let top = if stage == 3 { 0.7 } else { 0.35 };
    mb.hex(
        [
            v(-0.95, -0.3, -hl), v(0.95, -0.3, -hl), v(0.5, -0.3, hl + 0.55), v(-0.5, -0.3, hl + 0.55),
            v(-1.3, top, -hl), v(1.3, top, -hl), v(0.78, top + 0.12, hl + 0.8), v(-0.78, top + 0.12, hl + 0.8),
        ],
        hex(0x6a4528),
        hex(0xb98650),
    );
    for s in [-1.0f32, 1.0] {
        mb.cyl(v(1.3 * s, top, -hl), v(0.78 * s, top + 0.12, hl + 0.8), 0.07, 0.07, hex(0x6a4528), hex(0x6a4528), 5);
    }
    mb.ellipsoid(v(0.0, 0.1, hl + 0.78), v(0.25, 0.22, 0.2), hex(0x2c2a28));
    if stage == 3 {
        for (row, cnt, y) in [(0, 3, 0.95), (1, 2, 1.32), (2, 1, 1.68)].into_iter().filter(|_| cargo) {
            for i in 0..cnt {
                let x = (i as f32 - (cnt as f32 - 1.0) * 0.5) * 0.62;
                let _ = row;
                mb.log(v(x, y, -hl + 0.7), v(x, y, hl - 0.4), 0.28, mix(hex(0x7a5230), hex(0x9a6e3e), 0.3 * i as f32), hex(0xecd2a0), 8);
            }
        }
        mb.cyl(v(0.0, 0.7, hl - 0.2), v(0.0, 2.6, hl - 0.2), 0.07, 0.05, hex(0x6a4528), hex(0x8f6038), 6);
        mb.cuboid(v(0.34, 2.35, hl - 0.2), v(0.34, 0.2, 0.02), Quat::from_rotation_z(-0.1), hex(0xd94a3a), hex(0xff7860));
    } else {
        for i in 0..5 {
            let z = -hl + 0.8 + i as f32 * (len - 1.5) / 4.0;
            mb.cyl(v(-1.1, top, z), v(-0.9, top + 1.2, z), 0.05, 0.04, hex(0xd7b27a), hex(0xd7b27a), 4);
            mb.cyl(v(1.1, top, z), v(0.9, top + 1.2, z), 0.05, 0.04, hex(0xd7b27a), hex(0xd7b27a), 4);
        }
    }
}
