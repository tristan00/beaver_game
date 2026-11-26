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

// Field recordings, all CC0 from freesound.org (see `audio/SOURCES.md`), stored at the rate
// the game plays so starting them is only decoding.
static BROOK: &[u8] = include_bytes!("../audio/brook.ogg");
static RIVER: &[u8] = include_bytes!("../audio/river.ogg");
static SLAPS: [&[u8]; 3] = [include_bytes!("../audio/slap_1.ogg"), include_bytes!("../audio/slap_2.ogg"), include_bytes!("../audio/slap_3.ogg")];
static FLOOD: &[u8] = include_bytes!("../audio/flood.ogg");
static GROWL: &[u8] = include_bytes!("../audio/growl.ogg");
static HOWL: &[u8] = include_bytes!("../audio/howl.ogg");

/// The player's volumes, kept between games in a small file beside the saves.
#[derive(Resource, Clone, Copy, Debug)]
pub struct AudioVolumes(VolumeSettings);

/// What a volume control turns up or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Master,
    Effects,
    Ambience,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Master, Channel::Effects, Channel::Ambience];
    fn key(self) -> &'static str {
        match self {
            Channel::Master => "master",
            Channel::Effects => "effects",
            Channel::Ambience => "ambience",
        }
    }
    fn parse_key(key: &str) -> Option<Self> {
        match key {
            "master" => Some(Channel::Master),
            "effects" => Some(Channel::Effects),
            "ambience" => Some(Channel::Ambience),
            _ => None,
        }
    }
}

impl AudioVolumes {
    pub fn level(&self, ch: Channel) -> f32 {
        match ch {
            Channel::Master => self.0.master,
            Channel::Ambience => self.0.ambience,
            Channel::Effects => self.0.effects,
        }
    }
    pub fn set(&mut self, ch: Channel, v: f32) {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { return };
        match ch {
            Channel::Master => self.0.master = v,
            Channel::Ambience => self.0.ambience = v,
            Channel::Effects => self.0.effects = v,
        }
    }
    fn to_text(self) -> String {
        Channel::ALL.iter().map(|&c| format!("{} {:.2}\n", c.key(), self.level(c))).collect()
    }
    fn from_text(text: &str) -> Self {
        let mut v = Self(VolumeSettings::default());
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let (Some(key), Some(Ok(level))) = (words.next(), words.next().map(str::parse::<f32>)) else { continue };
            if let Some(c) = Channel::parse_key(key) {
                v.set(c, level);
            }
        }
        v
    }
}

const SOUND_FILE: &str = "sound.txt";

fn sound_path() -> std::path::PathBuf {
    crate::savegame::save_dir().join(SOUND_FILE)
}

/// Writes the volumes once they have stopped changing for half a second.
fn keep_volumes(time: Res<Time>, vols: Res<AudioVolumes>, mut waiting: Local<Option<f32>>) {
    if vols.is_changed() && !vols.is_added() {
        *waiting = Some(0.0);
    }
    let Some(t) = waiting.as_mut() else { return };
    *t += time.delta_secs();
    if *t >= 0.5 {
        *waiting = None;
        let _ = std::fs::create_dir_all(crate::savegame::save_dir());
        let _ = std::fs::write(sound_path(), vols.to_text());
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
        growl: decode(GROWL, player::make_clip)?,
        howl: decode(HOWL, player::make_clip)?,
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
        let kept = std::fs::read_to_string(sound_path()).map(|t| AudioVolumes::from_text(&t)).unwrap_or(AudioVolumes(VolumeSettings::default()));
        app.insert_resource(kept).add_systems(Update, keep_volumes);
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some()
            || app.world().get_resource::<Assets<AudioSource>>().is_none()
        {
            return;
        }
        let shared = SharedMix(Arc::new(Mutex::new(Mix::default())));
        app.add_audio_source::<Soundscape>()
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

fn volume_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    menu: Option<Res<crate::hud::MenuOpen>>,
    mut vols: ResMut<AudioVolumes>,
    mut muted: Local<Option<f32>>,
) {
    if menu.is_some_and(|m| m.0) {
        return;
    }
    if muted.is_some() && vols.0.master > 0.0 {
        // Turned back up some other way.
        *muted = None;
    }
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

/// Game days between two wolf sounds (a sighting's growl, the lookouts' tail slaps).
const WOLF_SOUND_DAYS: f64 = 30.0;

/// The wolf cues seen so far (sightings, slaps), those let through to be heard, and the game
/// day the last was heard.
type WolfSound = (((u32, u32), (u32, u32)), Option<f64>);

fn update_mix(
    time: Res<Time>,
    mut since: Local<f32>,
    sim: Res<Sim>,
    orbit: Res<OrbitCamera>,
    vols: Res<AudioVolumes>,
    shared: Res<SharedMix>,
    slaps: Option<Res<crate::entities_render::AlarmSlaps>>,
    floods: Option<Res<crate::hud::Floods>>,
    wolves: Option<Res<crate::hud::WolfCues>>,
    mut wolf_sound: Local<WolfSound>,
) {
    let _p = ::sim::prof::scope("game/update mix");
    *since += time.delta_secs();
    let cue = slaps.as_ref().is_some_and(|s| s.is_changed()) || floods.as_ref().is_some_and(|f| f.is_changed()) || wolves.as_ref().is_some_and(|w| w.is_changed());
    if *since < 0.1 && !vols.is_changed() && !cue {
        return;
    }
    *since = 0.0;
    let snap = mix::snapshot(&SimWorld { sim: &sim.0 }, orbit.target.x, orbit.target.z, orbit.distance);
    let (slapped, alarm_amp) = slaps.map_or((0, 0.0), |s| (s.count, s.amp));
    let sighted = wolves.as_ref().map_or(0, |w| w.sighted);
    // A wolf sighted (the growl) and the lookouts' alarm (tail slaps) sound at most once in
    // `WOLF_SOUND_DAYS` of game time; the slaps still show, unheard.
    let (heard, last) = &mut *wolf_sound;
    let (seen_before, slaps_before) = heard.0;
    if sighted != seen_before || slapped != slaps_before {
        let now = sim.0.time_days;
        let quiet = last.is_none_or(|t| now < t || now - t >= WOLF_SOUND_DAYS);
        if quiet {
            heard.1 = (heard.1 .0.wrapping_add((sighted != seen_before) as u32), heard.1 .1.wrapping_add((slapped != slaps_before) as u32));
            *last = Some(now);
        }
        heard.0 = (sighted, slapped);
    }
    let mix = Mix { layers: mix::layer_volumes(&snap), settings: vols.0, alarm_slaps: heard.1 .1, alarm_amp, floods: floods.map_or(0, |f| f.0), wolf_sighted: heard.1 .0, wolf_kills: wolves.as_ref().map_or(0, |w| w.kills) };
    *shared.0.lock().unwrap_or_else(|e| e.into_inner()) = mix;
}
