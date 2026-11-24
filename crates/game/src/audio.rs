mod mix;
mod player;

use crate::camera::OrbitCamera;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::audio::{AddAudioSource, AudioSource, Decodable, Source};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use mix::{AudioWorld, Cell, VolumeSettings};
use player::{Bank, Mix, Player, SAMPLE_RATE, Sound};
use std::num::NonZero;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// Field recordings, all CC0 from freesound.org (see `audio/SOURCES.md`).
const BROOK: &[u8] = include_bytes!("../audio/brook.ogg");
const RIVER: &[u8] = include_bytes!("../audio/river.ogg");
const SLAPS: [&[u8]; 3] = [include_bytes!("../audio/slap_1.ogg"), include_bytes!("../audio/slap_2.ogg"), include_bytes!("../audio/slap_3.ogg")];
const FLOOD: &[u8] = include_bytes!("../audio/flood.ogg");

#[derive(Resource, Clone, Copy, Debug)]
pub struct AudioVolumes(pub VolumeSettings);

impl Default for AudioVolumes {
    fn default() -> Self {
        Self(VolumeSettings::default())
    }
}

struct SimWorld<'a> {
    sim: &'a sim::SimState,
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
        Some(Cell { water_depth: self.sim.water.depth.get(gx, gz).copied().unwrap_or(0.0) })
    }
}

fn decode(bytes: &'static [u8], make: fn(&[f32], usize, u32) -> Option<Sound>) -> Option<Sound> {
    let decoder = AudioSource { bytes: Arc::from(bytes) }.decoder();
    let (channels, rate) = (decoder.channels().get() as usize, decoder.sample_rate().get());
    let samples: Vec<f32> = decoder.collect();
    make(&samples, channels, rate)
}

fn load_bank() -> Option<Bank> {
    Some(Bank {
        brook: decode(BROOK, player::make_loop)?,
        river: decode(RIVER, player::make_loop)?,
        slaps: SLAPS.iter().filter_map(|b| decode(b, player::make_clip)).collect(),
        flood: decode(FLOOD, player::make_clip)?,
    })
}

#[derive(Resource, Clone)]
struct SharedMix(Arc<Mutex<Mix>>);

#[derive(Asset, TypePath)]
struct Soundscape {
    shared: Arc<Mutex<Mix>>,
    bank: Arc<OnceLock<Arc<Bank>>>,
}

struct SoundscapeStream {
    player: Player,
    shared: Arc<Mutex<Mix>>,
    bank: Arc<OnceLock<Arc<Bank>>>,
    buf: Vec<f32>,
    pos: usize,
}

impl Iterator for SoundscapeStream {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            if !self.player.has_bank()
                && let Some(bank) = self.bank.get()
            {
                self.player.set_bank(bank.clone());
            }
            let mix = *self.shared.lock().unwrap_or_else(|e| e.into_inner());
            self.player.render(&mix, &mut self.buf);
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
        SoundscapeStream { player: Player::new(), shared: self.shared.clone(), bank: self.bank.clone(), buf: vec![0.0; 2048], pos: 2048 }
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
            .insert_resource(shared)
            .add_systems(Startup, start_soundscape)
            .add_systems(Update, (volume_keys, update_mix).chain());
    }
}

fn start_soundscape(mut commands: Commands, mut assets: ResMut<Assets<Soundscape>>, shared: Res<SharedMix>) {
    let bank = Arc::new(OnceLock::new());
    let slot = bank.clone();
    std::thread::spawn(move || {
        if let Some(b) = load_bank() {
            let _ = slot.set(Arc::new(b));
        }
    });
    let handle = assets.add(Soundscape { shared: shared.0.clone(), bank });
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
    vols: Res<AudioVolumes>,
    shared: Res<SharedMix>,
    slaps: Option<Res<crate::entities_render::AlarmSlaps>>,
    floods: Option<Res<crate::hud::Floods>>,
) {
    let _p = ::sim::prof::scope("game/update mix");
    *since += time.delta_secs();
    let cue = slaps.as_ref().is_some_and(|s| s.is_changed()) || floods.as_ref().is_some_and(|f| f.is_changed());
    if *since < 0.1 && !vols.is_changed() && !cue {
        return;
    }
    *since = 0.0;
    let snap = mix::snapshot(&SimWorld { sim: &sim.0 }, orbit.target.x, orbit.target.z, orbit.distance);
    let (alarm_slaps, alarm_amp) = slaps.map_or((0, 0.0), |s| (s.count, s.amp));
    let mix = Mix { layers: mix::layer_volumes(&snap), settings: vols.0, alarm_slaps, alarm_amp, floods: floods.map_or(0, |f| f.0) };
    *shared.0.lock().unwrap_or_else(|e| e.into_inner()) = mix;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn levels(buf: &[f32]) -> (f32, f32) {
        let rms = (buf.iter().map(|s| (s * s) as f64).sum::<f64>() / buf.len() as f64).sqrt() as f32;
        (rms, buf.iter().fold(0.0f32, |m, s| m.max(s.abs())))
    }

    fn render(player: &mut Player, mix: &Mix, secs: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..secs * SAMPLE_RATE as usize / 1024 {
            let mut buf = vec![0.0f32; 2048];
            player.render(mix, &mut buf);
            out.extend(buf);
        }
        out
    }

    #[test]
    fn beds_loop_without_a_seam() {
        let bank = load_bank().expect("every recording decodes");
        assert_eq!(bank.slaps.len(), SLAPS.len());
        for bed in [&bank.brook, &bank.river] {
            assert!(bed.len() > SAMPLE_RATE as usize * 10);
            assert!(bed.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
            let step = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs().max((a[1] - b[1]).abs());
            let largest = bed.windows(2).map(|w| step(w[0], w[1])).fold(0.0f32, f32::max);
            assert!(step(bed[bed.len() - 1], bed[0]) <= largest);
        }
    }

    #[test]
    fn resampling_does_not_fold_high_pitches_down() {
        // A 30 kHz tone at 96 kHz is above what 44.1 kHz can hold; it must vanish, not alias.
        let rate = 96_000u32;
        let tone = |hz: f32| -> Vec<f32> {
            (0..rate as usize * 5).flat_map(|i| [(TAU * (hz as f64 * i as f64 / rate as f64).fract() as f32).sin(); 2]).collect()
        };
        let rms = |l: &Sound| (l.iter().map(|f| (f[0] * f[0]) as f64).sum::<f64>() / l.len() as f64).sqrt();
        let mut ultrasonic = tone(30_000.0);
        let low = tone(1_000.0);
        ultrasonic.iter_mut().zip(&low).for_each(|(u, l)| *u += 0.01 * l);
        let mixed = player::make_loop(&ultrasonic, 2, rate).unwrap();
        let clean = player::make_loop(&low, 2, rate).unwrap();
        let diff = mixed.iter().zip(clean.iter()).map(|(a, b)| ((a[0] - b[0]) as f64).powi(2)).sum::<f64>() / mixed.len() as f64;
        assert!(diff.sqrt() < 0.05 * rms(&clean), "ultrasonic leaked: {:.4} vs {:.4}", diff.sqrt(), rms(&clean));
    }

    #[test]
    fn cues_stand_out_over_the_water_without_clipping() {
        let bank = Arc::new(load_bank().unwrap());
        let water = mix::LayerVolumes { brook: 0.8, river: 0.7 };
        let mut player = Player::new();
        player.set_bank(bank);
        let mut mix = Mix { layers: water, ..Mix::default() };
        let (bed, _) = levels(&render(&mut player, &mix, 8)[SAMPLE_RATE as usize * 8..]);
        assert!((0.03..0.1).contains(&bed), "bed rms {bed}");
        // Nothing sounds for what had already happened before the player started.
        let mut late = Player::new();
        late.set_bank(Arc::new(load_bank().unwrap()));
        let quiet = Mix { alarm_slaps: 5, alarm_amp: 1.2, floods: 3, ..Mix::default() };
        assert_eq!(levels(&render(&mut late, &quiet, 2)).1, 0.0);
        // The flood is heard over the loudest twentieth of a second; the slap, a crack, by its peak.
        let loudest = |buf: &[f32]| buf.chunks(SAMPLE_RATE as usize / 10).map(|c| levels(c).0).fold(0.0f32, f32::max);
        let water_alone = render(&mut player, &mix, 4);
        mix.alarm_slaps = 1;
        mix.alarm_amp = 1.2;
        let slap = render(&mut player, &mix, 2);
        mix.floods = 1;
        let flood = render(&mut player, &mix, 4);
        let (slap_peak, flood_peak, water_peak) = (levels(&slap).1, levels(&flood).1, levels(&water_alone).1);
        assert!(slap_peak > 1.8 * water_peak, "slap {slap_peak} water {water_peak}");
        assert!(loudest(&flood) > 2.0 * loudest(&water_alone), "flood {} water {}", loudest(&flood), loudest(&water_alone));
        assert!(slap_peak < 0.95 && flood_peak < 0.95, "slap {slap_peak} flood {flood_peak}");
    }
}
