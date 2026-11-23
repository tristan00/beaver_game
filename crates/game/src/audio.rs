mod mix;
mod synth;

use crate::camera::OrbitCamera;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::audio::{AddAudioSource, AudioSource, Decodable, Source};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use mix::{AudioWorld, Cell, VolumeSettings};
use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use synth::{Mix, SAMPLE_RATE, Synth};

#[derive(Resource, Clone, Copy, Debug)]
pub struct AudioVolumes(pub VolumeSettings);

impl Default for AudioVolumes {
    fn default() -> Self {
        Self(VolumeSettings::default())
    }
}

#[derive(Resource, Default, Clone, Debug)]
pub struct AudioWorkSites(pub Vec<(f32, f32)>);

struct SimWorld<'a> {
    sim: &'a sim::SimState,
    sites: &'a [(f32, f32)],
}

impl AudioWorld for SimWorld<'_> {
    fn time_days(&self) -> f64 {
        self.sim.time_days
    }
    fn cell(&self, x: f32, z: f32) -> Option<Cell> {
        let h = &self.sim.height;
        let gx = ((x + h.width() as f32 * CELL_SIZE / 2.0) / CELL_SIZE).floor();
        let gz = ((z + h.height() as f32 * CELL_SIZE / 2.0) / CELL_SIZE).floor();
        if gx < 0.0 || gz < 0.0 {
            return None;
        }
        let (gx, gz) = (gx as usize, gz as usize);
        h.get(gx, gz)?;
        Some(Cell {
            water_depth: self.sim.water.depth.get(gx, gz).copied().unwrap_or(0.0),
            vegetation: self.sim.ecology.vegetation.get(gx, gz).copied().unwrap_or(0.0),
        })
    }
    fn work_sites(&self) -> &[(f32, f32)] {
        self.sites
    }
}

#[derive(Resource, Clone)]
struct SharedMix(Arc<Mutex<Mix>>);

#[derive(Asset, TypePath)]
struct Soundscape {
    shared: Arc<Mutex<Mix>>,
    seed: u64,
}

struct SoundscapeStream {
    synth: Synth,
    shared: Arc<Mutex<Mix>>,
    buf: Vec<f32>,
    pos: usize,
}

impl Iterator for SoundscapeStream {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            let mix = *self.shared.lock().unwrap_or_else(|e| e.into_inner());
            self.synth.render(&mix, &mut self.buf);
            self.pos = 0;
        }
        self.pos += 1;
        Some(self.buf[self.pos - 1])
    }
}

impl Source for SoundscapeStream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> bevy::audio::ChannelCount {
        NonZero::new(2).expect("2 channels")
    }
    fn sample_rate(&self) -> bevy::audio::SampleRate {
        NonZero::new(SAMPLE_RATE).expect("sample rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for Soundscape {
    type Decoder = SoundscapeStream;
    fn decoder(&self) -> Self::Decoder {
        SoundscapeStream {
            synth: Synth::new(self.seed),
            shared: self.shared.clone(),
            buf: vec![0.0; 2048],
            pos: 2048,
        }
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some()
            || app.world().get_resource::<Assets<AudioSource>>().is_none()
        {
            return;
        }
        let shared = SharedMix(Arc::new(Mutex::new(Mix::default())));
        app.add_audio_source::<Soundscape>()
            .init_resource::<AudioVolumes>()
            .init_resource::<AudioWorkSites>()
            .insert_resource(shared)
            .add_systems(Startup, start_soundscape)
            .add_systems(Update, (volume_keys, update_mix).chain());
    }
}

fn start_soundscape(mut commands: Commands, mut assets: ResMut<Assets<Soundscape>>, shared: Res<SharedMix>, sim: Res<Sim>) {
    let handle = assets.add(Soundscape { shared: shared.0.clone(), seed: sim.0.seed });
    commands.spawn((AudioPlayer(handle), PlaybackSettings::ONCE));
}

fn volume_keys(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut vols: ResMut<AudioVolumes>, mut muted: Local<Option<f32>>) {
    let step = time.delta_secs() * 0.4;
    let adjust = |down: KeyCode, up: KeyCode, v: &mut f32| {
        if keys.pressed(down) {
            *v -= step;
        }
        if keys.pressed(up) {
            *v += step;
        }
        *v = v.clamp(0.0, 1.0);
    };
    let mut next = vols.0;
    adjust(KeyCode::Minus, KeyCode::Equal, &mut next.master);
    adjust(KeyCode::BracketLeft, KeyCode::BracketRight, &mut next.music);
    adjust(KeyCode::Semicolon, KeyCode::Quote, &mut next.effects);
    adjust(KeyCode::Comma, KeyCode::Period, &mut next.ambience);
    if keys.just_pressed(KeyCode::KeyM) {
        match muted.take() {
            Some(prev) => next.master = prev,
            None => {
                *muted = Some(next.master);
                next.master = 0.0;
            }
        }
    }
    if next != vols.0 {
        vols.0 = next;
    }
}

fn update_mix(
    time: Res<Time>,
    mut since: Local<f32>,
    sim: Res<Sim>,
    orbit: Res<OrbitCamera>,
    sites: Res<AudioWorkSites>,
    vols: Res<AudioVolumes>,
    shared: Res<SharedMix>,
    slaps: Option<Res<crate::entities_render::AlarmSlaps>>,
) {
    let _p = ::sim::prof::scope("game/update mix");
    *since += time.delta_secs();
    if *since < 0.1 && !vols.is_changed() && !slaps.as_ref().is_some_and(|s| s.is_changed()) {
        return;
    }
    *since = 0.0;
    let world = SimWorld { sim: &sim.0, sites: &sites.0 };
    let snap = mix::snapshot(&world, orbit.target.x, orbit.target.z, orbit.distance);
    let (alarm_slaps, alarm_amp) = slaps.map_or((0, 0.0), |s| (s.count, s.amp));
    let mix = Mix { layers: mix::layer_volumes(&snap), settings: vols.0, alarm_slaps, alarm_amp };
    *shared.0.lock().unwrap_or_else(|e| e.into_inner()) = mix;
}
