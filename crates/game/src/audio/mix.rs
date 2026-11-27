pub const YEAR_DAYS: f64 = sim::calendar::YEAR_DAYS;

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn year_phase(time_days: f64) -> f32 {
    (time_days / YEAR_DAYS).rem_euclid(1.0) as f32
}

pub fn warmth(phase: f32) -> f32 {
    const ANCHORS: [f32; 4] = [0.65, 1.0, 0.45, 0.0];
    let mut x = phase.rem_euclid(1.0) * 4.0 - 0.5;
    if x < 0.0 {
        x += 4.0;
    }
    let i = x.floor() as usize % 4;
    let f = x - x.floor();
    let s = f * f * (3.0 - 2.0 * f);
    ANCHORS[i] + (ANCHORS[(i + 1) % 4] - ANCHORS[i]) * s
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub year_phase: f32,
    pub water: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeSettings {
    pub master: f32,
    pub ambience: f32,
    pub effects: f32,
}

impl Default for VolumeSettings {
    fn default() -> Self {
        Self { master: 0.8, ambience: 1.0, effects: 1.0 }
    }
}

impl VolumeSettings {
    pub fn clamped(self) -> Self {
        let c = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        Self { master: c(self.master), ambience: c(self.ambience), effects: c(self.effects) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerVolumes {
    pub brook: f32,
    pub river: f32,
}

impl LayerVolumes {
    pub const SILENT: LayerVolumes = LayerVolumes { brook: 0.0, river: 0.0 };
}

pub fn layer_volumes(s: &Snapshot) -> LayerVolumes {
    let c = |v: f32| v.clamp(0.0, 1.0);
    let chill = 1.0 - smoothstep(0.0, 0.4, warmth(s.year_phase));
    let frozen = chill * chill;
    let bed = 1.0 - 0.25 * c(s.zoom);
    LayerVolumes {
        brook: c(0.8 * (1.0 - 0.4 * frozen) * bed),
        river: c(0.7 * smoothstep(0.05, 0.8, c(s.water)) * (1.0 - 0.8 * frozen) * bed),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    pub water_depth: f32,
}

pub trait AudioWorld {
    fn time_days(&self) -> f64;
    fn cell(&self, x: f32, z: f32) -> Option<Cell>;
}

pub fn water_around(world: &dyn AudioWorld, cx: f32, cz: f32, radius: f32) -> f32 {
    const N: i32 = 4;
    let step = radius.max(1.0) / N as f32;
    let (mut total, mut water) = (0.0f32, 0.0f32);
    for iz in -N..=N {
        for ix in -N..=N {
            let d = ((ix * ix + iz * iz) as f32).sqrt() / N as f32;
            if d > 1.0 {
                continue;
            }
            let Some(cell) = world.cell(cx + ix as f32 * step, cz + iz as f32 * step) else { continue };
            let w = 1.0 - 0.7 * d;
            total += w;
            if cell.water_depth > 0.05 {
                water += w;
            }
        }
    }
    if total <= 0.0 { 0.0 } else { (water / total * 2.5).min(1.0) }
}

pub fn snapshot(world: &dyn AudioWorld, cam_x: f32, cam_z: f32, cam_distance: f32) -> Snapshot {
    let radius = (cam_distance * 0.5).clamp(10.0, 60.0);
    Snapshot {
        year_phase: year_phase(world.time_days()),
        water: water_around(world, cam_x, cam_z, radius),
        zoom: ((cam_distance - 5.0) / 395.0).clamp(0.0, 1.0),
    }
}

