//! Saving the game: Ctrl+S, the pause menu, once a game year, and on the way out.
//! Loading goes through the menu (`menu::start_load`), the same way a new valley comes in.
use crate::hud::{BAD, GOOD};
use crate::icons::Icon;
use crate::sim_driver::SimClock;
use crate::state::{CurrentMap, MapChoice, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;

pub const SAVE_DIR: &str = "saves";
const QUICK: &str = "quicksave.sav";
const AUTO: &str = "autosave.sav";
/// The game is autosaved once a game year, when the new year begins, and never sooner than
/// this many real seconds of unpaused play after the last autosave (at the fastest speed a
/// year goes by in a minute).
const AUTOSAVE_SECONDS: f32 = 300.0;

/// Folder for saves and small player preferences. Uses `./saves` when running inside a repo
/// or when a local `saves/` folder already exists; otherwise stores saves in the user's app
/// data folder so a standalone `.exe` works from any location (Desktop, Downloads, etc.).
pub fn save_dir() -> PathBuf {
    let local = PathBuf::from(SAVE_DIR);
    if local.is_dir() || Path::new("crates/sim/content").is_dir() {
        return local;
    }
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(base).join("Beaver Colony").join(SAVE_DIR);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Beaver Colony").join(SAVE_DIR);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(base).join("beaver-colony").join(SAVE_DIR);
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/share/beaver-colony").join(SAVE_DIR);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join(SAVE_DIR);
        }
    }
    local
}

pub fn quick_path() -> PathBuf {
    save_dir().join(QUICK)
}

fn auto_path() -> PathBuf {
    save_dir().join(AUTO)
}

/// The newest save there is, quick or auto.
pub fn latest_save() -> Option<PathBuf> {
    [quick_path(), auto_path()]
        .into_iter()
        .filter_map(|p| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (t, p)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Every save in the save folder, newest first, with its header (or why it cannot be read).
pub fn saves() -> Vec<(PathBuf, Result<sim::save::Header, String>)> {
    let Ok(dir) = std::fs::read_dir(save_dir()) else { return Vec::new() };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sav"))
        .filter_map(|p| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (t, p)))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| {
        let h = sim::save::peek(&p);
        (p, h)
    }).collect()
}

pub fn map_name(m: &MapChoice) -> String {
    match m {
        MapChoice::Real(id) => id.clone(),
        MapChoice::Generated(seed) => format!("generated:{seed}"),
    }
}

pub fn map_from(name: &str) -> MapChoice {
    match name.strip_prefix("generated:").and_then(|s| s.parse().ok()) {
        Some(seed) => MapChoice::Generated(seed),
        None => MapChoice::Real(name.to_string()),
    }
}

/// Asks for a save from anywhere in the interface (the pause menu sets it).
#[derive(Resource, Default)]
pub struct SaveRequest(pub bool);

#[derive(Resource, Default)]
struct SaveJob {
    running: Option<Mutex<Receiver<Result<(PathBuf, sim::save::Header), String>>>>,
    played: f32,
    /// The game year of the last autosave, or the one play began in.
    year: Option<u32>,
}

pub struct SaveGamePlugin;

impl Plugin for SaveGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveRequest>()
            .init_resource::<SaveJob>()
            .add_systems(Update, (save_keys, autosave, start_saves, finish_saves).chain())
            .add_systems(Last, save_on_exit);
    }
}

/// Scripted, screenshot and benchmark runs never touch the player's saves.
fn automated(cli: &crate::cli::Cli) -> bool {
    cli.screenshot.is_some() || cli.colony_script.is_some() || cli.report_dir.is_some() || std::env::var_os("BC_PERF").is_some()
}

fn playing(sim: &Sim) -> bool {
    sim.0.colony.is_founded() && !sim.0.sandbox && !sim.0.colony.is_collapsed()
}

fn save_keys(keys: Res<ButtonInput<KeyCode>>, mut req: ResMut<SaveRequest>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ctrl && keys.just_pressed(KeyCode::KeyS) {
        req.0 = true;
    }
}

fn autosave(time: Res<Time<Real>>, clock: Res<SimClock>, sim: Res<Sim>, current: Res<CurrentMap>, cli: Res<crate::cli::Cli>, mut job: ResMut<SaveJob>) {
    if clock.paused || !playing(&sim) || automated(&cli) {
        return;
    }
    job.played += time.delta_secs();
    let year = sim::calendar::Date::at(sim.0.time_days).year;
    let last = *job.year.get_or_insert(year);
    if year != last && job.played >= AUTOSAVE_SECONDS && job.running.is_none() {
        job.played = 0.0;
        job.year = Some(year);
        begin(&mut job, &sim, &current.0, auto_path());
    }
}

fn start_saves(mut req: ResMut<SaveRequest>, mut job: ResMut<SaveJob>, sim: Res<Sim>, current: Res<CurrentMap>, mut notices: ResMut<crate::hud::Notices>, time: Res<Time>, menu: Option<ResMut<crate::menu::Menu>>) {
    if !std::mem::take(&mut req.0) {
        return;
    }
    if !playing(&sim) {
        notices.push(vec![(Icon::Cross, BAD)], "Nothing to save yet", time.elapsed_secs());
        if let Some(mut m) = menu.filter(|m| m.saved.is_some()) {
            m.saved = Some(crate::menu::SaveNote::Failed("Nothing to save yet".into()));
        }
        return;
    }
    if job.running.is_some() {
        return;
    }
    begin(&mut job, &sim, &current.0, quick_path());
}

/// Copy the world and write it on another thread, so play does not stutter.
fn begin(job: &mut SaveJob, sim: &Sim, map: &MapChoice, path: PathBuf) {
    let world = sim.0.clone();
    let name = map_name(map);
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let _ = send.send(sim::save::save(&world, &name, &path).map(|h| (path, h)));
    });
    job.running = Some(Mutex::new(receive));
}

fn finish_saves(mut job: ResMut<SaveJob>, mut notices: ResMut<crate::hud::Notices>, time: Res<Time>, mut menu: Option<ResMut<crate::menu::Menu>>) {
    let Some(rx) = &job.running else { return };
    let result = match rx.lock().unwrap().try_recv() {
        Ok(r) => r,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(_) => Err("the save was interrupted".into()),
    };
    job.running = None;
    match result {
        Ok((path, h)) => {
            sim::trace::event(format!("SAVED path={} day={:.2} map={} population={}", path.display(), h.day, h.map, h.population));
            if path == quick_path() {
                notices.push(vec![(Icon::Check, GOOD)], "Saved", time.elapsed_secs());
                // The pause menu covers the journal: say so on its Save button too.
                if let Some(m) = menu.as_mut().filter(|m| m.saved.is_some()) {
                    m.saved = Some(crate::menu::SaveNote::Saved);
                }
            }
        }
        Err(e) => {
            sim::trace::event(format!("SAVE_FAILED {e}"));
            notices.push(vec![(Icon::Cross, BAD)], format!("Not saved: {e}"), time.elapsed_secs());
            if let Some(m) = menu.as_mut().filter(|m| m.saved.is_some()) {
                m.saved = Some(crate::menu::SaveNote::Failed(format!("Not saved: {e}")));
            }
        }
    }
}

/// On the way out, write the autosave there and then, so nothing played is lost.
fn save_on_exit(mut exits: MessageReader<AppExit>, sim: Res<Sim>, current: Res<CurrentMap>, cli: Res<crate::cli::Cli>) {
    if exits.read().next().is_none() || !playing(&sim) || automated(&cli) {
        return;
    }
    match sim::save::save(&sim.0, &map_name(&current.0), &auto_path()) {
        Ok(h) => sim::trace::event(format!("SAVED_ON_EXIT day={:.2}", h.day)),
        Err(e) => sim::trace::event(format!("SAVE_FAILED on exit: {e}")),
    }
    sim::trace::flush();
}
