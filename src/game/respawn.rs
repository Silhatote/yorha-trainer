//! Enemy respawn: watches the enemies' HP and, when one dies, asks the game hook to create it
//! again (with the setInfo it was originally spawned with, or where it died).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use super::process::Process;
use super::sigs::Sigs;

/// Layout of the shared block the hooks use (allocated next to the game's code).
pub mod block {
    pub const LOG_INDEX: u64 = 0x00;
    pub const PENDING: u64 = 0x08;
    pub const OBJ_ID: u64 = 0x0C;
    pub const MGR: u64 = 0x10;
    pub const NAME: u64 = 0x18;
    pub const FUNC: u64 = 0x20;
    pub const RESULT: u64 = 0x28;
    pub const LOG_HOOK: u64 = 0x100;
    pub const EXEC_HOOK: u64 = 0x200;
    pub const SPAWN_BUF: u64 = 0x300;
}

const LOG_SLOTS: usize = 128;
const LOG_STRIDE: usize = 0x100;
pub const LOG_BYTES: usize = LOG_SLOTS * LOG_STRIDE;
const SI_SIZE: usize = 0xC0;
type SetInfo = [u8; SI_SIZE];

/// Entity info fields.
const INFO_OBJ_ID: u64 = 0x28;
const INFO_HANDLE: u64 = 0x30;
const INFO_ENTITY: u64 = 0x60;
/// Entity fields.
const ENT_POS: u64 = 0x50;
const ENT_HP: u64 = 0x858;
const ENT_MAX_HP: u64 = 0x85C;

const MAX_TRIES: u32 = 5;

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// How far around a death the population of that enemy kind is counted.
const AREA_RADIUS: f32 = 40.0;

#[derive(Clone, Debug)]
pub enum Event {
    Killed(u32),
    Spawned(u32),
    Failed(u32),
    /// Not respawned: the game had already brought the area back to its population.
    Skipped(u32),
}

#[derive(Clone)]
struct Rec {
    obj_id: u32,
    si: SetInfo,
    /// Spawn here instead of the setInfo's position.
    pos: Option<[f32; 3]>,
    /// Where it died, and how many of its kind were alive around there (itself included).
    /// The respawn is skipped if the area is already back to that many — some areas respawn
    /// their enemies on their own, and respawning on top of that multiplies them.
    area: [f32; 3],
    crowd: u32,
}

struct Tracked {
    obj_id: u32,
    si: Option<SetInfo>,
    alive: bool,
    dead: bool,
    pos: [f32; 3],
    /// Set when this entity is one of our respawns (to retry if the game discards it).
    ours: Option<(Rec, u32)>,
}

struct Queued {
    at: Instant,
    rec: Rec,
    tries: u32,
}

/// The respawn settings the UI controls.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub auto: bool,
    pub delay: f32,
    pub at_death: bool,
}

pub struct Monitor {
    block: u64,
    log: u64,
    handle_count: u64,
    handle_table: u64,
    tracked: HashMap<(u64, u32), Tracked>,
    /// Handle-table entries already known not to be enemies.
    ignored: HashSet<(u64, u32)>,
    templates: HashMap<u32, SetInfo>,
    queue: VecDeque<Queued>,
    last: Option<Rec>,
    awaiting: Option<(Rec, u32)>,
    next_scan: Instant,
}

impl Monitor {
    pub fn new(block: u64, log: u64, sigs: &Sigs) -> Option<Self> {
        Some(Self {
            block,
            log,
            handle_count: sigs.handle_count?,
            handle_table: sigs.handle_table?,
            tracked: HashMap::new(),
            ignored: HashSet::new(),
            templates: HashMap::new(),
            queue: VecDeque::new(),
            last: None,
            awaiting: None,
            next_scan: Instant::now(),
        })
    }

    /// One step (call every ~100 ms). Returns what happened.
    pub fn tick(&mut self, p: &Process, opt: Options) -> Vec<Event> {
        let mut events = Vec::new();
        self.scan_log(p);
        if Instant::now() >= self.next_scan {
            self.scan_handle_table(p);
            self.next_scan = Instant::now() + Duration::from_secs(2);
        }
        self.check_deaths(p, opt, &mut events);
        self.pump(p, &mut events);
        events
    }

    /// Spawns another copy of the last enemy killed (on request, so no population limit).
    pub fn respawn_last(&mut self) {
        if let Some(mut rec) = self.last.clone() {
            rec.crowd = u32::MAX;
            self.queue.push_back(Queued {
                at: Instant::now(),
                rec,
                tries: 0,
            });
        }
    }

    /// Respawns of a kind already waiting for an area (queued or being created).
    fn waiting(&self, obj_id: u32, at: [f32; 3]) -> u32 {
        let near = |r: &Rec| r.obj_id == obj_id && distance(r.area, at) <= AREA_RADIUS;
        self.queue.iter().filter(|q| near(&q.rec)).count() as u32
            + self.awaiting.iter().filter(|(r, _)| near(r)).count() as u32
    }

    /// Living enemies of a kind within `AREA_RADIUS` of a point.
    fn population(&self, obj_id: u32, at: [f32; 3]) -> u32 {
        self.tracked
            .values()
            .filter(|t| {
                t.obj_id == obj_id && t.alive && !t.dead && distance(t.pos, at) <= AREA_RADIUS
            })
            .count() as u32
    }

    /// Cancels anything not yet handed to the game.
    pub fn stop(&mut self, p: &Process) {
        p.write_u32(self.block + block::PENDING, 0);
        self.queue.clear();
    }

    fn track(&mut self, info: u64, handle: u32, obj_id: u32, si: Option<SetInfo>) -> &mut Tracked {
        let t = self.tracked.entry((info, handle)).or_insert(Tracked {
            obj_id,
            si: None,
            alive: false,
            dead: false,
            pos: [0.0; 3],
            ours: None,
        });
        if let (Some(si), None) = (si, t.si) {
            t.si = Some(si);
            self.templates.insert(obj_id, si);
        }
        t
    }

    /// Enemies created since the cheat was turned on, with their spawn setInfo.
    fn scan_log(&mut self, p: &Process) {
        let Some(log) = p.read_vec(self.log, LOG_BYTES) else {
            return;
        };
        for rec in log.as_chunks::<LOG_STRIDE>().0 {
            let info = u64::from_le_bytes(rec[0..8].try_into().unwrap());
            if info == 0 {
                continue;
            }
            let obj_id = u32::from_le_bytes(rec[8..12].try_into().unwrap());
            let handle = u32::from_le_bytes(rec[12..16].try_into().unwrap());
            if self
                .tracked
                .get(&(info, handle))
                .is_some_and(|t| t.si.is_some())
            {
                continue;
            }
            let si: SetInfo = rec[0x10..0x10 + SI_SIZE].try_into().unwrap();
            self.track(info, handle, obj_id, Some(si));
        }
    }

    /// Enemies that already existed before the cheat was turned on.
    fn scan_handle_table(&mut self, p: &Process) {
        let (Some(count), Some(table)) = (p.u32(self.handle_count), p.u64(self.handle_table))
        else {
            return;
        };
        if count == 0 || count > 65536 {
            return;
        }
        let Some(raw) = p.read_vec(table, count as usize * 16) else {
            return;
        };
        if self.ignored.len() > 50_000 {
            self.ignored.clear();
        }
        for e in raw.as_chunks::<16>().0 {
            let handle = u32::from_le_bytes(e[0..4].try_into().unwrap());
            let info = u64::from_le_bytes(e[8..16].try_into().unwrap());
            let key = (info, handle);
            if handle == 0
                || info == 0
                || self.tracked.contains_key(&key)
                || self.ignored.contains(&key)
            {
                continue;
            }
            if let Some(obj_id) = p.u32(info + INFO_OBJ_ID) {
                if obj_id >> 16 == 2 {
                    self.track(info, handle, obj_id, None);
                } else {
                    self.ignored.insert(key);
                }
            }
        }
    }

    fn check_deaths(&mut self, p: &Process, opt: Options, events: &mut Vec<Event>) {
        let mut killed = Vec::new();
        let mut killed_ours = Vec::new();
        let mut retry = Vec::new();
        self.tracked.retain(|&(info, handle), t| {
            if p.u32(info + INFO_HANDLE) != Some(handle) {
                // Released: despawned, cleaned up after death, or discarded by the game.
                if !t.alive {
                    if let Some((rec, tries)) = t.ours.take() {
                        retry.push((rec, tries));
                    }
                }
                return false;
            }
            let Some(ent) = p.u64(info + INFO_ENTITY).filter(|&e| e != 0) else {
                return true;
            };
            let (Some(hp), Some(max)) = (p.i32(ent + ENT_HP), p.i32(ent + ENT_MAX_HP)) else {
                return true;
            };
            if max <= 0 || max >= 100_000_000 || hp > max {
                return true; // not something with HP (projectiles and the like)
            }
            if hp > 0 {
                t.alive = true;
                if let Some(raw) = p.read::<12>(ent + ENT_POS) {
                    t.pos = [
                        f32::from_le_bytes(raw[0..4].try_into().unwrap()),
                        f32::from_le_bytes(raw[4..8].try_into().unwrap()),
                        f32::from_le_bytes(raw[8..12].try_into().unwrap()),
                    ];
                }
            } else if t.alive && !t.dead {
                t.dead = true;
                killed.push((t.obj_id, t.si, t.pos));
            } else if !t.alive && !t.dead {
                // One of our respawns killed before it was ever seen alive (it came back
                // right into the player's attacks): that's a kill too.
                if let Some((rec, _)) = t.ours.take() {
                    t.dead = true;
                    killed_ours.push(rec);
                }
            }
            true
        });
        for (rec, tries) in retry {
            if tries < MAX_TRIES {
                self.queue.push_back(Queued {
                    at: Instant::now() + Duration::from_secs(1),
                    rec,
                    tries: tries + 1,
                });
            }
        }
        let delay = Duration::from_secs_f32(opt.delay.clamp(0.0, 600.0));
        for (obj_id, si, pos) in killed {
            events.push(Event::Killed(obj_id));
            let use_death_pos = si.is_none() || opt.at_death;
            let rec = Rec {
                obj_id,
                si: si.unwrap_or_else(|| self.template(obj_id)),
                pos: use_death_pos.then_some(pos),
                area: pos,
                // Those still alive, this one, and the ones already waiting to come back
                // (several kills in a row would otherwise each see a smaller crowd).
                crowd: self.population(obj_id, pos) + 1 + self.waiting(obj_id, pos),
            };
            self.last = Some(rec.clone());
            if opt.auto {
                self.queue.push_back(Queued {
                    at: Instant::now() + delay,
                    rec,
                    tries: 0,
                });
            }
        }
        for rec in killed_ours {
            events.push(Event::Killed(rec.obj_id));
            if opt.auto {
                self.queue.push_back(Queued {
                    at: Instant::now() + delay,
                    rec,
                    tries: 0,
                });
            }
        }
    }

    /// The setInfo of another enemy of the same kind, or a neutral one (+0x88 must be 1, or
    /// CreateEntity refuses it).
    fn template(&self, obj_id: u32) -> SetInfo {
        if let Some(si) = self.templates.get(&obj_id) {
            return *si;
        }
        let mut b = [0u8; SI_SIZE];
        let mut f32_at = |off: usize, v: f32| b[off..off + 4].copy_from_slice(&v.to_le_bytes());
        for off in [0x00, 0x14, 0x28, 0x3C, 0x4C, 0x5C, 0x60, 0x64, 0x68, 0x6C] {
            f32_at(off, 1.0); // identity matrix, w = 1, scale 1
        }
        b[0x80..0x84].copy_from_slice(&(-1i32).to_le_bytes());
        b[0x88..0x8C].copy_from_slice(&1i32.to_le_bytes());
        b[0x8C..0x90].copy_from_slice(&(-1i32).to_le_bytes());
        b
    }

    /// Hands the next queued spawn to the game hook (one at a time).
    fn pump(&mut self, p: &Process, events: &mut Vec<Event>) {
        if p.u32(self.block + block::PENDING) != Some(0) {
            return;
        }
        if let Some((rec, tries)) = self.awaiting.take() {
            let info = p.u64(self.block + block::RESULT).unwrap_or(0);
            if info == 0 {
                // CreateEntity refused (e.g. during a loading screen): retry a few times.
                if tries < MAX_TRIES {
                    self.queue.push_back(Queued {
                        at: Instant::now() + Duration::from_secs(3),
                        rec,
                        tries: tries + 1,
                    });
                } else {
                    events.push(Event::Failed(rec.obj_id));
                }
            } else {
                events.push(Event::Spawned(rec.obj_id));
                if let Some(handle) = p.u32(info + INFO_HANDLE) {
                    let obj_id = rec.obj_id;
                    self.track(info, handle, obj_id, None).ours = Some((rec, tries));
                }
            }
        }
        let Some(front) = self.queue.front() else {
            return;
        };
        if Instant::now() < front.at {
            return;
        }
        let q = self.queue.pop_front().unwrap();
        if self.population(q.rec.obj_id, q.rec.area) >= q.rec.crowd {
            events.push(Event::Skipped(q.rec.obj_id));
            return;
        }
        let buf = self.block + block::SPAWN_BUF;
        p.write(buf, &q.rec.si);
        if let Some([x, y, z]) = q.rec.pos {
            p.write_f32(buf + 0x40, x);
            p.write_f32(buf + 0x44, y);
            p.write_f32(buf + 0x48, z);
        }
        p.write_u64(self.block + block::RESULT, 0);
        p.write_u32(self.block + block::OBJ_ID, q.rec.obj_id);
        p.write_u32(self.block + block::PENDING, 1);
        self.awaiting = Some((q.rec, q.tries));
    }
}
