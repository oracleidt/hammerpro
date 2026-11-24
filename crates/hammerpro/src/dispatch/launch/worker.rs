use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use hp_app::runtime::GameState;
use hp_compile::launch::process::{self, GameProcess};

use super::bridge::{self, GameProp};

const WATCH_INTERVAL: Duration = Duration::from_secs(5);
const WAIT_INTERVAL: Duration = Duration::from_millis(250);
const WINDOW_TIMEOUT: Duration = Duration::from_secs(180);
const LOAD_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoadRequest {
    pub root: PathBuf,
    pub map: String,
    pub args: Vec<String>,
    pub reuse: bool,
    pub place: Option<([f64; 3], [f64; 3])>,
    pub measure: bool,
    pub exe: Option<PathBuf>,
}

struct Resolved {
    request: LoadRequest,
    exe: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cmd {
    Watch(Option<PathBuf>),
    Load(LoadRequest),
    Boot { root: PathBuf, args: Vec<String>, exe: Option<PathBuf> },
    Place { root: PathBuf, map: String, pos: [f64; 3], ang: [f64; 3] },
    Bridge { root: PathBuf, install: bool },
    DumpProps { root: PathBuf },
    Measure { root: PathBuf, map: String },
    LoadBuilt { request: LoadRequest, since: Option<SystemTime> },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    State(GameState),
    Started { map: Option<String> },
    Sent { map: String },
    Waiting { map: String },
    NoBsp { map: String },
    NoExe,
    WindowTimeout { map: String },
    Failed(String),
    Bridge(bool),
    BridgeFailed(String),
    Props(Vec<GameProp>),
    PropsTimeout,
    Loaded { map: String, total: Duration, scripts: Option<Duration> },
}

struct Measure {
    root: PathBuf,
    map: String,
    since: Instant,
    before: Option<SystemTime>,
}

struct Waiting {
    load: Resolved,
    since: Instant,
}

struct Worker {
    root: Option<PathBuf>,
    last: Option<GameState>,
    waiting: Option<Waiting>,
    measure: Option<Measure>,
    events: Sender<Event>,
    ctx: Option<egui::Context>,
}

pub(crate) fn spawn(ctx: Option<egui::Context>) -> Option<(Sender<Cmd>, Receiver<Event>)> {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (event_tx, event_rx) = crossbeam_channel::unbounded();
    let mut worker = Worker { root: None, last: None, waiting: None, measure: None, events: event_tx, ctx };
    std::thread::Builder::new()
        .name("hp-game".to_owned())
        .spawn(move || worker.run(&cmd_rx))
        .map_err(|e| log::warn!("cannot start the game worker: {e}"))
        .ok()?;
    Some((cmd_tx, event_rx))
}

fn state_of(game: Option<&GameProcess>) -> GameState {
    match game {
        None => GameState::NotRunning,
        Some(g) if g.is_ready() => GameState::Ready,
        Some(_) => GameState::Starting,
    }
}

impl Worker {
    fn run(&mut self, commands: &Receiver<Cmd>) {
        loop {
            let busy = self.waiting.is_some() || self.measure.is_some();
            let interval = if busy { WAIT_INTERVAL } else { WATCH_INTERVAL };
            match commands.recv_timeout(interval) {
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.check();
        }
    }

    fn send(&self, event: Event) {
        if self.events.send(event).is_ok()
            && let Some(ctx) = &self.ctx
        {
            ctx.request_repaint();
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Watch(root) => {
                if self.root != root {
                    self.root = root;
                    self.last = None;
                    let installed = self.root.as_deref().is_some_and(bridge::is_installed);
                    self.send(Event::Bridge(installed));
                }
            }
            Cmd::Place { root, map, pos, ang } => self.place(&root, &map, pos, ang),
            Cmd::Bridge { root, install } => {
                let result = if install { bridge::install(&root) } else { bridge::remove(&root) };
                match result {
                    Ok(()) => self.send(Event::Bridge(bridge::is_installed(&root))),
                    Err(e) => self.send(Event::BridgeFailed(e.to_string())),
                }
            }
            Cmd::DumpProps { root } => self.dump_props(&root),
            Cmd::Measure { root, map } => self.start_measure(&root, &map),
            Cmd::LoadBuilt { request, since } => {
                let bsp = request.root.join("garrysmod").join("maps").join(format!("{}.bsp", request.map));
                let written = std::fs::metadata(&bsp).and_then(|m| m.modified()).ok();
                let fresh = match (written, since) {
                    (Some(w), Some(s)) => w >= s,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                if fresh {
                    self.load(request);
                }
            }
            Cmd::Load(request) => self.load(request),
            Cmd::Boot { root, args, exe } => {
                if process::find_game(&root).is_some() {
                    return;
                }
                let Some(exe) = game_exe(&root, exe.as_deref()) else {
                    self.send(Event::NoExe);
                    return;
                };
                match hp_compile::launch::spawn_game(&exe, &args) {
                    Ok(_) => self.send(Event::Started { map: None }),
                    Err(e) => self.send(Event::Failed(e.to_string())),
                }
            }
        }
    }

    fn start_measure(&mut self, root: &Path, map: &str) {
        if bridge::is_installed(root) {
            let before = bridge::ack_time(root);
            self.measure =
                Some(Measure { root: root.to_path_buf(), map: map.to_owned(), since: Instant::now(), before });
        }
    }

    fn check_measure(&mut self) {
        let Some(m) = &self.measure else { return };
        if m.since.elapsed() > LOAD_TIMEOUT {
            self.measure = None;
            return;
        }
        let Some(ack) = bridge::read_ack(&m.root, m.before) else { return };
        if !ack.spawned || !ack.map.eq_ignore_ascii_case(&m.map) {
            if let Some(m) = &mut self.measure {
                m.before = bridge::ack_time(&m.root);
            }
            return;
        }
        let Some(m) = self.measure.take() else { return };
        let total = m.since.elapsed();
        let scripts = ack.lua_ms.map(Duration::from_millis).filter(|s| *s <= total);
        self.send(Event::Loaded { map: m.map, total, scripts });
    }

    fn check(&mut self) {
        self.check_measure();
        let game = self.root.as_deref().and_then(process::find_game);
        let state = if self.root.is_some() { state_of(game.as_ref()) } else { GameState::Unknown };
        if self.last != Some(state) {
            self.last = Some(state);
            self.send(Event::State(state));
        }
        let Some(waiting) = &self.waiting else { return };
        let root = waiting.load.request.root.clone();
        let game = if self.root.as_deref() == Some(root.as_path()) { game } else { process::find_game(&root) };
        match game {
            Some(g) if g.is_ready() => {
                let Some(waiting) = self.waiting.take() else { return };
                self.send_map(&g, &waiting.load);
            }
            Some(_) if waiting.since.elapsed() < WINDOW_TIMEOUT => {}
            Some(_) => {
                let map = waiting.load.request.map.clone();
                self.waiting = None;
                self.send(Event::WindowTimeout { map });
            }
            None => {
                let Some(waiting) = self.waiting.take() else { return };
                self.spawn_map(&waiting.load);
            }
        }
    }

    fn place(&self, root: &Path, map: &str, pos: [f64; 3], ang: [f64; 3]) {
        if !bridge::is_installed(root) {
            return;
        }
        let cmd = bridge::Command {
            seq: bridge::next_seq(),
            map: Some(map.to_owned()),
            setpos: Some(pos),
            setang: Some(ang),
            dump_props: false,
        };
        if let Err(e) = bridge::send(root, &cmd) {
            log::warn!("cannot write the bridge command: {e}");
        }
    }

    fn dump_props(&self, root: &Path) {
        let before = bridge::props_time(root);
        let cmd = bridge::Command { seq: bridge::next_seq(), dump_props: true, ..bridge::Command::default() };
        if bridge::send(root, &cmd).is_err() {
            self.send(Event::PropsTimeout);
            return;
        }
        let start = Instant::now();
        while start.elapsed() < bridge::PROPS_TIMEOUT {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(props) = bridge::read_props(root, before) {
                self.send(Event::Props(props));
                return;
            }
        }
        self.send(Event::PropsTimeout);
    }

    fn load(&mut self, request: LoadRequest) {
        let bsp = request.root.join("garrysmod").join("maps").join(format!("{}.bsp", request.map));
        if !bsp.is_file() {
            self.send(Event::NoBsp { map: request.map });
            return;
        }
        let Some(exe) = game_exe(&request.root, request.exe.as_deref()) else {
            self.send(Event::NoExe);
            return;
        };
        if let Some((pos, ang)) = request.place {
            self.place(&request.root, &request.map, pos, ang);
        }
        if request.measure {
            self.start_measure(&request.root, &request.map);
        }
        let game = if request.reuse { process::find_game(&request.root) } else { None };
        let load = Resolved { request, exe };
        match game {
            Some(g) if g.is_ready() => self.send_map(&g, &load),
            Some(_) => {
                self.send(Event::Waiting { map: load.request.map.clone() });
                self.waiting = Some(Waiting { load, since: Instant::now() });
            }
            None => self.spawn_map(&load),
        }
    }

    fn send_map(&self, game: &GameProcess, load: &Resolved) {
        let line = process::hijack_command_line(&load.exe, &load.request.args);
        match process::send_command_line(game, &line) {
            Ok(()) => self.send(Event::Sent { map: load.request.map.clone() }),
            Err(e) => self.send(Event::Failed(e)),
        }
    }

    fn spawn_map(&self, load: &Resolved) {
        match hp_compile::launch::spawn_game(&load.exe, &load.request.args) {
            Ok(_) => self.send(Event::Started { map: Some(load.request.map.clone()) }),
            Err(e) => self.send(Event::Failed(e.to_string())),
        }
    }
}

pub(crate) fn game_exe(root: &Path, chosen: Option<&Path>) -> Option<PathBuf> {
    if let Some(exe) = chosen.filter(|p| p.is_absolute() && p.is_file()) {
        return Some(exe.to_path_buf());
    }
    hp_compile::ToolPaths::detect(root).and_then(|t| t.game_exe).or_else(|| {
        [root.join("bin").join("win64").join("gmod.exe"), root.join("gmod.exe"), root.join("hl2.exe")]
            .into_iter()
            .find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hammerpro_game_worker_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn next(rx: &Receiver<Event>) -> Event {
        rx.recv_timeout(Duration::from_secs(10)).expect("an event")
    }

    #[test]
    fn the_worker_reports_the_game_and_refuses_missing_maps() {
        let dir = scratch("watch");
        let (tx, rx) = spawn(None).expect("worker");
        tx.send(Cmd::Watch(Some(dir.clone()))).expect("send");
        assert_eq!(next(&rx), Event::Bridge(false));
        assert_eq!(next(&rx), Event::State(GameState::NotRunning), "no game in an empty folder");
        let request = LoadRequest {
            root: dir.clone(),
            map: "gm_test".to_owned(),
            args: vec!["+map".to_owned(), "gm_test".to_owned()],
            reuse: true,
            place: None,
            measure: false,
            exe: None,
        };
        tx.send(Cmd::Load(request.clone())).expect("send");
        assert_eq!(next(&rx), Event::NoBsp { map: "gm_test".to_owned() });
        std::fs::create_dir_all(dir.join("garrysmod/maps")).expect("maps");
        std::fs::write(dir.join("garrysmod/maps/gm_test.bsp"), b"VBSP").expect("bsp");
        tx.send(Cmd::Load(request.clone())).expect("send");
        assert_eq!(next(&rx), Event::NoExe);
        tx.send(Cmd::Boot { root: dir.clone(), args: Vec::new(), exe: None }).expect("send");
        assert_eq!(next(&rx), Event::NoExe);
        std::fs::write(dir.join("gmod.exe"), b"not a program").expect("exe");
        tx.send(Cmd::Load(request)).expect("send");
        assert!(matches!(next(&rx), Event::Failed(_)), "not a program");
        std::fs::create_dir_all(dir.join("garrysmod")).expect("garrysmod");
        tx.send(Cmd::Bridge { root: dir.clone(), install: true }).expect("send");
        assert_eq!(next(&rx), Event::Bridge(true));
        tx.send(Cmd::Place { root: dir.clone(), map: "gm_test".to_owned(), pos: [1.0, 2.0, 3.0], ang: [0.0; 3] })
            .expect("send");
        tx.send(Cmd::DumpProps { root: dir.clone() }).expect("send");
        assert_eq!(rx.recv_timeout(Duration::from_secs(20)).expect("answer"), Event::PropsTimeout);
        let cmd = std::fs::read_to_string(dir.join("garrysmod/data/hammerpro/cmd.json")).expect("command");
        assert!(cmd.contains("dump_props"), "{cmd}");
        tx.send(Cmd::Bridge { root: dir.clone(), install: false }).expect("send");
        assert_eq!(next(&rx), Event::Bridge(false));
        tx.send(Cmd::Watch(None)).expect("send");
        assert_eq!(next(&rx), Event::Bridge(false));
        assert_eq!(next(&rx), Event::State(GameState::Unknown));
        drop(tx);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn executables_come_from_the_install_or_the_settings() {
        let dir = scratch("exe");
        assert_eq!(game_exe(&dir, None), None);
        std::fs::write(dir.join("hl2.exe"), b"").expect("write");
        assert_eq!(game_exe(&dir, None), Some(dir.join("hl2.exe")));
        std::fs::write(dir.join("gmod.exe"), b"").expect("write");
        std::fs::create_dir_all(dir.join("bin/win64")).expect("bin");
        std::fs::write(dir.join("bin/win64/gmod.exe"), b"").expect("write");
        assert_eq!(game_exe(&dir, None), Some(dir.join("bin").join("win64").join("gmod.exe")));
        let chosen = dir.join("custom.exe");
        assert_eq!(game_exe(&dir, Some(&chosen)), Some(dir.join("bin").join("win64").join("gmod.exe")), "missing");
        std::fs::write(&chosen, b"").expect("write");
        assert_eq!(game_exe(&dir, Some(&chosen)), Some(chosen.clone()));
        assert_eq!(
            game_exe(&dir, Some(Path::new("custom.exe"))),
            Some(dir.join("bin").join("win64").join("gmod.exe")),
            "relative paths are ignored"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
