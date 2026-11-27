use super::mix::{LayerVolumes, VolumeSettings};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub layers: LayerVolumes,
    pub settings: VolumeSettings,
    pub alarm_slaps: u32,
    pub alarm_amp: f32,
    pub floods: u32,
    pub wolf_sighted: u32,
    pub wolf_kills: u32,
}

impl Default for Mix {
    fn default() -> Self {
        Self { layers: LayerVolumes::SILENT, settings: VolumeSettings::default(), alarm_slaps: 0, alarm_amp: 0.0, floods: 0, wolf_sighted: 0, wolf_kills: 0 }
    }
}

pub type Sound = Arc<[[f32; 2]]>;

pub struct Bank {
    pub brook: Sound,
    pub river: Sound,
    pub slaps: Vec<Sound>,
    pub flood: Sound,
    pub growl: Sound,
    pub howl: Sound,
}

pub fn resample(samples: &[f32], channels: usize, rate: u32) -> Option<Vec<[f32; 2]>> {
    if channels == 0 || rate == 0 {
        return None;
    }
    let src: Vec<[f32; 2]> =
        samples.chunks_exact(channels).map(|f| [f[0], if channels > 1 { f[1] } else { f[0] }]).collect();
    if src.len() < 2 {
        return None;
    }
    if rate == SAMPLE_RATE {
        return Some(src);
    }
    let n = (src.len() as f64 * SAMPLE_RATE as f64 / rate as f64) as usize;
    let step = rate as f64 / SAMPLE_RATE as f64;
    let cut = 0.95 * (1.0 / step).min(1.0);
    let half = (32.0 / cut).ceil() as i64;
    Some(
        (0..n)
            .map(|i| {
                let x = i as f64 * step;
                let mid = x.floor() as i64;
                let mut acc = [0.0f64; 2];
                for j in (mid - half + 1).max(0)..=(mid + half).min(src.len() as i64 - 1) {
                    let d = x - j as f64;
                    let w = d / half as f64;
                    if w.abs() >= 1.0 {
                        continue;
                    }
                    let t = PI as f64 * d * cut;
                    let sinc = if t.abs() < 1e-9 { 1.0 } else { t.sin() / t };
                    let p = PI as f64 * w;
                    let window = 0.35875 + 0.48829 * p.cos() + 0.14128 * (2.0 * p).cos() + 0.01168 * (3.0 * p).cos();
                    let k = cut * sinc * window;
                    acc[0] += src[j as usize][0] as f64 * k;
                    acc[1] += src[j as usize][1] as f64 * k;
                }
                [acc[0] as f32, acc[1] as f32]
            })
            .collect(),
    )
}

pub fn make_loop(samples: &[f32], channels: usize, rate: u32) -> Option<Sound> {
    let a = resample(samples, channels, rate)?;
    let n = a.len();
    if n < SAMPLE_RATE as usize * 4 {
        return None;
    }
    let fade = (SAMPLE_RATE as usize * 3).min(n / 4);
    let mut out: Vec<[f32; 2]> = a[..n - fade].to_vec();
    for (i, o) in out.iter_mut().take(fade).enumerate() {
        let x = (i as f32 + 0.5) / fade as f32 * PI / 2.0;
        let tail = a[n - fade + i];
        for c in 0..2 {
            o[c] = o[c] * x.sin() + tail[c] * x.cos();
        }
    }
    let rms = (out.iter().map(|f| (f[0] * f[0] + f[1] * f[1]) as f64).sum::<f64>() / (2 * out.len()) as f64).sqrt();
    if rms < 1e-6 {
        return None;
    }
    let g = (0.06 / rms) as f32;
    out.iter_mut().for_each(|f| *f = [f[0] * g, f[1] * g]);
    Some(out.into())
}

pub fn make_clip(samples: &[f32], channels: usize, rate: u32) -> Option<Sound> {
    resample(samples, channels, rate).map(Into::into)
}

struct Voice {
    sound: Sound,
    pos: usize,
    amp: f32,
}

pub struct Player {
    bank: Option<Arc<Bank>>,
    pos: [usize; 2],
    cur: [f32; 2],
    duck: f32,
    voices: Vec<Voice>,
    heard: Option<[u32; 4]>,
    next_slap: usize,
}

impl Player {
    pub fn new() -> Self {
        Player { bank: None, pos: [0; 2], cur: [0.0; 2], duck: 1.0, voices: Vec::new(), heard: None, next_slap: 0 }
    }

    pub fn has_bank(&self) -> bool {
        self.bank.is_some()
    }

    pub fn set_bank(&mut self, bank: Arc<Bank>) {
        self.bank = Some(bank);
    }

    fn cues(&mut self, mix: &Mix) {
        let now = [mix.alarm_slaps, mix.floods, mix.wolf_sighted, mix.wolf_kills];
        let [slaps, floods, sighted, kills] = self.heard.unwrap_or(now);
        self.heard = Some(now);
        let Some(bank) = &self.bank else { return };
        if slaps != mix.alarm_slaps && !bank.slaps.is_empty() {
            let sound = bank.slaps[self.next_slap % bank.slaps.len()].clone();
            self.next_slap += 1;
            self.voices.push(Voice { sound, pos: 0, amp: mix.alarm_amp.clamp(0.0, 1.2) });
        }
        if floods != mix.floods {
            self.voices.push(Voice { sound: bank.flood.clone(), pos: 0, amp: 0.9 });
        }
        if sighted != mix.wolf_sighted {
            self.voices.push(Voice { sound: bank.growl.clone(), pos: 0, amp: 0.55 });
        }
        if kills != mix.wolf_kills {
            self.voices.push(Voice { sound: bank.howl.clone(), pos: 0, amp: 0.35 });
        }
    }

    pub fn render(&mut self, mix: &Mix, out: &mut [f32]) {
        self.cues(mix);
        let settings = mix.settings.clamped();
        let target = [mix.layers.brook.clamp(0.0, 1.0), mix.layers.river.clamp(0.0, 1.0)];
        let k = 1.0 / (1.6 * SAMPLE_RATE as f32);
        let Some(bank) = self.bank.clone() else {
            out.fill(0.0);
            return;
        };
        let beds = [&bank.brook, &bank.river];
        let (duck_in, duck_out) = (1.0 / (0.03 * SAMPLE_RATE as f32), 1.0 / (1.5 * SAMPLE_RATE as f32));
        for frame in out.chunks_exact_mut(2) {
            let (mut bed, mut fx) = ([0.0f32; 2], [0.0f32; 2]);
            let duck = if self.voices.is_empty() { 1.0 } else { 0.45 };
            self.duck += (duck - self.duck) * if duck < self.duck { duck_in } else { duck_out };
            for i in 0..2 {
                self.cur[i] += (target[i] - self.cur[i]) * k;
                let s = beds[i][self.pos[i]];
                self.pos[i] = (self.pos[i] + 1) % beds[i].len();
                bed[0] += s[0] * self.cur[i] * self.duck;
                bed[1] += s[1] * self.cur[i] * self.duck;
            }
            self.voices.retain_mut(|v| {
                let s = v.sound[v.pos];
                fx[0] += s[0] * v.amp;
                fx[1] += s[1] * v.amp;
                v.pos += 1;
                v.pos < v.sound.len()
            });
            for c in 0..2 {
                frame[c] = ((bed[c] * settings.ambience + fx[c] * settings.effects) * settings.master).tanh();
            }
        }
    }
}
