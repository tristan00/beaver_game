use bevy::prelude::Resource;
use std::path::PathBuf;

pub const USAGE: &str = "usage: beaver_colony [--map ID|generated] [--seed S] [--cam iso|top|low|lodge] [--stress] [--autoplay-settle] [--demo-pond] [--showcase poses|world|buildings] [--screenshot out.png [--frames N]]";
pub const DEFAULT_FRAMES: u32 = 10;

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Cli {
    pub seed: u64,
    pub map: Option<String>,
    pub cam: String,
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    pub stress: bool,
    pub autoplay_settle: bool,
    pub demo_pond: bool,
    pub showcase: Option<String>,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli { seed: 1, map: None, cam: "iso".into(), screenshot: None, frames: DEFAULT_FRAMES, stress: false, autoplay_settle: false, demo_pond: false, showcase: None };
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} requires a value"));
            match arg.as_str() {
                "--seed" => cli.seed = value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
                "--map" => {
                    let m = value("--map")?;
                    if m != "generated" && !sim::maps::all().iter().any(|i| i.id == m) {
                        let ids: Vec<String> = sim::maps::all().into_iter().map(|i| i.id).collect();
                        return Err(format!("--map: unknown map '{m}' (maps: {}, generated)", ids.join(", ")));
                    }
                    cli.map = Some(m);
                }
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
                "--demo-pond" => cli.demo_pond = true,
                "--showcase" => {
                    let v = value("--showcase")?;
                    if !["poses", "world", "buildings"].contains(&v.as_str()) {
                        return Err(format!("--showcase: unknown scene '{v}' (poses|world|buildings)"));
                    }
                    cli.showcase = Some(v);
                }
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                other => return Err(format!("unknown argument '{other}'")),
            }
        }
        Ok(cli)
    }

    pub fn plain_world(&self) -> bool {
        self.stress || self.autoplay_settle || self.showcase.is_some()
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
    fn the_start_flag_and_menu_are_gone() {
        assert!(p(&["--start", "1"]).unwrap_err().contains("--start"));
        assert!(p(&["--start", "2"]).is_err());
    }

    #[test]
    fn scripted_scenes_use_a_plain_world_and_everything_else_the_new_game() {
        assert!(!p(&[]).unwrap().plain_world());
        assert!(!p(&["--demo-pond"]).unwrap().plain_world());
        for a in [["--stress", ""], ["--autoplay-settle", ""], ["--showcase", "poses"]] {
            let args: Vec<&str> = a.into_iter().filter(|s| !s.is_empty()).collect();
            assert!(p(&args).unwrap().plain_world(), "{args:?}");
        }
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
