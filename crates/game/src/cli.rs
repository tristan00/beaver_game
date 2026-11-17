//! Command-line parsing: `beaver_colony [--start N] [--screenshot PATH --frames N] [--seed S] [--cam PRESET]`.
use bevy::prelude::Resource;
use sim::starts::Start;
use std::path::PathBuf;

pub const USAGE: &str = "usage: beaver_colony [--seed S] [--cam iso|top|low|lodge] [--start 1|2|3 [--demo-pond]] [--stress] [--autoplay-settle] [--demo-dam] [--screenshot out.png [--frames N]]";
/// Minimum frames before capturing; the capture also waits until the terrain is renderable.
pub const DEFAULT_FRAMES: u32 = 10;

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Cli {
    pub seed: u64,
    pub cam: String,
    /// When set, wait until the terrain is drawn (and at least `frames` frames ran), save a PNG here and exit.
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    /// Render the 5000-tree / 300-beaver stress scene, log frame times, then exit (non-zero if any
    /// measured frame took 16 ms or more).
    pub stress: bool,
    /// Scripted opening: scout, settle at the best site, then let `--screenshot` capture.
    pub autoplay_settle: bool,
    /// Scripted dam preview (implies `--autoplay-settle`): settle, stock stores, draw a dam, hold the preview for `--screenshot`.
    pub demo_dam: bool,
    /// Build this start directly and skip the start-up menu.
    pub start: Option<Start>,
    /// Scripted start 1 (needs `--start 1`): order the designed dam, let the family build it, hold
    /// the picture until the pond has filled.
    pub demo_pond: bool,
}

impl Cli {
    /// Parses arguments (without the program name). Errors name the offending argument.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli { seed: 1, cam: "iso".into(), screenshot: None, frames: DEFAULT_FRAMES, stress: false, autoplay_settle: false, start: None, demo_pond: false, demo_dam: false };
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} requires a value"));
            match arg.as_str() {
                "--seed" => cli.seed = value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
                "--frames" => {
                    cli.frames = value("--frames")?.parse().map_err(|e| format!("--frames: {e}"))?;
                    if cli.frames == 0 {
                        return Err("--frames must be at least 1".into());
                    }
                }
                "--cam" => {
                    cli.cam = value("--cam")?;
                    if crate::camera::preset(&cli.cam).is_none() {
                        return Err(format!("--cam: unknown preset '{}'", cli.cam));
                    }
                }
                "--stress" => cli.stress = true,
                "--autoplay-settle" => cli.autoplay_settle = true,
                "--demo-dam" => {
                    cli.demo_dam = true;
                    cli.autoplay_settle = true;
                }
                "--start" => {
                    let v = value("--start")?;
                    let n: u8 = v.parse().map_err(|_| format!("--start: '{v}' is not a start number (1, 2 or 3)"))?;
                    cli.start = Some(Start::from_number(n).ok_or_else(|| format!("--start: no start {n} (choose 1, 2 or 3)"))?);
                }
                "--demo-pond" => cli.demo_pond = true,
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                other => return Err(format!("unknown argument '{other}'")),
            }
        }
        if cli.demo_pond && cli.start != Some(Start::River) {
            return Err("--demo-pond needs --start 1".into());
        }
        Ok(cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(a: &[&str]) -> Result<Cli, String> { Cli::parse(a.iter().map(|s| s.to_string())) }

    #[test]
    fn parses_all_flags() {
        let c = p(&["--screenshot", "o.png", "--frames", "5", "--seed", "9", "--cam", "top"]).unwrap();
        assert_eq!((c.seed, c.frames, c.cam.as_str()), (9, 5, "top"));
        assert_eq!(c.screenshot, Some("o.png".into()));
    }

    #[test]
    fn demo_dam_implies_autoplay() {
        let c = p(&["--demo-dam"]).unwrap();
        assert!(c.demo_dam && c.autoplay_settle);
    }

    #[test]
    fn start_flag_picks_a_start_and_rejects_bad_values() {
        assert_eq!(p(&[]).unwrap().start, None, "no flag: the menu opens");
        for (n, s) in [("1", Start::River), ("2", Start::EarlyLake), ("3", Start::IndustrialLake)] {
            assert_eq!(p(&["--start", n]).unwrap().start, Some(s));
        }
        for bad in ["0", "4", "256", "-1", "x", "1.5", ""] {
            assert!(p(&["--start", bad]).is_err(), "--start {bad:?} must be rejected");
        }
        assert!(p(&["--start"]).is_err(), "missing value");
    }

    #[test]
    fn demo_pond_needs_start_one() {
        assert!(p(&["--demo-pond"]).is_err());
        assert!(p(&["--start", "2", "--demo-pond"]).is_err());
        assert!(p(&["--start", "1", "--demo-pond"]).unwrap().demo_pond);
    }

    #[test]
    fn midgame_is_gone() {
        assert!(p(&["--midgame"]).unwrap_err().contains("--midgame"));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(p(&["--seed"]).is_err());
        assert!(p(&["--seed", "x"]).is_err());
        assert!(p(&["--frames", "0"]).is_err());
        assert!(p(&["--cam", "nope"]).is_err());
        assert!(p(&["--bogus"]).is_err());
    }
}

