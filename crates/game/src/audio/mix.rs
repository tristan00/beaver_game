//! Mixing logic (pure std, no Bevy): turns season, time of day and what the camera is near
//! into per-layer volumes. Everything here is deterministic and unit-tested.
use std::f32::consts::TAU;

/// Length of one in-game year in sim days (the sim has no calendar yet; this is the audio's own).
pub const YEAR_DAYS: f64 = 96.0;

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
    /// `phase` is the year fraction in 0..1 (spring first).
    pub fn from_phase(phase: f32) -> Season {
        match (phase.rem_euclid(1.0) * 4.0) as u32 {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }
}

/// Year fraction (0..1) for a sim time in days.
pub fn year_phase(time_days: f64) -> f32 {
    (time_days / YEAR_DAYS).rem_euclid(1.0) as f32
}

/// 0 at night, 1 in full daylight. The game starts on a bright morning.
pub fn daylight(time_days: f64) -> f32 {
    let f = (time_days + 0.35).rem_euclid(1.0) as f32;
    smoothstep(-0.1, 0.35, (TAU * (f - 0.25)).sin())
}

/// How warm the year is: 1 mid-summer, 0 mid-winter, smooth in between.
pub fn warmth(phase: f32) -> f32 {
    const ANCHORS: [f32; 4] = [0.65, 1.0, 0.45, 0.0]; // mid spring, summer, autumn, winter
    let mut x = phase.rem_euclid(1.0) * 4.0 - 0.5;
    if x < 0.0 {
        x += 4.0;
    }
    let i = x.floor() as usize % 4;
    let f = x - x.floor();
    let s = f * f * (3.0 - 2.0 * f);
    ANCHORS[i] + (ANCHORS[(i + 1) % 4] - ANCHORS[i]) * s
}

/// How much of each kind of place is around the camera, each 0..=1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surroundings {
    pub water: f32,
    pub forest: f32,
    pub wetland: f32,
    pub work: f32,
}

/// Everything the mixer needs, produced by the adapter from sim and camera state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub year_phase: f32,
    pub daylight: f32,
    pub place: Surroundings,
    /// 0 camera close to the ground, 1 camera far away (detail sounds fade with distance).
    pub zoom: f32,
}

/// User volume controls, each 0..=1.
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

/// Target volume (0..=1) of every sound layer.
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

/// Season and place to layer volumes.
pub fn layer_volumes(s: &Snapshot) -> LayerVolumes {
    let c = |v: f32| v.clamp(0.0, 1.0);
    let p = s.place;
    let (water, forest, wetland, work) = (c(p.water), c(p.forest), c(p.wetland), c(p.work));
    let day = c(s.daylight);
    let night = 1.0 - day;
    let warm = warmth(s.year_phase);
    let chill = 1.0 - smoothstep(0.0, 0.4, warm);
    let frozen = chill * chill;
    // Detail sounds fade as the camera pulls away; the broad beds barely do.
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

// ---- Adapter: how the mixer reads the world ------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    pub water_depth: f32,
    pub vegetation: f32,
}

/// The small adapter the soundscape reads; implemented for the real sim and for stub data.
pub trait AudioWorld {
    fn time_days(&self) -> f64;
    /// Cell under world position (x, z); `None` off the map.
    fn cell(&self, x: f32, z: f32) -> Option<Cell>;
    /// World (x, z) of places where beavers are working (felling, building).
    fn work_sites(&self) -> &[(f32, f32)];
}

/// Samples the world around (cx, cz) within `radius` to see what the camera is near.
pub fn surroundings(world: &dyn AudioWorld, cx: f32, cz: f32, radius: f32) -> Surroundings {
    const N: i32 = 4; // samples per side of centre: a 9x9 grid
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

/// Builds the mixer input from a world and a camera.
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

#[allow(dead_code)] // used by tests and the audio_dump example
/// Stub data: a round pond with a marshy rim, a forest patch and optional work sites.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(phase: f32, day: f32, place: Surroundings) -> Snapshot {
        Snapshot { year_phase: phase, daylight: day, place, zoom: 0.0 }
    }
    const SUMMER: f32 = 0.375;
    const WINTER: f32 = 0.875;
    const POND: Surroundings = Surroundings { water: 0.9, forest: 0.3, wetland: 0.7, work: 0.0 };

    fn all(v: &LayerVolumes) -> [f32; 10] {
        [v.water, v.wind, v.winter_wind, v.birds, v.frogs, v.gnaw, v.splash, v.thunk, v.ice, v.music]
    }

    #[test]
    fn audio_volumes_stay_in_range() {
        for i in 0..40 {
            for d in [0.0, 0.5, 1.0] {
                for z in [0.0, 1.0] {
                    for p in [0.0, 0.5, 1.0] {
                        let place = Surroundings { water: p, forest: 1.0 - p, wetland: p, work: p };
                        let v = layer_volumes(&Snapshot { year_phase: i as f32 / 40.0, daylight: d, place, zoom: z });
                        assert!(all(&v).iter().all(|x| (0.0..=1.0).contains(x) && x.is_finite()));
                    }
                }
            }
        }
    }

    #[test]
    fn audio_summer_pond_is_alive() {
        let v = layer_volumes(&snap(SUMMER, 0.8, POND));
        assert!(v.water > 0.8, "water {}", v.water);
        assert!(v.birds > 0.2 && v.frogs > 0.3 && v.splash > 0.1);
        assert_eq!((v.ice, v.winter_wind), (0.0, 0.0));
        assert!(v.wind > 0.1);
    }

    #[test]
    fn audio_winter_is_quiet_and_icy() {
        let s = layer_volumes(&snap(SUMMER, 0.8, POND));
        let w = layer_volumes(&snap(WINTER, 0.8, POND));
        assert!(w.ice > 0.5 && w.winter_wind > 0.4);
        assert_eq!((w.birds, w.frogs, w.splash), (0.0, 0.0, 0.0));
        assert!(w.wind < 0.01);
        assert!(w.water < 0.3 * s.water, "frozen water muted: {} vs {}", w.water, s.water);
        assert!(w.music < s.music);
    }

    #[test]
    fn audio_seasons_blend_smoothly() {
        let mut last = layer_volumes(&snap(0.0, 0.8, POND));
        for i in 1..=200 {
            let v = layer_volumes(&snap(i as f32 / 200.0, 0.8, POND));
            for (a, b) in all(&last).iter().zip(all(&v)) {
                assert!((a - b).abs() < 0.08, "jump {a} -> {b} at step {i}");
            }
            last = v;
        }
        assert_eq!(Season::from_phase(0.1), Season::Spring);
        assert_eq!(Season::from_phase(0.3), Season::Summer);
        assert_eq!(Season::from_phase(0.6), Season::Autumn);
        assert_eq!(Season::from_phase(0.9), Season::Winter);
    }

    #[test]
    fn audio_place_drives_layers() {
        let base = Surroundings::default();
        let at = |p: Surroundings| layer_volumes(&snap(SUMMER, 0.8, p));
        let b = at(base);
        assert!(at(Surroundings { water: 1.0, ..base }).water > b.water + 0.5);
        assert!(at(Surroundings { water: 1.0, ..base }).splash > b.splash);
        assert!(at(Surroundings { forest: 1.0, ..base }).birds > b.birds + 0.3);
        assert!(at(Surroundings { forest: 1.0, ..base }).wind > b.wind);
        assert_eq!(b.frogs, 0.0);
        assert!(at(Surroundings { wetland: 1.0, ..base }).frogs > 0.3);
        assert_eq!((b.gnaw, b.thunk), (0.0, 0.0));
        let w = at(Surroundings { work: 1.0, ..base });
        assert!(w.gnaw > 0.5 && w.thunk > 0.5 && w.music < b.music);
    }

    #[test]
    fn audio_night_swaps_birds_for_frogs() {
        let day = layer_volumes(&snap(SUMMER, 1.0, POND));
        let night = layer_volumes(&snap(SUMMER, 0.0, POND));
        assert_eq!(night.birds, 0.0);
        assert!(night.frogs > day.frogs && night.gnaw >= day.gnaw);
    }

    #[test]
    fn audio_far_camera_fades_detail_not_beds() {
        let near = layer_volumes(&snap(SUMMER, 0.8, POND));
        let far = layer_volumes(&Snapshot { zoom: 1.0, ..snap(SUMMER, 0.8, POND) });
        assert!(far.birds < 0.5 * near.birds && far.frogs < 0.5 * near.frogs);
        assert!(far.water > 0.7 * near.water);
    }

    #[test]
    fn audio_time_helpers() {
        assert_eq!(year_phase(0.0), 0.0);
        assert!((year_phase(YEAR_DAYS * 2.25) - 0.25).abs() < 1e-5);
        assert!(daylight(0.0) > 0.9, "game starts in the morning");
        assert!(daylight(0.5) < 0.1, "night half a day later");
        assert!(warmth(0.375) > 0.99 && warmth(0.875) < 0.01);
    }

    #[test]
    fn audio_adapter_reads_stub_world() {
        let mut w = StubWorld {
            days: YEAR_DAYS * 0.375,
            pond: Some((0.0, 0.0, 14.0)),
            forest: Some((40.0, 0.0, 12.0)),
            sites: vec![(60.0, 0.0)],
        };
        let at_pond = snapshot(&w, 0.0, 0.0, 30.0);
        assert!(at_pond.place.water > 0.7 && at_pond.place.wetland > 0.3 && at_pond.place.work == 0.0);
        let in_forest = snapshot(&w, 40.0, 0.0, 30.0);
        assert!(in_forest.place.forest > 0.7 && in_forest.place.water == 0.0);
        let at_work = snapshot(&w, 60.0, 0.0, 30.0);
        assert!(at_work.place.work > 0.9);
        assert!(layer_volumes(&at_work).thunk > 0.5);
        w.days = YEAR_DAYS * 0.875;
        assert!(layer_volumes(&snapshot(&w, 0.0, 0.0, 30.0)).ice > 0.5);
        let empty = StubWorld { days: 0.0, pond: None, forest: None, sites: vec![] };
        assert_eq!(snapshot(&empty, 0.0, 0.0, 50.0).place.water, 0.0);
    }

    #[test]
    fn audio_volume_settings_clamp() {
        let v = VolumeSettings { master: 2.0, music: -1.0, ambience: f32::NAN, effects: 0.5 }.clamped();
        assert_eq!((v.master, v.music, v.ambience, v.effects), (1.0, 0.0, 0.0, 0.5));
    }
}
