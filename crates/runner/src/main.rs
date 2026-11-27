use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

struct Args {
    map: String,
    policy: PathBuf,
    policy_args: Vec<String>,
    days: f64,
    speed: f64,
    log: Option<PathBuf>,
    save_end: Option<PathBuf>,
    goal: runner::Goal,
}

const USAGE: &str = "usage: runner --map ID --policy PROGRAM [--days N] [--speed DAYS_PER_SECOND] [--score wet|beavers|blend] [--log FILE] [--save-end FILE] [-- POLICY ARGS]";

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut map, mut policy, mut policy_args) = (None, None, Vec::new());
    let (mut days, mut speed, mut log, mut save_end) = (runner::DEFAULT_DAYS, runner::DEFAULT_SPEED, None, None);
    let mut goal = runner::Goal::Wet;
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--map" => map = Some(value("--map")?),
            "--policy" => policy = Some(PathBuf::from(value("--policy")?)),
            "--days" => days = value("--days")?.parse().map_err(|e| format!("--days: {e}"))?,
            "--speed" => speed = value("--speed")?.parse().map_err(|e| format!("--speed: {e}"))?,
            "--log" => log = Some(PathBuf::from(value("--log")?)),
            "--save-end" => save_end = Some(PathBuf::from(value("--save-end")?)),
            "--score" => goal = runner::Goal::parse(&value("--score")?).ok_or("--score is wet, beavers or blend")?,
            "--" => policy_args.extend(it.by_ref()),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    if !(speed > 0.0 && days > 0.0) {
        return Err("--days and --speed must be above 0".into());
    }
    Ok(Args { map: map.ok_or("--map is required")?, policy: policy.ok_or("--policy is required")?, policy_args, days, speed, log, save_end, goal })
}

struct Mailbox {
    slot: Mutex<Option<sim::SimState>>,
    filled: Condvar,
    wanted: AtomicBool,
    over: AtomicBool,
}

type Log = Arc<Mutex<Box<dyn Write + Send>>>;

fn note(log: &Log, line: String) {
    if let Ok(mut l) = log.lock() {
        let _ = writeln!(l, "{line}");
    }
}

fn main() {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let (mut s, site) = sim::starts::new_game_on_map(&a.map).unwrap_or_else(|e| panic!("map {}: {e:?}", a.map));
    s.apply(sim::Command::Settle { cell: site.camp }).expect("settle at the designed start");
    let start_wet = runner::wet_cells(&s.water.depth);

    let (log, policy_err): (Log, Stdio) = match &a.log {
        Some(path) => {
            let f = std::fs::File::create(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let err = Stdio::from(f.try_clone().expect("log file"));
            (Arc::new(Mutex::new(Box::new(f))), err)
        }
        None => (Arc::new(Mutex::new(Box::new(std::io::stderr()))), Stdio::inherit()),
    };
    let mut child = Process::new(&a.policy)
        .args(&a.policy_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(policy_err)
        .spawn()
        .unwrap_or_else(|e| panic!("could not start {}: {e}", a.policy.display()));

    let mail = Arc::new(Mailbox { slot: Mutex::new(None), filled: Condvar::new(), wanted: AtomicBool::new(false), over: AtomicBool::new(false) });
    let (spare_tx, spare_rx) = mpsc::channel::<sim::SimState>();
    let sent = Arc::new(AtomicU64::new(0));
    let sender = {
        let (mail, sent, map) = (mail.clone(), sent.clone(), a.map.clone());
        let mut to_policy = child.stdin.take().expect("policy stdin");
        std::thread::spawn(move || loop {
            mail.wanted.store(true, Ordering::Release);
            let state = {
                let mut slot = mail.slot.lock().unwrap();
                loop {
                    if mail.over.load(Ordering::Acquire) {
                        return;
                    }
                    if let Some(st) = slot.take() {
                        break st;
                    }
                    slot = mail.filled.wait(slot).unwrap();
                }
            };
            let bytes = sim::save::encode(&state, &map).expect("encode state");
            let _ = spare_tx.send(state);
            if runner::write_frame(&mut to_policy, &bytes).is_err() {
                return;
            }
            sent.fetch_add(1, Ordering::Relaxed);
        })
    };

    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    let reader = {
        let out = child.stdout.take().expect("policy stdout");
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                let line = line.trim().to_string();
                if !line.is_empty() && !line.starts_with('#') && cmd_tx.send(line).is_err() {
                    break;
                }
            }
        })
    };

    let step_real = Duration::from_secs_f64(sim::STEP_DAYS as f64 / a.speed);
    let start_day = s.time_days;
    let begun = Instant::now();
    let mut due = begun;
    let (mut applied, mut rejected, mut behind) = (0u64, 0u64, Duration::ZERO);
    let window_from = (a.days - runner::SPEED_WINDOW_DAYS).max(0.0);
    let (mut window_days, mut window_busy) = (0.0f64, Duration::ZERO);
    note(&log, format!("runner: map={} days={} speed={} policy={}", a.map, a.days, a.speed, a.policy.display()));
    while s.time_days - start_day < a.days - 1e-9 {
        let busy_from = Instant::now();
        let in_window = s.time_days - start_day >= window_from - 1e-9;
        while let Ok(line) = cmd_rx.try_recv() {
            let result = sim::script::parse_command(&line).and_then(|c| {
                if !c.is_player_command() { Err("not a player command".to_string()) } else { s.apply(c).map_err(|e| e.to_string()) }
            });
            match result {
                Ok(()) => {
                    applied += 1;
                    note(&log, format!("day {:.3} ok: {line}", s.time_days));
                }
                Err(e) => {
                    rejected += 1;
                    note(&log, format!("day {:.3} rejected: {line} ({e})", s.time_days));
                }
            }
        }
        let before = s.time_days;
        if let Err(e) = s.step(sim::STEP_DAYS) {
            note(&log, format!("day {:.3} the simulation stopped: {e}", s.time_days));
            break;
        }
        if in_window {
            window_days += s.time_days - before;
            window_busy += busy_from.elapsed();
        }
        if mail.wanted.swap(false, Ordering::AcqRel) {
            let mut slot = mail.slot.lock().unwrap();
            let copy = match spare_rx.try_recv() {
                Ok(mut old) => {
                    old.clone_from(&s);
                    old
                }
                Err(_) => s.clone(),
            };
            *slot = Some(copy);
            mail.filled.notify_one();
        }
        due += step_real;
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        } else {
            behind += now - due;
            due = now;
        }
    }
    let elapsed = begun.elapsed().as_secs_f64();

    {
        let _slot = mail.slot.lock().unwrap();
        mail.over.store(true, Ordering::Release);
    }
    mail.filled.notify_all();
    let policy = match child.try_wait() {
        Ok(Some(st)) => format!("exited({})", st.code().map_or("?".into(), |c| c.to_string())),
        _ => "ran".to_string(),
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    let _ = sender.join();
    if let Some(path) = &a.save_end {
        if let Err(e) = sim::save::save(&s, &a.map, path) {
            note(&log, format!("could not save {}: {e}", path.display()));
        }
    }
    let c = s.census();
    let dams = s.buildings.buildings.iter().filter(|b| b.dam.is_some()).count();
    let days = s.time_days - start_day;
    let speed = window_days / window_busy.as_secs_f64().max(1e-9);
    let result = format!(
        "map={} goal={:?} score={} wet_start={} wet_end={} speed={:.2} days={:.1} beavers={} dams={} commands={} rejected={} states_sent={} pace={:.2} behind_s={:.1} policy={}",
        a.map,
        a.goal,
        a.goal.score(start_wet, &s, speed),
        start_wet,
        runner::wet_cells(&s.water.depth),
        speed,
        days,
        c.beavers,
        dams,
        applied,
        rejected,
        sent.load(Ordering::Relaxed),
        days / elapsed.max(1e-9),
        behind.as_secs_f64(),
        policy,
    );
    note(&log, result.clone());
    println!("{result}");
}
