//! Each beaver's own look: a few small differences (fur shade and warmth, build, size, head,
//! tail) drawn at random when it comes into the colony, so beavers can be told apart. A kit
//! takes after its parents, with some of its own. Kept for the dead too, and saved after the
//! world (see `save`).
use crate::colony::BeaverId;

/// Each from -1 to 1; 0 is the plain beaver model.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Looks {
    /// Dark (-1) to light (1) fur.
    pub shade: f32,
    /// Grey-brown (-1) to red-brown (1) fur.
    pub warmth: f32,
    /// Lean (-1) to stocky (1).
    pub build: f32,
    /// Small (-1) to big (1) for its age.
    pub size: f32,
    /// Small (-1) to big (1) head.
    pub head: f32,
    /// Narrow, short (-1) to broad, long (1) tail.
    pub tail: f32,
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Looks {
    /// Drawn at random from `key` (the beaver and when and where it came in).
    pub fn drawn(key: u64) -> Looks {
        let mut z = mix(key);
        let mut next = || {
            z = mix(z);
            // The sum of two halves leans towards the middle: most beavers look ordinary.
            let a = (z >> 40) as f32 / (1u64 << 24) as f32;
            let b = ((z >> 16) & 0xFF_FFFF) as f32 / (1u64 << 24) as f32;
            a + b - 1.0
        };
        Looks { shade: next(), warmth: next(), build: next(), size: next(), head: next(), tail: next() }
    }

    /// A kit's: the middle of its parents' (whichever are known), with some of its own.
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

/// The key a beaver's looks are drawn from.
pub fn key(id: BeaverId, time_days: f64, pos: (f32, f32)) -> u64 {
    mix(id.0 ^ mix(time_days.to_bits() ^ mix(((pos.0.to_bits() as u64) << 32) | pos.1.to_bits() as u64)))
}
