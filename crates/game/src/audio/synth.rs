//! Procedural sound synthesis (pure std, no assets): continuous beds (water, wind), one-shot
//! voices (birds, frogs, gnawing, splashes, thunks, ice) and a gentle generative music loop.
//! `Synth::render` produces interleaved stereo f32 at `SAMPLE_RATE`.
use super::mix::{LayerVolumes, Season, VolumeSettings};
use std::f32::consts::{PI, TAU};

pub const SAMPLE_RATE: u32 = 44_100;
const BLOCK: usize = 256;
const MAX_VOICES: usize = 72;

/// Layer volumes plus user settings: everything the synth is steered by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub layers: LayerVolumes,
    pub settings: VolumeSettings,
}

impl Default for Mix {
    fn default() -> Self {
        Self { layers: LayerVolumes::SILENT, settings: VolumeSettings::default() }
    }
}

// ---- small DSP helpers ----------------------------------------------------------------------

#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    pub fn f(&mut self) -> f32 {
        (self.u32() >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn noise(&mut self) -> f32 {
        self.f() * 2.0 - 1.0
    }
    fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

/// Chamberlin state-variable filter: returns (low, band, high).
#[derive(Clone, Copy, Default)]
struct Svf {
    lp: f32,
    bp: f32,
}

impl Svf {
    fn run(&mut self, x: f32, fc: f32, damp: f32) -> (f32, f32, f32) {
        let f = 2.0 * (PI * fc.clamp(10.0, SAMPLE_RATE as f32 * 0.16) / SAMPLE_RATE as f32).sin();
        let hp = x - self.lp - damp * self.bp;
        self.bp += f * hp;
        self.lp += f * self.bp;
        (self.lp, self.bp, hp)
    }
}

/// Slow wandering control signal in 0..1 (sum of three incommensurate sines).
#[derive(Clone, Copy)]
struct Wander {
    ph: [f32; 3],
    rate: [f32; 3],
}

impl Wander {
    fn new(rng: &mut Rng, hz: f32) -> Self {
        let r = |rng: &mut Rng, m: f32| TAU * hz * m * rng.range(0.85, 1.15) / SAMPLE_RATE as f32;
        Wander { ph: [rng.f() * TAU, rng.f() * TAU, rng.f() * TAU], rate: [r(rng, 1.0), r(rng, 1.7), r(rng, 2.9)] }
    }
    fn next(&mut self) -> f32 {
        for i in 0..3 {
            self.ph[i] = (self.ph[i] + self.rate[i]) % TAU;
        }
        0.5 + 0.5 * (0.5 * self.ph[0].sin() + 0.3 * self.ph[1].sin() + 0.2 * self.ph[2].sin())
    }
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos(), a.sin())
}

fn midi_hz(m: f32) -> f32 {
    440.0 * ((m - 69.0) / 12.0).exp2()
}

// ---- one-shot voices ----------------------------------------------------------------------

enum Kind {
    /// Water bubble: rising sine blip.
    Bubble { f0: f32, ph: f32 },
    /// A run of syllables (bird phrase or frog peeper series). `shape`: 0 up, 1 down, 2 trill, 3 peeper.
    Phrase { n: u32, syl: f32, gap: f32, base: f32, span: f32, shape: u8, seed: f32, ph: f32 },
    /// Frog croak: pulse-train modulated harmonic tone.
    Croak { f0: f32, rate: f32, len: f32, ph: f32, pp: f32 },
    /// Beaver gnawing: a rapid run of tiny woody clicks.
    Gnaw { rate: f32, len: f32, next: f32, env: f32, svf: Svf, fc: f32 },
    /// Water plop, or a tail slap (crack + thump + spray).
    Splash { slap: bool, a: Svf, b: Svf, ph: f32 },
    /// Wood thunk: modal body plus a click.
    Thunk { f0: f32, ph: [f32; 3], lp: f32 },
    /// Ice: 0 creak, 1 tinkle, 2 deep boom.
    Ice { kind: u8, f0: f32, f1: f32, len: f32, ph: f32 },
}

struct Voice {
    kind: Kind,
    /// Seconds since start; negative = delayed start.
    t: f32,
    amp: f32,
    pan: f32,
    /// true = effects bus, false = ambience bus.
    effect: bool,
}

impl Voice {
    fn new(kind: Kind, amp: f32, pan: f32, effect: bool) -> Self {
        Voice { kind, t: 0.0, amp, pan, effect }
    }
    fn delayed(mut self, s: f32) -> Self {
        self.t = -s;
        self
    }

    /// Next mono sample; `None` once the voice has finished.
    fn tick(&mut self, rng: &mut Rng) -> Option<f32> {
        let dt = 1.0 / SAMPLE_RATE as f32;
        self.t += dt;
        let t = self.t;
        if t < 0.0 {
            return Some(0.0);
        }
        let out = match &mut self.kind {
            Kind::Bubble { f0, ph } => {
                if t > 0.12 {
                    return None;
                }
                let fr = *f0 * (1.0 + 11.0 * t);
                *ph = (*ph + TAU * fr * dt) % TAU;
                ph.sin() * (-t * 45.0).exp() * (t * 600.0).min(1.0)
            }
            Kind::Phrase { n, syl, gap, base, span, shape, seed, ph } => {
                let period = *syl + *gap;
                if t >= *n as f32 * period {
                    return None;
                }
                let idx = (t / period) as u32;
                let local = t - idx as f32 * period;
                if local >= *syl {
                    0.0
                } else {
                    let u = local / *syl;
                    let fmod = 1.0 + 0.12 * (idx as f32 * 2.1 + *seed).sin();
                    let fr = match *shape {
                        0 => *base * fmod * (1.0 + *span * u),
                        1 => *base * fmod * (1.0 + *span * (1.0 - u)),
                        2 => *base * fmod * (1.0 + *span * 0.5 * (TAU * u * 3.0).sin()),
                        _ => *base * (1.0 + *span * u * u),
                    };
                    let fr = if *shape == 2 { fr * (1.0 + 0.02 * (TAU * 38.0 * local).sin()) } else { fr };
                    *ph = (*ph + TAU * fr * dt) % TAU;
                    let env = (PI * u).sin().powf(0.6);
                    (ph.sin() + 0.28 * (2.0 * *ph).sin() + 0.08 * (3.0 * *ph).sin()) * env * 0.6
                }
            }
            Kind::Croak { f0, rate, len, ph, pp } => {
                if t > *len {
                    return None;
                }
                let u = t / *len;
                *pp = (*pp + *rate * (1.0 - 0.3 * u) * dt) % 1.0;
                let pulse = (0.5 + 0.5 * (TAU * *pp).cos()).powi(2);
                *ph = (*ph + TAU * *f0 * (1.15 - 0.3 * u) * dt) % TAU;
                let tone = ph.sin() + 0.6 * (2.0 * *ph).sin() + 0.5 * (3.0 * *ph).sin() + 0.3 * (4.0 * *ph).sin();
                tone * (PI * u).sin().sqrt() * pulse * 0.45
            }
            Kind::Gnaw { rate, len, next, env, svf, fc } => {
                if t > *len {
                    return None;
                }
                if t >= *next {
                    *env = rng.range(0.55, 1.0);
                    *next += rng.range(0.8, 1.25) / *rate;
                }
                *env *= (-dt / 0.006).exp();
                let (_, bp, _) = svf.run(rng.noise(), *fc, 0.35);
                bp * *env * 3.0 * (PI * t / *len).sin().sqrt()
            }
            Kind::Splash { slap, a, b, ph } => {
                let len = if *slap { 1.0 } else { 0.7 };
                if t > len {
                    return None;
                }
                let n = rng.noise();
                let body_t = if *slap { (t - 0.03).max(0.0) } else { t };
                let sweep = 600.0 + 1800.0 * (-body_t / 0.12).exp();
                let (_, bp, _) = a.run(n, sweep, 0.7);
                let rise = 1.0 - (-body_t / 0.012).exp();
                let decay = (-body_t / if *slap { 0.28 } else { 0.18 }).exp();
                let mut s = bp * rise * decay * if *slap { 1.4 } else { 1.0 };
                if *slap {
                    let (_, crack, _) = b.run(n, 2800.0, 0.6);
                    *ph = (*ph + TAU * (55.0 + 90.0 * (-t / 0.05).exp()) * dt) % TAU;
                    s += crack * (-t / 0.012).exp() * 2.2 + ph.sin() * (-t / 0.11).exp() * 0.9;
                } else {
                    // a little "bloop" under the splash
                    *ph = (*ph + TAU * (260.0 + 900.0 * t) * dt) % TAU;
                    s += ph.sin() * (-t / 0.05).exp() * 0.5;
                }
                s * 0.7
            }
            Kind::Thunk { f0, ph, lp } => {
                if t > 0.4 {
                    return None;
                }
                let drop = 1.0 + 0.12 * (-t / 0.04).exp();
                let ratios = [1.0, 2.76, 5.4];
                let amps = [1.0, 0.5, 0.3];
                let taus = [0.1, 0.05, 0.025];
                let mut s = 0.0;
                for i in 0..3 {
                    ph[i] = (ph[i] + TAU * *f0 * ratios[i] * drop * dt) % TAU;
                    s += ph[i].sin() * amps[i] * (-t / taus[i]).exp();
                }
                *lp += 0.25 * (rng.noise() - *lp);
                s * 0.7 + *lp * (-t / 0.004).exp() * 2.5
            }
            Kind::Ice { kind, f0, f1, len, ph } => {
                if t > *len {
                    return None;
                }
                let u = t / *len;
                match *kind {
                    0 => {
                        let fr = (*f0 + (*f1 - *f0) * u) * (1.0 + 0.04 * (TAU * 23.0 * t).sin());
                        *ph = (*ph + TAU * fr * dt) % TAU;
                        let trem = 0.65 + 0.35 * (TAU * 41.0 * t).sin();
                        (ph.sin() + 0.5 * (2.01 * *ph).sin()) * (PI * u).sin().powi(2) * trem * 0.6
                    }
                    1 => {
                        *ph = (*ph + TAU * *f0 * dt) % TAU;
                        (ph.sin() + 0.4 * (2.7 * *ph).sin()) * (-t / 0.06).exp() * 0.6
                    }
                    _ => {
                        let fr = 40.0 + 30.0 * (-t / 0.3).exp();
                        *ph = (*ph + TAU * fr * dt) % TAU;
                        ph.sin() * (-t / 0.35).exp() * (t * 100.0).min(1.0)
                    }
                }
            }
        };
        Some(out * self.amp)
    }
}

// ---- generative music -------------------------------------------------------------------------

struct PadOsc {
    freq: f32,
    ph: f32,
    gain: f32,
    target: f32,
    pan: f32,
}

struct Pluck {
    freq: f32,
    ph: f32,
    t: f32,
    amp: f32,
    pan: f32,
}

struct Music {
    rng: Rng,
    until_step: i32,
    step: u32,
    degree: i32,
    root: f32,
    minor: bool,
    pad: Vec<PadOsc>,
    plucks: Vec<Pluck>,
    delay: [Vec<f32>; 2],
    di: usize,
    damp: [f32; 2],
    lowpass: [f32; 2],
}

const MAJOR_PENT: [i32; 5] = [0, 2, 4, 7, 9];
const MINOR_PENT: [i32; 5] = [0, 3, 5, 7, 10];
/// ~54 bpm; a step is half a beat.
const STEP_SAMPLES: i32 = (SAMPLE_RATE as f32 * 60.0 / 54.0 / 2.0) as i32;

impl Music {
    fn new(seed: u64) -> Self {
        let sr = SAMPLE_RATE as f32;
        Music {
            rng: Rng::new(seed ^ 0xB1EA_5E5),
            until_step: 0,
            step: 0,
            degree: 0,
            root: 48.0,
            minor: false,
            pad: Vec::new(),
            plucks: Vec::new(),
            delay: [vec![0.0; (sr * 0.41) as usize], vec![0.0; (sr * 0.53) as usize]],
            di: 0,
            damp: [0.0; 2],
            lowpass: [0.0; 2],
        }
    }

    fn note(&self, deg: i32) -> f32 {
        let ints = if self.minor { MINOR_PENT } else { MAJOR_PENT };
        self.root + 12.0 * deg.div_euclid(5) as f32 + ints[deg.rem_euclid(5) as usize] as f32
    }

    fn new_chord(&mut self, season: Season) {
        // Season picks key and mood: bright and open in spring/summer, mellow in autumn, hushed in winter.
        let (root, minor) = match season {
            Season::Spring => (55.0, false),
            Season::Summer => (48.0, false),
            Season::Autumn => (45.0, true),
            Season::Winter => (50.0, true),
        };
        self.root = root;
        self.minor = minor;
        // Wander along the scale, tending to come home.
        let moves = [-3, -2, 0, 2, 3, 5];
        self.degree = (self.degree + moves[(self.rng.f() * 6.0) as usize % 6]).clamp(-2, 7);
        if self.rng.chance(0.3) {
            self.degree = 0;
        }
        for o in &mut self.pad {
            o.target = 0.0;
        }
        let d = self.degree;
        let tones = [(d - 5, 0.7), (d, 1.0), (d + 2, 0.8), (d + 4, 0.7), (d + 7, 0.35)];
        for (deg, level) in tones {
            let f = midi_hz(self.note(deg));
            for (k, det) in [0.9985f32, 1.0015].iter().enumerate() {
                let pan = if k == 0 { -0.4 } else { 0.4 } + self.rng.range(-0.2, 0.2);
                let ph = self.rng.f() * TAU;
                self.pad.push(PadOsc { freq: f * det, ph, gain: 0.0, target: 0.028 * level, pan });
            }
        }
    }

    fn pluck(&mut self, season: Season) {
        let (prob, reach) = match season {
            Season::Spring => (0.34, 9),
            Season::Summer => (0.3, 9),
            Season::Autumn => (0.22, 7),
            Season::Winter => (0.12, 5),
        };
        if self.plucks.len() >= 10 || !self.rng.chance(prob) {
            return;
        }
        let deg = self.degree + 5 + (self.rng.f() * reach as f32) as i32;
        let freq = midi_hz(self.note(deg) + 12.0);
        let (amp, pan) = (self.rng.range(0.035, 0.065), self.rng.range(-0.7, 0.7));
        self.plucks.push(Pluck { freq, ph: 0.0, t: 0.0, amp, pan });
    }

    fn tick(&mut self, season: Season) -> (f32, f32) {
        self.until_step -= 1;
        if self.until_step <= 0 {
            self.until_step = STEP_SAMPLES;
            if self.step % 16 == 0 {
                self.new_chord(season);
            }
            if self.step % 2 == 0 || self.rng.chance(0.3) {
                self.pluck(season);
            }
            self.step = self.step.wrapping_add(1);
        }
        let dt = 1.0 / SAMPLE_RATE as f32;
        let (mut l, mut r) = (0.0, 0.0);
        let k = dt / 2.2;
        for o in &mut self.pad {
            o.gain += (o.target - o.gain) * k;
            o.ph = (o.ph + TAU * o.freq * dt) % TAU;
            let s = (o.ph.sin() + 0.25 * (2.0 * o.ph).sin()) * o.gain;
            let (gl, gr) = pan_gains(o.pan);
            l += s * gl;
            r += s * gr;
        }
        self.pad.retain(|o| o.target > 0.0 || o.gain > 1e-4);
        for p in &mut self.plucks {
            p.t += dt;
            p.ph = (p.ph + TAU * p.freq * dt) % TAU;
            let env = (p.t * 250.0).min(1.0) * (-p.t * 1.5).exp();
            let s = (p.ph.sin() + 0.3 * (2.0 * p.ph).sin() * (-p.t * 3.0).exp() + 0.08 * (3.0 * p.ph).sin()) * env * p.amp;
            let (gl, gr) = pan_gains(p.pan);
            l += s * gl;
            r += s * gr;
        }
        self.plucks.retain(|p| p.t < 4.5);
        // Soft cross-fed echo for space.
        let (il, ir) = (self.di % self.delay[0].len(), self.di % self.delay[1].len());
        let (wl, wr) = (self.delay[0][il], self.delay[1][ir]);
        self.damp[0] += 0.35 * (wr * 0.55 - self.damp[0]);
        self.damp[1] += 0.35 * (wl * 0.55 - self.damp[1]);
        self.delay[0][il] = l + self.damp[0];
        self.delay[1][ir] = r + self.damp[1];
        self.di = self.di.wrapping_add(1);
        l += wl * 0.5;
        r += wr * 0.5;
        // Take the edge off.
        self.lowpass[0] += 0.3 * (l - self.lowpass[0]);
        self.lowpass[1] += 0.3 * (r - self.lowpass[1]);
        (self.lowpass[0], self.lowpass[1])
    }
}

// ---- the synth ------------------------------------------------------------------------------

const N_LAYERS: usize = 9;
const WATER: usize = 0;
const WIND: usize = 1;
const WWIND: usize = 2;
const BIRDS: usize = 3;
const FROGS: usize = 4;
const GNAW: usize = 5;
const SPLASH: usize = 6;
const THUNK: usize = 7;
const ICE: usize = 8;

fn layer_array(v: &LayerVolumes) -> [f32; N_LAYERS] {
    [v.water, v.wind, v.winter_wind, v.birds, v.frogs, v.gnaw, v.splash, v.thunk, v.ice]
}

pub struct Synth {
    rng: Rng,
    cur: [f32; N_LAYERS],
    music_cur: f32,
    voices: Vec<Voice>,
    music: Music,
    // continuous beds, per channel
    water_a: [Svf; 2],
    water_b: [Svf; 2],
    water_c: [Svf; 2],
    water_lfo: [Wander; 3],
    wind_a: [Svf; 2],
    wind_b: [Svf; 2],
    wind_hiss: [Svf; 2],
    howl: [Svf; 2],
    gust: Wander,
    gust_slow: Wander,
    warmth_cur: f32,
    frames: u64,
}

impl Synth {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let water_lfo = [Wander::new(&mut rng, 0.35), Wander::new(&mut rng, 0.6), Wander::new(&mut rng, 0.13)];
        let gust = Wander::new(&mut rng, 0.09);
        let gust_slow = Wander::new(&mut rng, 0.05);
        Synth {
            music: Music::new(seed),
            rng,
            cur: [0.0; N_LAYERS],
            music_cur: 0.0,
            voices: Vec::new(),
            water_a: [Svf::default(); 2],
            water_b: [Svf::default(); 2],
            water_c: [Svf::default(); 2],
            water_lfo,
            wind_a: [Svf::default(); 2],
            wind_b: [Svf::default(); 2],
            wind_hiss: [Svf::default(); 2],
            howl: [Svf::default(); 2],
            gust,
            gust_slow,
            warmth_cur: 1.0,
            frames: 0,
        }
    }

    /// Renders interleaved stereo frames into `out` (length must be even).
    pub fn render(&mut self, mix: &Mix, out: &mut [f32]) {
        let settings = mix.settings.clamped();
        let target = layer_array(&mix.layers);
        let music_target = mix.layers.music.clamp(0.0, 1.0);
        let k = 1.0 / (1.6 * SAMPLE_RATE as f32);
        let mut since_events = BLOCK;
        for frame in out.chunks_exact_mut(2) {
            if since_events >= BLOCK {
                since_events = 0;
                self.spawn_events(mix);
            }
            since_events += 1;
            for i in 0..N_LAYERS {
                self.cur[i] += (target[i] - self.cur[i]) * k;
            }
            self.music_cur += (music_target - self.music_cur) * k;
            self.warmth_cur += (mix.layers.warmth - self.warmth_cur) * k;

            let (mut amb_l, mut amb_r) = self.beds();
            let (mut fx_l, mut fx_r) = (0.0, 0.0);
            let rng = &mut self.rng;
            self.voices.retain_mut(|v| match v.tick(rng) {
                Some(s) => {
                    let (gl, gr) = pan_gains(v.pan);
                    if v.effect {
                        fx_l += s * gl;
                        fx_r += s * gr;
                    } else {
                        amb_l += s * gl;
                        amb_r += s * gr;
                    }
                    true
                }
                None => false,
            });
            let (ml, mr) = if self.music_cur > 1e-3 { self.music.tick(mix.layers.season) } else { (0.0, 0.0) };
            let (ml, mr) = (ml * self.music_cur, mr * self.music_cur);

            let g = settings.master;
            let l = (amb_l * settings.ambience + fx_l * settings.effects + ml * settings.music * 2.2) * g;
            let r = (amb_r * settings.ambience + fx_r * settings.effects + mr * settings.music * 2.2) * g;
            frame[0] = l.tanh();
            frame[1] = r.tanh();
        }
        self.frames += (out.len() / 2) as u64;
    }

    /// Continuous layers: flowing water, summer breeze and leaves, winter wind.
    fn beds(&mut self) -> (f32, f32) {
        let mut out = [0.0f32; 2];
        let (w, wd, ww) = (self.cur[WATER], self.cur[WIND], self.cur[WWIND]);
        if w > 1e-3 {
            let (l0, l1, l2) = (self.water_lfo[0].next(), self.water_lfo[1].next(), self.water_lfo[2].next());
            for c in 0..2 {
                let n = self.rng.noise();
                let (_, body, _) = self.water_a[c].run(n, 650.0 + 700.0 * l0, 0.55);
                let (_, _, hiss) = self.water_b[c].run(self.rng.noise(), 3800.0, 1.0);
                let (rumble, _, _) = self.water_c[c].run(self.rng.noise(), 160.0, 1.0);
                let s = body * 1.7 + hiss * (0.12 + 0.12 * l1) + rumble * 0.5;
                out[c] += s * (0.5 + 0.5 * l2) * w * 0.45;
            }
        }
        if wd > 1e-3 || ww > 1e-3 {
            let gust = self.gust.next();
            let slow = self.gust_slow.next();
            let warm = self.warmth_cur;
            for c in 0..2 {
                let n = self.rng.noise();
                if wd > 1e-3 {
                    let (lp, bp, _) = self.wind_a[c].run(n, 220.0 + 650.0 * gust, 0.7);
                    let (_, _, hp) = self.wind_hiss[c].run(self.rng.noise(), 3200.0, 1.0);
                    let leaves = hp * (0.05 + 0.2 * gust * gust) * warm;
                    out[c] += (bp * 1.6 + lp * 0.4 + leaves) * (0.35 + 0.65 * gust) * wd * 0.6;
                }
                if ww > 1e-3 {
                    let (_, howl, _) = self.howl[c].run(self.rng.noise(), 330.0 + 520.0 * slow + 120.0 * gust, 0.14);
                    let (lp, _, _) = self.wind_b[c].run(n, 300.0 + 500.0 * gust, 0.8);
                    out[c] += (howl * 1.6 + lp * 0.6) * (0.35 + 0.65 * slow) * ww * 0.3;
                }
            }
        }
        (out[0], out[1])
    }

    fn spawn(&mut self, v: Voice) {
        if self.voices.len() < MAX_VOICES {
            self.voices.push(v);
        }
    }

    /// Poisson-ish scheduling of one-shot voices, once per block, rate scaled by layer volume.
    fn spawn_events(&mut self, mix: &Mix) {
        let dt = BLOCK as f32 / SAMPLE_RATE as f32;
        let c = self.cur;
        let eff = true;
        let roll = |s: &mut Self, rate: f32, vol: f32| vol > 0.01 && s.rng.f() < rate * vol * dt;

        // water: bubbles
        if roll(self, 26.0, c[WATER]) {
            let f0 = self.rng.range(380.0, 1500.0);
            let (amp, pan) = (self.rng.range(0.05, 0.14) * c[WATER], self.rng.range(-0.8, 0.8));
            self.spawn(Voice::new(Kind::Bubble { f0, ph: 0.0 }, amp, pan, false));
        }
        // birds
        if roll(self, 0.55, c[BIRDS]) {
            let amp = (0.12 + 0.22 * c[BIRDS]) * self.rng.range(0.6, 1.0);
            let pan = self.rng.range(-0.9, 0.9);
            let seed = self.rng.f() * 10.0;
            let kind = match self.rng.f() {
                x if x < 0.4 => Kind::Phrase { n: 2 + (self.rng.f() * 3.0) as u32, syl: 0.09, gap: self.rng.range(0.05, 0.12), base: self.rng.range(3000.0, 4200.0), span: self.rng.range(0.25, 0.5), shape: 0, seed, ph: 0.0 },
                x if x < 0.65 => Kind::Phrase { n: 3 + (self.rng.f() * 2.0) as u32, syl: 0.26, gap: 0.12, base: self.rng.range(1900.0, 2700.0), span: self.rng.range(0.2, 0.45), shape: 1, seed, ph: 0.0 },
                x if x < 0.85 => Kind::Phrase { n: 1 + (self.rng.f() * 2.0) as u32, syl: 0.55, gap: 0.2, base: self.rng.range(2600.0, 3600.0), span: 0.35, shape: 2, seed, ph: 0.0 },
                _ => Kind::Phrase { n: 6 + (self.rng.f() * 5.0) as u32, syl: 0.07, gap: 0.045, base: self.rng.range(3500.0, 4600.0), span: 0.4, shape: if self.rng.chance(0.5) { 0 } else { 1 }, seed, ph: 0.0 },
            };
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        // frogs: croaks and peepers
        if roll(self, 1.3, c[FROGS]) {
            let f0 = self.rng.range(95.0, 190.0);
            let amp = (0.13 + 0.25 * c[FROGS]) * self.rng.range(0.6, 1.0);
            let kind = Kind::Croak { f0, rate: self.rng.range(18.0, 30.0), len: self.rng.range(0.22, 0.5), ph: 0.0, pp: 0.0 };
            let pan = self.rng.range(-0.9, 0.9);
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        if roll(self, 0.6, c[FROGS]) {
            let amp = (0.06 + 0.09 * c[FROGS]) * self.rng.range(0.6, 1.0);
            let kind = Kind::Phrase { n: 5 + (self.rng.f() * 5.0) as u32, syl: 0.055, gap: 0.12, base: self.rng.range(2500.0, 3100.0), span: 0.12, shape: 3, seed: 0.0, ph: 0.0 };
            let pan = self.rng.range(-0.9, 0.9);
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        // beavers gnawing
        if roll(self, 0.3, c[GNAW]) {
            let kind = Kind::Gnaw { rate: self.rng.range(7.0, 11.0), len: self.rng.range(0.9, 2.4), next: 0.0, env: 0.0, svf: Svf::default(), fc: self.rng.range(1500.0, 2600.0) };
            let (amp, pan) = (0.18 + 0.3 * c[GNAW], self.rng.range(-0.6, 0.6));
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        // splashes and tail slaps
        if roll(self, 0.22, c[SPLASH]) {
            let (amp, pan) = (0.4 + 0.6 * c[SPLASH], self.rng.range(-0.8, 0.8));
            self.spawn(Voice::new(Kind::Splash { slap: false, a: Svf::default(), b: Svf::default(), ph: 0.0 }, amp, pan, eff));
        }
        if roll(self, 0.05, c[SPLASH]) {
            let (amp, pan) = (0.6 + 0.6 * c[SPLASH], self.rng.range(-0.8, 0.8));
            self.spawn(Voice::new(Kind::Splash { slap: true, a: Svf::default(), b: Svf::default(), ph: 0.0 }, amp, pan, eff));
        }
        // wood thunks at work sites (often in pairs)
        if roll(self, 0.8, c[THUNK]) {
            let (amp, pan) = (0.22 + 0.35 * c[THUNK], self.rng.range(-0.6, 0.6));
            let f0 = self.rng.range(150.0, 260.0);
            let twice = self.rng.chance(0.4);
            self.spawn(Voice::new(Kind::Thunk { f0, ph: [0.0; 3], lp: 0.0 }, amp, pan, eff));
            if twice {
                let f1 = f0 * self.rng.range(0.9, 1.1);
                let delay = self.rng.range(0.22, 0.4);
                self.spawn(Voice::new(Kind::Thunk { f0: f1, ph: [0.0; 3], lp: 0.0 }, amp * 0.8, pan, eff).delayed(delay));
            }
        }
        // ice
        if roll(self, 0.14, c[ICE]) {
            let f0 = self.rng.range(900.0, 2200.0);
            let kind = Kind::Ice { kind: 0, f0, f1: f0 * self.rng.range(0.15, 0.4), len: self.rng.range(0.35, 0.8), ph: 0.0 };
            let (amp, pan) = (0.06 + 0.12 * c[ICE], self.rng.range(-0.8, 0.8));
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        if roll(self, 0.3, c[ICE]) {
            let kind = Kind::Ice { kind: 1, f0: self.rng.range(2800.0, 5200.0), f1: 0.0, len: 0.4, ph: 0.0 };
            let (amp, pan) = (0.03 + 0.05 * c[ICE], self.rng.range(-0.8, 0.8));
            self.spawn(Voice::new(kind, amp, pan, eff));
        }
        if roll(self, 0.05, c[ICE]) {
            let kind = Kind::Ice { kind: 2, f0: 0.0, f1: 0.0, len: 1.3, ph: 0.0 };
            let amp = 0.3 + 0.3 * c[ICE];
            self.spawn(Voice::new(kind, amp, 0.0, eff));
        }
        let _ = mix;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(layers: LayerVolumes, secs: usize, seed: u64) -> Vec<f32> {
        let mix = Mix { layers, settings: VolumeSettings::default() };
        let mut s = Synth::new(seed);
        let mut out = vec![0.0; secs * SAMPLE_RATE as usize * 2];
        for c in out.chunks_mut(2048) {
            s.render(&mix, c);
        }
        out
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    fn solo(f: impl Fn(&mut LayerVolumes)) -> Vec<f32> {
        let mut l = LayerVolumes::SILENT;
        f(&mut l);
        render(l, 12, 3)
    }

    #[test]
    fn audio_every_layer_is_audible_but_gentle() {
        let layers: [(&str, Vec<f32>); 10] = [
            ("water", solo(|l| l.water = 1.0)),
            ("wind", solo(|l| l.wind = 1.0)),
            ("winter_wind", solo(|l| { l.winter_wind = 1.0; l.warmth = 0.0; l.season = Season::Winter })),
            ("birds", solo(|l| l.birds = 1.0)),
            ("frogs", solo(|l| l.frogs = 1.0)),
            ("gnaw", solo(|l| l.gnaw = 1.0)),
            ("splash", solo(|l| l.splash = 1.0)),
            ("thunk", solo(|l| l.thunk = 1.0)),
            ("ice", solo(|l| l.ice = 1.0)),
            ("music", solo(|l| l.music = 1.0)),
        ];
        for (name, out) in &layers {
            let (r, peak) = (rms(out), out.iter().fold(0.0f32, |a, v| a.max(v.abs())));
            println!("{name}: rms {r:.4} peak {peak:.3}");
            assert!(out.iter().all(|v| v.is_finite()), "{name} not finite");
            assert!(r > 0.004, "{name} too quiet: {r}");
            assert!(r < 0.3 && peak <= 1.0, "{name} too loud: rms {r} peak {peak}");
        }
    }

    #[test]
    fn audio_render_is_deterministic_and_silent_when_muted() {
        let mut l = LayerVolumes::SILENT;
        l.water = 0.8;
        l.birds = 0.8;
        l.music = 0.5;
        assert_eq!(render(l, 2, 9), render(l, 2, 9));
        assert_ne!(render(l, 2, 9), render(l, 2, 10));
        let mix = Mix { layers: l, settings: VolumeSettings { master: 0.0, ..Default::default() } };
        let mut out = vec![1.0; 4096];
        Synth::new(1).render(&mix, &mut out);
        assert!(out.iter().all(|v| *v == 0.0));
        let silent = render(LayerVolumes::SILENT, 1, 1);
        assert!(silent.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn audio_volume_controls_scale_buses() {
        let mut l = LayerVolumes::SILENT;
        l.water = 0.8;
        l.music = 0.8;
        let at = |s: VolumeSettings| {
            let mut synth = Synth::new(2);
            let mut out = vec![0.0; 2 * SAMPLE_RATE as usize * 3];
            synth.render(&Mix { layers: l, settings: s }, &mut out);
            rms(&out)
        };
        let base = VolumeSettings::default();
        let full = at(base);
        assert!(at(VolumeSettings { master: base.master * 0.5, ..base }) < 0.7 * full);
        let (no_music, no_amb) = (at(VolumeSettings { music: 0.0, ..base }), at(VolumeSettings { ambience: 0.0, ..base }));
        assert!(no_music < full && no_amb < full && no_music > 0.0 && no_amb > 0.0);
    }

    #[test]
    fn audio_winter_mix_differs_from_summer() {
        let summer = LayerVolumes { water: 0.9, wind: 0.4, birds: 0.7, frogs: 0.4, music: 0.6, ..LayerVolumes::SILENT };
        let winter = LayerVolumes { water: 0.15, winter_wind: 0.6, ice: 0.9, music: 0.5, warmth: 0.0, season: Season::Winter, ..LayerVolumes::SILENT };
        let (s, w) = (render(summer, 6, 4), render(winter, 6, 4));
        // Zero-crossing rate is a crude brightness measure: birdsong keeps summer brighter.
        let zc = |x: &[f32]| x.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|p| p[0] * p[1] < 0.0).count();
        assert!(zc(&s) > zc(&w), "summer {} winter {}", zc(&s), zc(&w));
    }
}
