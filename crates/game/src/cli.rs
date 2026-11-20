use bevy::prelude::Resource;
use std::path::PathBuf;

pub const USAGE: &str = "usage: beaver_colony [--map ID|generated] [--seed S] [--cam iso|top|low|lodge] [--stress] [--autoplay-settle] [--demo-pond] [--sandbox] [--sandbox-script FILE] [--script FILE [--report-dir DIR]] [--showcase poses|world|buildings] [--screenshot out.png [--frames N]]";
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
    pub sandbox: bool,
    pub sandbox_script: Option<PathBuf>,
    pub colony_script: Option<PathBuf>,
    pub report_dir: Option<PathBuf>,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli { seed: 1, map: None, cam: "iso".into(), screenshot: None, frames: DEFAULT_FRAMES, stress: false, autoplay_settle: false, demo_pond: false, showcase: None, sandbox: false, sandbox_script: None, colony_script: None, report_dir: None };
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
                "--sandbox" => cli.sandbox = true,
                "--sandbox-script" => {
                    cli.sandbox_script = Some(PathBuf::from(value("--sandbox-script")?));
                    cli.sandbox = true;
                }
                "--script" => cli.colony_script = Some(PathBuf::from(value("--script")?)),
                "--report-dir" => cli.report_dir = Some(PathBuf::from(value("--report-dir")?)),
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
        if cli.sandbox && (cli.demo_pond || cli.stress || cli.autoplay_settle || cli.showcase.is_some()) {
            return Err("--sandbox has no beavers and cannot run with --demo-pond, --stress, --autoplay-settle or --showcase".into());
        }
        if cli.colony_script.is_some() && (cli.sandbox || cli.plain_world() || cli.demo_pond) {
            return Err("--script cannot be combined with sandbox, stress or showcase modes".into());
        }
        if cli.report_dir.is_some() && (cli.colony_script.is_none() || cli.screenshot.is_some()) {
            return Err("--report-dir requires --script and cannot combine with --screenshot".into());
        }
        if cli.sandbox && cli.map.as_deref() == Some("generated") {
            return Err("--sandbox runs on the real maps only".into());
        }
        Ok(cli)
    }

    pub fn plain_world(&self) -> bool {
        self.stress || self.autoplay_settle || self.showcase.is_some()
    }
}

