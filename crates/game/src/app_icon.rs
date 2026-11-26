//! Application and taskbar icon: a beaver in profile gnawing a notched tree trunk.
//!
//! Rasterized procedurally from signed distance fields so the runtime window/taskbar icon and
//! embedded Windows `.ico` always match without shipping hand-edited bitmaps.

use crate::icons::{sd_circle, sd_ellipse, sd_poly, sd_round_rect, sd_seg};
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub struct AppIconPlugin;

impl Plugin for AppIconPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconApplied>()
            .add_systems(Update, apply_window_icon.run_if(|a: Res<IconApplied>| !a.0));
    }
}

#[derive(Resource, Default)]
struct IconApplied(bool);

fn apply_window_icon(
    windows: Query<Entity, With<PrimaryWindow>>,
    mut applied: ResMut<IconApplied>,
    _non_send: NonSendMarker,
) {
    let Ok(entity) = windows.single() else {
        return;
    };
    let done = bevy::winit::WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(window) = winit_windows.get_window(entity) else {
            return false;
        };
        let rgba128 = rasterize(128);
        let rgba32 = downsample(&rgba128, 128, 4);
        if let Ok(icon) = winit::window::Icon::from_rgba(rgba32, 32, 32) {
            window.set_window_icon(Some(icon));
        }
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowExtWindows;
            if let Ok(tb) = winit::window::Icon::from_rgba(rgba128, 128, 128) {
                window.set_taskbar_icon(Some(tb));
            }
        }
        true
    });
    if done {
        applied.0 = true;
    }
}

/// Rasterize the beaver-gnawing-a-tree icon at `size x size` (RGBA8, unpremultiplied) with 4x4 SSAA.
pub fn rasterize(size: u32) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
    let inv = 64.0 / size as f32;
    let sub = 4u32;
    let sub_inv = 1.0 / sub as f32;
    let samples = (sub * sub) as f32;

    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..sub {
                for sx in 0..sub {
                    let x = (px as f32 + (sx as f32 + 0.5) * sub_inv) * inv;
                    let y = (py as f32 + (sy as f32 + 0.5) * sub_inv) * inv;
                    let c = sample(Vec2::new(x, y));
                    // Accumulate premultiplied color for clean anti-aliased edges.
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let a = (acc[3] / samples).clamp(0.0, 1.0);
            let idx = ((py * size + px) * 4) as usize;
            if a > 1e-4 {
                out[idx] = ((acc[0] / acc[3]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[idx + 1] = ((acc[1] / acc[3]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[idx + 2] = ((acc[2] / acc[3]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[idx + 3] = (a * 255.0 + 0.5) as u8;
            }
        }
    }
    out
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a]
}

fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let sa = src[3];
    if sa <= 0.0 {
        return dst;
    }
    let da = dst[3];
    let oa = sa + da * (1.0 - sa);
    if oa <= 1e-6 {
        return [0.0; 4];
    }
    [
        (src[0] * sa + dst[0] * da * (1.0 - sa)) / oa,
        (src[1] * sa + dst[1] * da * (1.0 - sa)) / oa,
        (src[2] * sa + dst[2] * da * (1.0 - sa)) / oa,
        oa,
    ]
}

fn lerp_rgb(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 1.0]
}

fn downsample(src: &[u8], size: u32, factor: u32) -> Vec<u8> {
    let dst_size = size / factor;
    let n = (factor * factor) as u32;
    let mut out = vec![0u8; (dst_size * dst_size * 4) as usize];
    for y in 0..dst_size {
        for x in 0..dst_size {
            let mut acc = [0u32; 4];
            for sy in 0..factor {
                for sx in 0..factor {
                    let i = (((y * factor + sy) * size + x * factor + sx) * 4) as usize;
                    let a = src[i + 3] as u32;
                    acc[0] += src[i] as u32 * a;
                    acc[1] += src[i + 1] as u32 * a;
                    acc[2] += src[i + 2] as u32 * a;
                    acc[3] += a;
                }
            }
            let o = ((y * dst_size + x) * 4) as usize;
            if acc[3] > 0 {
                out[o] = ((acc[0] + acc[3] / 2) / acc[3]).min(255) as u8;
                out[o + 1] = ((acc[1] + acc[3] / 2) / acc[3]).min(255) as u8;
                out[o + 2] = ((acc[2] + acc[3] / 2) / acc[3]).min(255) as u8;
                out[o + 3] = ((acc[3] + n / 2) / n).min(255) as u8;
            }
        }
    }
    out
}

fn cov(d: f32) -> f32 {
    (0.5 - d * 1.4).clamp(0.0, 1.0)
}

/// Evaluate the icon color at `p` in `0..64` logical coordinates.
fn sample(p: Vec2) -> [f32; 4] {
    // 1. Rounded-square badge with sky, grassy bank, and pond strip.
    let badge_d = sd_round_rect(p, Vec2::new(32.0, 32.0), Vec2::new(29.5, 29.5), 12.5);
    let badge_a = cov(badge_d);
    if badge_a <= 0.0 {
        return [0.0; 4];
    }

    // Warm sunlit sky gradient.
    let mut col = lerp_rgb(rgba(235, 244, 248, 1.0), rgba(182, 217, 232, 1.0), (p.y - 3.0) / 40.0);

    // Distant soft meadow hill.
    let hill_y = 43.0 + ((p.x - 20.0) * 0.08).sin() * 2.6;
    if p.y > hill_y {
        let t = ((p.y - hill_y) / 10.0).clamp(0.0, 1.0);
        col = over(col, lerp_rgb(rgba(130, 168, 96, 1.0), rgba(96, 134, 72, 1.0), t));
    }

    // Riverbank earth under the beaver and tree.
    let bank_d = sd_ellipse(p, Vec2::new(34.0, 62.0), Vec2::new(38.0, 14.0), -2.0);
    let bank_c = lerp_rgb(rgba(154, 126, 86, 1.0), rgba(114, 88, 56, 1.0), ((p.y - 48.0) / 14.0).clamp(0.0, 1.0));
    col = over(col, [bank_c[0], bank_c[1], bank_c[2], cov(bank_d)]);

    // Calm pond water strip along the very bottom-left.
    let water_y = 54.5 + ((p.x * 0.22).sin()) * 0.8;
    let water_d = water_y - p.y;
    let water_c = lerp_rgb(rgba(78, 146, 184, 1.0), rgba(54, 114, 152, 1.0), ((p.y - 54.0) / 8.0).clamp(0.0, 1.0));
    col = over(col, [water_c[0], water_c[1], water_c[2], cov(water_d)]);
    let ripple_d = sd_seg(p, Vec2::new(9.0, 57.5), Vec2::new(26.0, 57.5), 0.7);
    col = over(col, rgba(196, 232, 246, cov(ripple_d) * 0.7));

    // 2. Tree on the right side (trunk + deep chewed V-notch + lush green canopy).
    let trunk_raw = sd_poly(
        p,
        &[
            Vec2::new(43.5, 16.0),
            Vec2::new(55.5, 16.0),
            Vec2::new(57.5, 57.5),
            Vec2::new(41.5, 57.5),
        ],
    );
    // Deep triangular notch chewed into the left side of the trunk right where the beaver bites.
    let notch_cut = sd_poly(
        p,
        &[
            Vec2::new(40.0, 32.5),
            Vec2::new(51.2, 39.2),
            Vec2::new(40.0, 46.2),
        ],
    );
    // Fresh exposed wood rim around the notch.
    let notch_wood = trunk_raw.max(notch_cut - 2.4);
    let trunk_d = trunk_raw.max(-notch_cut);

    // Subtle dark bark outline around trunk.
    let trunk_out = trunk_d - 1.1;
    col = over(col, rgba(58, 36, 20, cov(trunk_out)));

    // Bark fill with left-to-right cylindrical shading.
    let bark_t = ((p.x - 42.0) / 14.0).clamp(0.0, 1.0);
    let mut bark = lerp_rgb(rgba(148, 98, 56, 1.0), rgba(84, 52, 28, 1.0), bark_t);
    // Vertical bark texture streaks.
    let streak = ((p.x * 1.9 + (p.y * 0.12).sin() * 0.8).sin()).abs();
    if streak > 0.86 {
        bark = lerp_rgb(bark, rgba(64, 38, 20, 1.0), 0.35);
    }
    col = over(col, [bark[0], bark[1], bark[2], cov(trunk_d)]);

    // Exposed pale sapwood inside the chewed notch edge!
    let sap_d = notch_wood.max(-notch_cut);
    let sap = lerp_rgb(rgba(246, 224, 174, 1.0), rgba(214, 176, 118, 1.0), ((p.x - 42.0) / 10.0).clamp(0.0, 1.0));
    col = over(col, [sap[0], sap[1], sap[2], cov(sap_d)]);

    // Lush layered green tree crown at top-right.
    let crown_back = sd_circle(p, Vec2::new(50.0, 14.0), 13.0)
        .min(sd_circle(p, Vec2::new(38.0, 13.5), 9.5))
        .min(sd_circle(p, Vec2::new(58.0, 19.5), 8.5))
        .min(sd_circle(p, Vec2::new(45.0, 8.0), 9.0));
    col = over(col, rgba(44, 86, 36, cov(crown_back - 1.1)));
    let crown_shade = lerp_rgb(rgba(96, 162, 66, 1.0), rgba(56, 114, 44, 1.0), ((p.y - 4.0) / 22.0).clamp(0.0, 1.0));
    col = over(col, [crown_shade[0], crown_shade[1], crown_shade[2], cov(crown_back)]);
    let crown_hi = sd_circle(p, Vec2::new(43.5, 10.5), 6.2).min(sd_circle(p, Vec2::new(52.0, 12.5), 5.2));
    col = over(col, rgba(142, 198, 92, cov(crown_hi) * 0.72));

    // Flying wood chips popping out into the air from the chewed notch!
    let chip1 = sd_ellipse(p, Vec2::new(37.2, 22.8), Vec2::new(2.3, 0.95), -35.0);
    let chip2 = sd_ellipse(p, Vec2::new(41.8, 25.4), Vec2::new(1.9, 0.85), -58.0);
    let chip3 = sd_ellipse(p, Vec2::new(40.2, 48.8), Vec2::new(2.0, 0.9), 26.0);
    let chips = chip1.min(chip2).min(chip3);
    col = over(col, rgba(112, 74, 38, cov(chips - 0.65)));
    col = over(col, rgba(248, 228, 180, cov(chips)));

    // 3. Beaver in profile (left/center, facing right, gnawing the tree!).
    // 3a. Flat cross-hatched beaver tail trailing to bottom-left.
    let tail_d = sd_ellipse(p, Vec2::new(13.5, 49.5), Vec2::new(9.2, 4.1), 16.0);
    col = over(col, rgba(40, 28, 22, cov(tail_d - 1.1)));
    let mut tail_col = lerp_rgb(rgba(92, 72, 62, 1.0), rgba(62, 48, 42, 1.0), ((p.y - 45.0) / 9.0).clamp(0.0, 1.0));
    // Diamond cross-hatch pattern on the flat tail.
    let u = (p.x + p.y) * 0.7;
    let v = (p.x - p.y) * 0.7;
    if (u.sin().abs() > 0.88) || (v.sin().abs() > 0.88) {
        tail_col = lerp_rgb(tail_col, rgba(132, 108, 94, 1.0), 0.35);
    }
    col = over(col, [tail_col[0], tail_col[1], tail_col[2], cov(tail_d)]);

    // 3b. Plump arched beaver body + head + muzzle silhouette (profile facing right).
    let body_d = sd_ellipse(p, Vec2::new(25.0, 42.5), Vec2::new(11.8, 10.2), -24.0);
    let rump_d = sd_circle(p, Vec2::new(21.5, 46.0), 8.2);
    let chest_d = sd_ellipse(p, Vec2::new(32.5, 40.5), Vec2::new(6.8, 7.2), -12.0);
    let head_d = sd_ellipse(p, Vec2::new(33.2, 31.8), Vec2::new(7.4, 6.2), 8.0);
    let snout_d = sd_ellipse(p, Vec2::new(39.4, 34.4), Vec2::new(4.2, 3.0), 14.0);
    let ear_d = sd_circle(p, Vec2::new(28.0, 25.8), 2.7);
    let beaver_body = body_d.min(rump_d).min(chest_d).min(head_d).min(snout_d).min(ear_d);

    // Dark fur outline so the beaver reads crisply at 16px and 32px.
    col = over(col, rgba(54, 29, 14, cov(beaver_body - 1.3)));

    // Warm chestnut fur fill with top-left highlight and belly shadow.
    let fur_t = ((p.y - 24.0) / 28.0).clamp(0.0, 1.0);
    let fur_base = lerp_rgb(rgba(166, 104, 56, 1.0), rgba(108, 62, 30, 1.0), fur_t);
    col = over(col, [fur_base[0], fur_base[1], fur_base[2], cov(beaver_body)]);

    // Soft warm highlight along arched back & forehead.
    let back_hi = sd_ellipse(p, Vec2::new(24.0, 36.2), Vec2::new(7.2, 3.6), -32.0)
        .min(sd_ellipse(p, Vec2::new(32.8, 28.5), Vec2::new(4.2, 2.2), 10.0));
    col = over(col, rgba(196, 134, 78, cov(back_hi) * 0.52));

    // Lighter muzzle cheek patch.
    let cheek_d = sd_ellipse(p, Vec2::new(38.0, 35.4), Vec2::new(3.6, 2.3), 10.0);
    col = over(col, rgba(188, 136, 86, cov(cheek_d) * 0.65));

    // Inner ear dot.
    let ear_in = sd_circle(p, Vec2::new(28.3, 26.2), 1.35);
    col = over(col, rgba(96, 54, 28, cov(ear_in)));

    // Hind foot grounded on the bank.
    let foot_d = sd_ellipse(p, Vec2::new(26.5, 53.6), Vec2::new(5.6, 2.2), 4.0);
    col = over(col, rgba(44, 24, 12, cov(foot_d - 0.8)));
    col = over(col, rgba(84, 48, 26, cov(foot_d)));

    // Front paw braced against the tree trunk below the notch!
    let arm_d = sd_seg(p, Vec2::new(33.2, 44.2), Vec2::new(42.0, 45.6), 2.25);
    col = over(col, rgba(48, 26, 12, cov(arm_d - 0.8)));
    col = over(col, rgba(94, 54, 28, cov(arm_d)));

    // Open mouth / jaw cavity just behind the incisors.
    let mouth_d = sd_ellipse(p, Vec2::new(39.4, 39.2), Vec2::new(2.4, 2.0), 12.0);
    col = over(col, rgba(48, 24, 14, cov(mouth_d)));

    // Prominent curved amber-ivory beaver incisors biting into the tree notch!
    let tooth_d = sd_poly(
        p,
        &[
            Vec2::new(40.2, 36.4),
            Vec2::new(43.0, 36.9),
            Vec2::new(42.4, 41.3),
            Vec2::new(39.9, 40.6),
        ],
    );
    col = over(col, rgba(96, 54, 16, cov(tooth_d - 0.65)));
    let tooth_c = lerp_rgb(rgba(255, 248, 222, 1.0), rgba(244, 182, 66, 1.0), ((p.y - 36.4) / 4.8).clamp(0.0, 1.0));
    col = over(col, [tooth_c[0], tooth_c[1], tooth_c[2], cov(tooth_d)]);
    let tooth_line = sd_seg(p, Vec2::new(41.6, 36.8), Vec2::new(41.2, 40.9), 0.25);
    col = over(col, rgba(164, 108, 42, cov(tooth_line) * 0.7));

    // Nose tip at the front of the snout.
    let nose_d = sd_ellipse(p, Vec2::new(42.4, 34.4), Vec2::new(1.7, 1.4), 10.0);
    col = over(col, rgba(36, 22, 16, cov(nose_d)));

    // Bright alert eye with tiny white catchlight.
    let eye_d = sd_circle(p, Vec2::new(35.6, 30.8), 1.9);
    col = over(col, rgba(28, 18, 14, cov(eye_d)));
    let glint_d = sd_circle(p, Vec2::new(36.2, 30.2), 0.65);
    col = over(col, rgba(255, 255, 255, cov(glint_d)));

    // 4. Clean subtle badge rim so the icon pops on both light and dark taskbars.
    let rim_d = (badge_d + 1.0).abs() - 1.1;
    col = over(col, rgba(64, 42, 24, cov(rim_d) * 0.85));

    [col[0], col[1], col[2], col[3] * badge_a]
}
