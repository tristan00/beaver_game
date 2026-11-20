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

/// Spring colour arrives with the thaw, rather than taking half of spring to appear.
/// The final interval wraps continuously from winter to the next spring.
pub fn blend<T: Copy>(keys: &[T; 4], phase: f32, lerp: impl Fn(T, T, f32) -> T) -> T {
    const DAYS: [f32; 5] = [15.0, 135.0, 225.0, 305.0, 375.0];
    let mut day = phase.rem_euclid(1.0) * 360.0;
    if day < DAYS[0] { day += 360.0; }
    let i = (0..4).find(|&i| day < DAYS[i + 1]).unwrap_or(3);
    lerp(keys[i], keys[(i + 1) % 4], smooth(DAYS[i], DAYS[i + 1], day))
}

pub fn snow(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * 360.0;
    let distance = (day - 310.0).abs();
    1.0 - smooth(18.0, 62.0, distance.min(360.0 - distance))
}

pub fn autumn(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * 360.0;
    smooth(165.0, 220.0, day) * (1.0 - smooth(255.0, 295.0, day))
}

pub fn bloom(phase: f32) -> f32 {
    let day = phase.rem_euclid(1.0) * 360.0;
    smooth(0.0, 24.0, day) * (1.0 - smooth(80.0, 195.0, day))
}
