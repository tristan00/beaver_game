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
        let choice = match cli.map.as_deref() {
            Some("generated") => MapChoice::Generated(seed),
            Some(id) => MapChoice::Real(id.to_string()),
            None => MapChoice::first(),
        };
        let (sim, dam) = if cli.plain_world() {
            (SimState::new(seed), None)
        } else {
            choice.build().map(|(s, d)| (s, Some(d))).unwrap_or_else(|e| {
                eprintln!("error: cannot build the new game: {e}");
                std::process::exit(1);
            })
        };
        app.insert_resource(Sim(sim)).insert_resource(PlannedDam(dam)).insert_resource(CurrentMap(choice));
    }
}
