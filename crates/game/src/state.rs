use bevy::prelude::*;
use sim::starts::DamSite;
use sim::SimState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapChoice {
    Real(String),
    Generated(u64),
}

impl MapChoice {
    pub fn first() -> MapChoice {
        sim::maps::all().first().map_or(MapChoice::Generated(1), |m| MapChoice::Real(m.id.clone()))
    }

    pub fn build(&self) -> Result<(SimState, DamSite), String> {
        match self {
            MapChoice::Real(id) => sim::starts::new_game_on_map(id).map_err(|e| format!("map {id}: {e}")),
            MapChoice::Generated(seed) => sim::starts::new_game_with_dam(*seed).map_err(|e| format!("seed {seed}: {e}")),
        }
    }

    pub fn sandbox(&self) -> Result<SimState, String> {
        match self {
            MapChoice::Real(id) => sim::sandbox::on_map(id),
            MapChoice::Generated(_) => Err("the sandbox runs on the real maps only".into()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            MapChoice::Real(id) => sim::maps::all().into_iter().find(|m| &m.id == id).map_or(id.clone(), |m| format!("{}, {}", m.name, m.place)),
            MapChoice::Generated(seed) => format!("Generated valley, seed {seed}"),
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct CurrentMap(pub MapChoice);

#[derive(Resource)]
pub struct Sim(pub SimState);

#[derive(Resource, Default)]
pub struct PlannedDam(pub Option<DamSite>);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<crate::cli::Cli>();
        let seed = cli.seed;
        if let Some(path) = &cli.colony_script {
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("script {}: {e}", path.display()));
            let root = path.parent().unwrap_or(std::path::Path::new("."));
            let (out, sim, dam) = sim::script::run_to_state(&text, &|p| std::fs::read_to_string(root.join(p)).map_err(|e| e.to_string()))
                .unwrap_or_else(|e| panic!("script {}: {e}", path.display()));
            for line in &out.lines { println!("script: {line}"); }
            assert!(out.passed(), "colony script checks failed: {:?}", out.checks);
            if let Some(dir) = &cli.report_dir {
                crate::report_capture::write_audit(dir, &sim, &out).expect("write scripted colony audit");
            }
            let choice = text.lines().find_map(|l| l.trim().strip_prefix("map ")).map_or(MapChoice::Generated(sim.seed), |s| MapChoice::Real(s.trim().into()));
            app.insert_resource(Sim(sim)).insert_resource(PlannedDam(dam)).insert_resource(CurrentMap(choice));
            return;
        }
        if let Some(path) = &cli.load {
            let (header, sim) = sim::save::load(path).unwrap_or_else(|e| {
                eprintln!("error: cannot load {}: {e}", path.display());
                std::process::exit(1);
            });
            let choice = crate::savegame::map_from(&header.map);
            app.insert_resource(Sim(sim)).insert_resource(PlannedDam(None)).insert_resource(CurrentMap(choice));
            return;
        }
        let choice = match cli.map.as_deref() {
            Some("generated") => MapChoice::Generated(seed),
            Some(id) => MapChoice::Real(id.to_string()),
            None => MapChoice::first(),
        };
        let script = cli.sandbox_script.clone().map(|p| run_sandbox_script(&p));
        let choice = script.as_ref().map_or(choice, |(_, map)| MapChoice::Real(map.clone()));
        let (sim, dam) = if let Some((s, _)) = script {
            (s, None)
        } else if cli.sandbox {
            (choice.sandbox().unwrap_or_else(|e| {
                eprintln!("error: cannot build the sandbox: {e}");
                std::process::exit(1);
            }), None)
        } else {
            choice.build().map(|(s, d)| (s, Some(d))).unwrap_or_else(|e| {
                eprintln!("error: cannot build the new game: {e}");
                std::process::exit(1);
            })
        };
        app.insert_resource(Sim(sim)).insert_resource(PlannedDam(dam)).insert_resource(CurrentMap(choice));
    }
}

fn run_sandbox_script(path: &std::path::Path) -> (SimState, String) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read sandbox script {}: {e}", path.display());
        std::process::exit(2);
    });
    match sim::sandbox::run_script(&text) {
        Ok((s, report)) => {
            for l in &report.lines {
                println!("sandbox: {l}");
            }
            let failed = report.checks.iter().filter(|c| !c.1).count();
            println!("sandbox: {} checks, {failed} failed", report.checks.len());
            (s, report.map)
        }
        Err(e) => {
            eprintln!("error: sandbox script {}: {e}", path.display());
            std::process::exit(2);
        }
    }
}
