//! Command-line parsing: `beaver_colony [--screenshot PATH --frames N] [--seed S] [--cam PRESET]`.
use bevy::prelude::Resource;
use std::path::PathBuf;

pub const USAGE: &str = "usage: beaver_colony [--seed S] [--cam iso|top|low|lodge] [--demo-pond] [--stress] [--autoplay-settle] [--mode water|forest|food|work] [--open colony|stores|buildings|tech|events|build|gather|jobs|screens] [--demo-dam] [--demo-settle] [--demo-jobs] [--demo-orders drag] [--demo-place valid|invalid|dam] [--showcase poses|world] [--select beaver|family|building|site|dam|tree] [--screenshot out.png [--frames N]]";
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
    /// Scripted new game: order the designed dam, let the family build it, hold
    /// the picture until the pond has filled.
    pub demo_pond: bool,
    /// Scripted placement preview: hold a valid or invalid building ghost, or the dam hover preview.
    pub demo_place: Option<String>,
    /// Open this screen or toolbar menu at start (screenshots).
    pub open: Option<String>,
    /// Staged still scene for screenshots: `poses` or `world` (see `showcase.rs`).
    pub showcase: Option<String>,
    /// Scripted order gesture: `drag` sends a selected beaver to a dragged area.
    pub demo_orders: Option<String>,
    /// Open this entity panel at start (screenshots): beaver|family|building|site|dam|tree.
    pub select: Option<String>,
    /// Start in this map mode (screenshots): water|forest|food|work.
    pub mode: Option<String>,
    /// Settle choice with the first candidate picked (screenshots).
    pub demo_settle: bool,
    /// Settled family with two adults put on jobs, job panel open (screenshots).
    pub demo_jobs: bool,
}

impl Cli {
    /// Parses arguments (without the program name). Errors name the offending argument.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli { seed: 1, cam: "iso".into(), screenshot: None, frames: DEFAULT_FRAMES, stress: false, autoplay_settle: false, demo_pond: false, demo_dam: false, demo_place: None, open: None, showcase: None, demo_orders: None, select: None, mode: None, demo_settle: false, demo_jobs: false };
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
                "--demo-pond" => cli.demo_pond = true,
                "--demo-settle" => cli.demo_settle = true,
                "--demo-jobs" => cli.demo_jobs = true,
                "--demo-place" => {
                    let v = value("--demo-place")?;
                    if !["valid", "invalid", "dam"].contains(&v.as_str()) {
                        return Err(format!("--demo-place: unknown scene '{v}' (valid|invalid|dam)"));
                    }
                    cli.demo_place = Some(v);
                }
                "--demo-orders" => {
                    let v = value("--demo-orders")?;
                    if v != "drag" {
                        return Err(format!("--demo-orders: unknown scene '{v}' (drag)"));
                    }
                    cli.demo_orders = Some(v);
                }
                "--open" => {
                    let v = value("--open")?;
                    let known = crate::ui::shell::NavItem::ALL.iter().any(|n| n.label().eq_ignore_ascii_case(&v)) || crate::ui::shell::HudTool::ALL.iter().any(|t| t.off_reason().is_none() && t.label().eq_ignore_ascii_case(&v));
                    if !known {
                        return Err(format!("--open: unknown screen '{v}'"));
                    }
                    cli.open = Some(v);
                }
                "--showcase" => {
                    let v = value("--showcase")?;
                    if !["poses", "world"].contains(&v.as_str()) {
                        return Err(format!("--showcase: unknown scene '{v}' (poses|world)"));
                    }
                    cli.showcase = Some(v);
                }
                "--mode" => {
                    let v = value("--mode")?;
                    if crate::ui::mapmodes::MapMode::from_name(&v).is_none() {
                        return Err(format!("--mode: unknown mode '{v}' (water|forest|food|work)"));
                    }
                    cli.mode = Some(v);
                }
                "--select" => {
                    let v = value("--select")?;
                    if !["beaver", "family", "building", "site", "dam", "tree"].contains(&v.as_str()) {
                        return Err(format!("--select: unknown panel '{v}' (beaver|family|building|site|dam|tree)"));
                    }
                    cli.select = Some(v);
                }
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                other => return Err(format!("unknown argument '{other}'")),
            }
        }
        Ok(cli)
    }

    /// Whether a scripted scene (stress, autoplay, showcase) founds its own colony on a plain
    /// world instead of the new game.
    /// Whether the new game opens on the settle choice (every scripted scene that needs a colony
    /// settles the designed camp itself; `--demo-settle` picks a candidate for the screenshot).
    pub fn chooses_settle(&self) -> bool {
        let scripted = self.demo_pond || self.demo_place.is_some() || self.demo_orders.is_some() || self.demo_jobs || self.select.is_some() || self.open.is_some() || self.mode.is_some();
        self.demo_settle || !scripted
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
    fn demo_dam_implies_autoplay() {
        let c = p(&["--demo-dam"]).unwrap();
        assert!(c.demo_dam && c.autoplay_settle);
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
    fn open_takes_a_screen_or_a_toolbar_menu_and_work_is_no_screen() {
        for ok in ["stores", "buildings", "build", "jobs", "screens"] {
            assert!(p(&["--open", ok]).is_ok(), "{ok}");
        }
        assert!(p(&["--open", "work"]).is_err());
        assert!(p(&["--open", "haul"]).is_err(), "haul is off");
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

    #[test]
    fn demo_orders_needs_a_known_scene() {
        assert!(p(&["--demo-orders", "spin"]).is_err());
        assert!(p(&["--demo-orders", "resize"]).is_err());
        assert_eq!(p(&["--demo-orders", "drag"]).unwrap().demo_orders.as_deref(), Some("drag"));
    }
}
