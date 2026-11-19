use std::f32::consts::TAU;

pub const YEAR_DAYS: f64 = sim::calendar::YEAR_DAYS;

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn from_phase(phase: f32) -> Season {
        match (phase.rem_euclid(1.0) * 4.0) as u32 {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }
}

pub fn year_phase(time_days: f64) -> f32 {
    (time_days / YEAR_DAYS).rem_euclid(1.0) as f32
}

pub fn daylight(time_days: f64) -> f32 {
    let f = (time_days + 0.35).rem_euclid(1.0) as f32;
    smoothstep(-0.1, 0.35, (TAU * (f - 0.25)).sin())
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surroundings {
    pub water: f32,
    pub forest: f32,
    pub wetland: f32,
    pub work: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub year_phase: f32,
    pub daylight: f32,
    pub place: Surroundings,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeSettings {
    pub master: f32,
    pub music: f32,
    pub ambience: f32,
    pub effects: f32,
}

impl Default for VolumeSettings {
    fn default() -> Self {
        Self { master: 0.8, music: 0.6, ambience: 1.0, effects: 1.0 }
    }
}

impl VolumeSettings {
    pub fn clamped(self) -> Self {
        let c = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        Self { master: c(self.master), music: c(self.music), ambience: c(self.ambience), effects: c(self.effects) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerVolumes {
    pub water: f32,
    pub wind: f32,
    pub winter_wind: f32,
    pub birds: f32,
    pub frogs: f32,
    pub gnaw: f32,
    pub splash: f32,
    pub thunk: f32,
    pub ice: f32,
    pub music: f32,
    pub warmth: f32,
    pub season: Season,
}

impl LayerVolumes {
    pub const SILENT: LayerVolumes = LayerVolumes {
        water: 0.0,
        wind: 0.0,
        winter_wind: 0.0,
        birds: 0.0,
        frogs: 0.0,
        gnaw: 0.0,
        splash: 0.0,
        thunk: 0.0,
        ice: 0.0,
        music: 0.0,
        warmth: 1.0,
        season: Season::Summer,
    };
}

pub fn layer_volumes(s: &Snapshot) -> LayerVolumes {
    let c = |v: f32| v.clamp(0.0, 1.0);
    let p = s.place;
    let (water, forest, wetland, work) = (c(p.water), c(p.forest), c(p.wetland), c(p.work));
    let day = c(s.daylight);
    let night = 1.0 - day;
    let warm = warmth(s.year_phase);
    let chill = 1.0 - smoothstep(0.0, 0.4, warm);
    let frozen = chill * chill;
    let detail = 1.0 - 0.6 * c(s.zoom);
    let bed = 1.0 - 0.25 * c(s.zoom);

    let water_v = (0.06 + 0.94 * water) * (1.0 - 0.85 * frozen) * bed;
    let wind = (0.22 + 0.5 * forest + 0.1 * (1.0 - water)) * (1.0 - chill) * bed;
    let winter_wind = chill * (0.45 + 0.4 * (1.0 - forest)) * bed;
    let bird_season = smoothstep(0.35, 0.8, warm);
    let birds = bird_season * day * (0.15 + 0.55 * forest + 0.25 * wetland + 0.15 * water) * detail;
    let frog_season = smoothstep(0.5, 0.9, warm);
    let frogs = frog_season * wetland * (0.45 + 0.55 * night) * detail;
    let gnaw = work * (0.6 + 0.4 * night) * detail;
    let thunk = work * (0.5 + 0.5 * day) * detail;
    let splash = water * (1.0 - frozen) * (0.3 + 0.3 * work) * detail;
    let ice = frozen * (0.1 + 0.9 * water) * detail;
    let music = (0.75 - 0.25 * work) * (1.0 - 0.2 * chill);
    LayerVolumes {
        water: c(water_v),
        wind: c(wind),
        winter_wind: c(winter_wind),
        birds: c(birds),
        frogs: c(frogs),
        gnaw: c(gnaw),
        splash: c(splash),
        thunk: c(thunk),
        ice: c(ice),
        music: c(music),
        warmth: warm,
        season: Season::from_phase(s.year_phase),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    pub water_depth: f32,
    pub vegetation: f32,
}

pub trait AudioWorld {
    fn time_days(&self) -> f64;
    fn cell(&self, x: f32, z: f32) -> Option<Cell>;
    fn work_sites(&self) -> &[(f32, f32)];
}

pub fn surroundings(world: &dyn AudioWorld, cx: f32, cz: f32, radius: f32) -> Surroundings {
    const N: i32 = 4;
    let radius = radius.max(1.0);
    let step = radius / N as f32;
    let (mut total, mut water, mut forest, mut wet) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for iz in -N..=N {
        for ix in -N..=N {
            let (x, z) = (cx + ix as f32 * step, cz + iz as f32 * step);
            let d = ((ix * ix + iz * iz) as f32).sqrt() / N as f32;
            if d > 1.0 {
                continue;
            }
            let Some(cell) = world.cell(x, z) else { continue };
            let w = 1.0 - 0.7 * d;
            total += w;
            if cell.water_depth > 0.05 {
                water += w;
                if cell.water_depth < 0.4 {
                    wet += w;
                }
            } else {
                forest += w * smoothstep(0.2, 0.7, cell.vegetation);
                let near_water = [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step)]
                    .iter()
                    .any(|(dx, dz)| world.cell(x + dx, z + dz).is_some_and(|c| c.water_depth > 0.05));
                if near_water && cell.vegetation > 0.15 {
                    wet += w;
                }
            }
        }
    }
    let work_reach = radius * 0.8 + 8.0;
    let work = world
        .work_sites()
        .iter()
        .map(|(x, z)| 1.0 - ((x - cx).powi(2) + (z - cz).powi(2)).sqrt() / work_reach)
        .fold(0.0f32, f32::max)
        .clamp(0.0, 1.0);
    if total <= 0.0 {
        return Surroundings { work, ..Default::default() };
    }
    Surroundings {
        water: (water / total * 2.5).min(1.0),
        forest: (forest / total * 1.6).min(1.0),
        wetland: (wet / total * 3.0).min(1.0),
        work,
    }
}

pub fn snapshot(world: &dyn AudioWorld, cam_x: f32, cam_z: f32, cam_distance: f32) -> Snapshot {
    let t = world.time_days();
    let radius = (cam_distance * 0.5).clamp(10.0, 60.0);
    Snapshot {
        year_phase: year_phase(t),
        daylight: daylight(t),
        place: surroundings(world, cam_x, cam_z, radius),
        zoom: ((cam_distance - 5.0) / 395.0).clamp(0.0, 1.0),
    }
}

#[allow(dead_code)]
pub struct StubWorld {
    pub days: f64,
    pub pond: Option<(f32, f32, f32)>,
    pub forest: Option<(f32, f32, f32)>,
    pub sites: Vec<(f32, f32)>,
}

impl AudioWorld for StubWorld {
    fn time_days(&self) -> f64 {
        self.days
    }
    fn cell(&self, x: f32, z: f32) -> Option<Cell> {
        let dist = |c: Option<(f32, f32, f32)>| c.map(|(cx, cz, r)| ((x - cx).hypot(z - cz), r));
        let mut cell = Cell { water_depth: 0.0, vegetation: 0.1 };
        if let Some((d, r)) = dist(self.pond) {
            if d < r - 4.0 {
                cell.water_depth = 0.8;
            } else if d < r {
                cell.water_depth = 0.2;
            } else if d < r + 5.0 {
                cell.vegetation = 0.6;
            }
        }
        if let Some((d, r)) = dist(self.forest) {
            if d < r && cell.water_depth == 0.0 {
                cell.vegetation = 0.9;
            }
        }
        Some(cell)
    }
    fn work_sites(&self) -> &[(f32, f32)] {
        &self.sites
    }
}

