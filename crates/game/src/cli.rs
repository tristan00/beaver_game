//! Command-line parsing: `beaver_colony [--screenshot PATH --frames N] [--seed S] [--cam PRESET]`.
use bevy::prelude::Resource;
use std::path::PathBuf;

pub const USAGE: &str = "usage: beaver_colony [--seed S] [--cam iso|top|low|lodge] [--stress] [--screenshot out.png [--frames N]]";
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
}

impl Cli {
    /// Parses arguments (without the program name). Errors name the offending argument.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli { seed: 1, cam: "iso".into(), screenshot: None, frames: DEFAULT_FRAMES, stress: false };
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
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                other => return Err(format!("unknown argument '{other}'")),
            }
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
    fn rejects_bad_input() {
        assert!(p(&["--seed"]).is_err());
        assert!(p(&["--seed", "x"]).is_err());
        assert!(p(&["--frames", "0"]).is_err());
        assert!(p(&["--cam", "nope"]).is_err());
        assert!(p(&["--bogus"]).is_err());
    }
}

