//! Shared visual calendar. Terrain, foliage and lighting must follow the same year.
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn parse_phase(value: &str) -> Option<f32> {
    match value {
        "spring" => Some(0.125),
        "summer" => Some(0.375),
        "autumn" => Some(0.625),
        "winter" => Some(0.875),
        n => n.parse::<f32>().ok().filter(|p| p.is_finite()).map(|p| p.rem_euclid(1.0)),
    }
}

pub fn phase(time_days: f64) -> f32 {
    std::env::var("BEAVER_SEASON").ok().and_then(|v| parse_phase(&v))
        .unwrap_or_else(|| sim::calendar::year_phase(time_days))
}

/// Days in the game year; every curve below is in days of the year.
const YEAR: f32 = sim::calendar::YEAR_DAYS as f32;

/// Spring colour arrives with the thaw, rather than taking half of spring to appear.
/// The final interval wraps continuously from winter to the next spring.
pub fn blend<T: Copy>(keys: &[T; 4], phase: f32, lerp: impl Fn(T, T, f32) -> T) -> T {
    const DAYS: [f32; 5] = [15.0, 135.0, 225.0, 305.0, 375.0];
    let mut day = phase.rem_euclid(1.0) * YEAR;
    if day < DAYS[0] { day += YEAR; }
    let i = (0..4).find(|&i| day < DAYS[i + 1]).unwrap_or(3);
    lerp(keys[i], keys[(i + 1) % 4], smooth(DAYS[i], DAYS[i + 1], day))
}

pub fn snow(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * YEAR;
    let distance = (day - 310.0).abs();
    1.0 - smooth(18.0, 62.0, distance.min(YEAR - distance))
}

pub fn autumn(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * YEAR;
    smooth(165.0, 220.0, day) * (1.0 - smooth(255.0, 295.0, day))
}

pub fn bloom(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * YEAR;
    smooth(0.0, 24.0, day) * (1.0 - smooth(80.0, 195.0, day))
}

pub fn leaves(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * YEAR;
    if day < 30.0 { 0.22 + 0.78 * smooth(0.0, 30.0, day) }
    else if day < 220.0 { 1.0 }
    else if day < 285.0 { 1.0 - smooth(220.0, 285.0, day) }
    else { 0.22 * smooth(340.0, YEAR, day) }
}
