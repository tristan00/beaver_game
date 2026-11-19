#![allow(dead_code)]
mod audio {
    #[path = "../../src/audio/mix.rs"]
    pub mod mix;
    #[path = "../../src/audio/synth.rs"]
    pub mod synth;
}

use audio::mix::{self, StubWorld, VolumeSettings, YEAR_DAYS};
use audio::synth::{Mix, SAMPLE_RATE, Synth};
use std::io::Write;

const SECONDS: usize = 20;

fn write_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&SAMPLE_RATE.to_le_bytes())?;
    f.write_all(&(SAMPLE_RATE * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    f.flush()
}

fn main() -> std::io::Result<()> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "target/audio_dump".into()));
    std::fs::create_dir_all(&dir)?;
    let scenes = [("summer_pond", 0.375, 0.0, 40.0), ("winter", 0.875, 0.0, 40.0)];
    for (name, year, day, distance) in scenes {
        let world = StubWorld {
            days: YEAR_DAYS * year + day,
            pond: Some((0.0, 0.0, 16.0)),
            forest: Some((26.0, 8.0, 14.0)),
            sites: vec![(22.0, -4.0)],
        };
        let snap = mix::snapshot(&world, 6.0, 2.0, distance);
        let layers = mix::layer_volumes(&snap);
        let mix = Mix { layers, settings: VolumeSettings::default() };
        let mut synth = Synth::new(7);
        let mut out = vec![0.0f32; SECONDS * SAMPLE_RATE as usize * 2];
        for chunk in out.chunks_mut(2048) {
            synth.render(&mix, chunk);
        }
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        let peak = out.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, &out)?;
        println!("{}: {} s, rms {:.3}, peak {:.3}  layers {:?}", path.display(), SECONDS, rms, peak, layers);
    }
    Ok(())
}
