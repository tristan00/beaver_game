use crate::colony::BeaverId;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Looks {
    pub shade: f32,
    pub warmth: f32,
    pub build: f32,
    pub size: f32,
    pub head: f32,
    pub tail: f32,
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Looks {
    pub fn drawn(key: u64) -> Looks {
        let mut z = mix(key);
        let mut next = || {
            z = mix(z);
            let a = (z >> 40) as f32 / (1u64 << 24) as f32;
            let b = ((z >> 16) & 0xFF_FFFF) as f32 / (1u64 << 24) as f32;
            a + b - 1.0
        };
        Looks { shade: next(), warmth: next(), build: next(), size: next(), head: next(), tail: next() }
    }

    pub fn of_kit(key: u64, mother: Option<Looks>, father: Option<Looks>) -> Looks {
        let own = Looks::drawn(key);
        let parents: Vec<Looks> = [mother, father].into_iter().flatten().collect();
        if parents.is_empty() {
            return own;
        }
        let n = parents.len() as f32;
        let mid = |f: fn(&Looks) -> f32| parents.iter().map(f).sum::<f32>() / n;
        let blend = |p: f32, o: f32| (0.7 * p + 0.45 * o).clamp(-1.0, 1.0);
        Looks {
            shade: blend(mid(|l| l.shade), own.shade),
            warmth: blend(mid(|l| l.warmth), own.warmth),
            build: blend(mid(|l| l.build), own.build),
            size: blend(mid(|l| l.size), own.size),
            head: blend(mid(|l| l.head), own.head),
            tail: blend(mid(|l| l.tail), own.tail),
        }
    }
}

pub fn key(id: BeaverId, time_days: f64, pos: (f32, f32)) -> u64 {
    mix(id.0 ^ mix(time_days.to_bits() ^ mix(((pos.0.to_bits() as u64) << 32) | pos.1.to_bits() as u64)))
}
