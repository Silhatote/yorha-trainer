//! The worker thread: finds the game, keeps the cheats in the state the UI asks for, runs the
//! respawn monitor and reads the hotkeys. The UI only talks to it through `Shared`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::game::cheats::{self, CheatId, Installed, Values};
use crate::game::process::{self, Process};
use crate::game::respawn::{self, Monitor};
use crate::game::sigs::Sigs;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// The game isn't running.
    Waiting,
    /// Connected to the game.
    Attached { pid: u32 },
    /// Couldn't open the process (another user, or protected).
    NoAccess { pid: u32 },
}

#[derive(Clone, Debug)]
pub enum LogKind {
    Attached(u32),
    Detached,
    On(CheatId),
    Off(CheatId),
    Error(CheatId, String),
    Killed(u32),
    Spawned(u32),
    SpawnFailed(u32),
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub at: Instant,
    pub kind: LogKind,
}

/// What the UI wants.
#[derive(Clone, Copy, Debug)]
pub struct Wanted {
    pub enabled: [bool; 4],
    pub values: Values,
    pub respawn: respawn::Options,
    pub hotkeys: bool,
}

/// Shared between the UI and the worker.
pub struct Shared {
    pub status: Status,
    pub wanted: Wanted,
    /// Which cheats are in the game right now.
    pub active: [bool; 4],
    /// Which cheats this game version supports (all true until attached).
    pub supported: [bool; 4],
    pub log: VecDeque<LogEntry>,
    pub kills: u32,
    pub respawns: u32,
    /// Hotkey toggles the UI must save (the worker flips `wanted.enabled` itself).
    pub toggled_by_hotkey: bool,
    respawn_last: bool,
    quit: bool,
}

pub fn index(id: CheatId) -> usize {
    CheatId::ALL.iter().position(|&c| c == id).unwrap()
}

impl Shared {
    fn push(&mut self, kind: LogKind) {
        self.log.push_front(LogEntry {
            at: Instant::now(),
            kind,
        });
        self.log.truncate(80);
    }
}

pub struct Trainer {
    pub shared: Arc<Mutex<Shared>>,
    worker: Option<JoinHandle<()>>,
}

impl Trainer {
    pub fn start(wanted: Wanted) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            status: Status::Waiting,
            wanted,
            active: [false; 4],
            supported: [true; 4],
            log: VecDeque::new(),
            kills: 0,
            respawns: 0,
            toggled_by_hotkey: false,
            respawn_last: false,
            quit: false,
        }));
        let s = shared.clone();
        let worker = std::thread::Builder::new()
            .name("trainer".into())
            .spawn(move || run(s))
            .ok();
        Self { shared, worker }
    }

    pub fn respawn_last(&self) {
        if let Ok(mut s) = self.shared.lock() {
            s.respawn_last = true;
        }
    }

    /// Takes every cheat out of the game and stops the worker.
    pub fn shutdown(&mut self) {
        if let Ok(mut s) = self.shared.lock() {
            s.quit = true;
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for Trainer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Game {
    p: Process,
    sigs: Sigs,
    installed: HashMap<CheatId, Installed>,
    monitor: Option<Monitor>,
    values: Option<Values>,
}

impl Game {
    fn remove(&mut self, id: CheatId) {
        if id == CheatId::Respawn {
            if let Some(m) = self.monitor.as_mut() {
                m.stop(&self.p);
            }
            self.monitor = None;
        }
        if let Some(c) = self.installed.remove(&id) {
            c.remove(&self.p);
        }
    }

    fn remove_all(&mut self) {
        for id in CheatId::ALL {
            self.remove(id);
        }
    }

    fn supported(&self) -> [bool; 4] {
        [
            self.sigs.auto_collect_ok(),
            self.sigs.range_ok(),
            self.sigs.respawn_ok(),
            self.sigs.item_cap_ok(),
        ]
    }
}

const HOTKEYS: [(i32, Option<CheatId>); 5] = [
    (0x70, Some(CheatId::AutoCollect)), // F1
    (0x71, Some(CheatId::Range)),       // F2
    (0x72, Some(CheatId::Respawn)),     // F3
    (0x73, Some(CheatId::ItemCap)),     // F4
    (0x75, None),                       // F6: respawn the last enemy killed
];

fn key_down(vk: i32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // SAFETY: reads the key state; no pointers involved.
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

fn run(shared: Arc<Mutex<Shared>>) {
    let mut game: Option<Game> = None;
    let mut next_look = Instant::now();
    let mut keys_before = [false; HOTKEYS.len()];

    loop {
        std::thread::sleep(Duration::from_millis(100));

        // Hotkeys (edge-triggered).
        let mut hotkey_toggles = Vec::new();
        let mut hotkey_respawn = false;
        for (k, (vk, action)) in HOTKEYS.iter().enumerate() {
            let down = key_down(*vk);
            if down && !keys_before[k] {
                match action {
                    Some(id) => hotkey_toggles.push(*id),
                    None => hotkey_respawn = true,
                }
            }
            keys_before[k] = down;
        }

        let (wanted, quit, respawn_last) = {
            let Ok(mut s) = shared.lock() else { return };
            if s.wanted.hotkeys && game.is_some() {
                for id in &hotkey_toggles {
                    let i = index(*id);
                    s.wanted.enabled[i] = !s.wanted.enabled[i];
                    s.toggled_by_hotkey = true;
                }
            }
            let respawn_last = std::mem::take(&mut s.respawn_last)
                || (hotkey_respawn && s.wanted.hotkeys && game.is_some());
            (s.wanted, s.quit, respawn_last)
        };

        if quit {
            if let Some(g) = game.as_mut() {
                if g.p.is_alive() {
                    g.remove_all();
                }
            }
            return;
        }

        // Connect.
        if game.is_none() {
            if Instant::now() < next_look {
                continue;
            }
            next_look = Instant::now() + Duration::from_secs(1);
            let Some(pid) = process::find_pid(process::GAME_EXE) else {
                set_status(&shared, Status::Waiting);
                continue;
            };
            let Some(p) = Process::open(pid, process::GAME_EXE) else {
                set_status(&shared, Status::NoAccess { pid });
                continue;
            };
            let Some(sigs) = Sigs::resolve(&p) else {
                continue;
            };
            let g = Game {
                p,
                sigs,
                installed: HashMap::new(),
                monitor: None,
                values: None,
            };
            if let Ok(mut s) = shared.lock() {
                s.status = Status::Attached { pid };
                s.supported = g.supported();
                s.active = [false; 4];
                s.push(LogKind::Attached(pid));
            }
            game = Some(g);
        }
        let g = game.as_mut().unwrap();

        // The game closed: nothing to restore.
        if !g.p.is_alive() {
            game = None;
            if let Ok(mut s) = shared.lock() {
                s.status = Status::Waiting;
                s.active = [false; 4];
                s.supported = [true; 4];
                s.push(LogKind::Detached);
            }
            continue;
        }

        // Bring the cheats to the wanted state.
        let supported = g.supported();
        let mut changes = Vec::new();
        for id in CheatId::ALL {
            let i = index(id);
            let want = wanted.enabled[i] && supported[i];
            let have = g.installed.contains_key(&id);
            if want && !have {
                match cheats::install(id, &g.p, &g.sigs, &wanted.values) {
                    Ok(c) => {
                        if id == CheatId::Respawn {
                            g.monitor = c
                                .vars
                                .respawn_block
                                .zip(c.vars.respawn_log)
                                .and_then(|(b, l)| Monitor::new(b, l, &g.sigs));
                        }
                        g.installed.insert(id, c);
                        changes.push(LogKind::On(id));
                    }
                    Err(e) => changes.push(LogKind::Error(id, e)),
                }
            } else if !want && have {
                g.remove(id);
                changes.push(LogKind::Off(id));
            }
        }

        // Live values.
        if g.values
            .is_none_or(|v| v.range != wanted.values.range || v.item_cap != wanted.values.item_cap)
        {
            for c in g.installed.values() {
                if let Some(a) = c.vars.range {
                    g.p.write_f32(a, wanted.values.range);
                }
                if let Some(a) = c.vars.item_cap {
                    g.p.write_u32(a, wanted.values.item_cap);
                }
            }
            g.values = Some(wanted.values);
        }

        // Respawn.
        let mut events = Vec::new();
        if let Some(m) = g.monitor.as_mut() {
            if respawn_last {
                m.respawn_last();
            }
            events = m.tick(&g.p, wanted.respawn);
        }

        if let Ok(mut s) = shared.lock() {
            for id in CheatId::ALL {
                s.active[index(id)] = g.installed.contains_key(&id);
            }
            s.supported = supported;
            for c in changes {
                if let LogKind::Error(id, _) = &c {
                    // Don't retry a cheat that failed every 100 ms.
                    s.wanted.enabled[index(*id)] = false;
                    s.toggled_by_hotkey = true;
                }
                s.push(c);
            }
            for e in events {
                match e {
                    respawn::Event::Killed(o) => {
                        s.kills += 1;
                        s.push(LogKind::Killed(o));
                    }
                    respawn::Event::Spawned(o) => {
                        s.respawns += 1;
                        s.push(LogKind::Spawned(o));
                    }
                    respawn::Event::Failed(o) => s.push(LogKind::SpawnFailed(o)),
                }
            }
        }
    }
}

fn set_status(shared: &Arc<Mutex<Shared>>, status: Status) {
    if let Ok(mut s) = shared.lock() {
        s.status = status;
    }
}
