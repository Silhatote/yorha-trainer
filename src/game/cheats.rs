//! Installing and removing each cheat in the running game.
//!
//! Every cheat keeps the original bytes it overwrote and the memory it allocated, so it can be
//! removed cleanly (and is, when it's turned off or the trainer closes).

use std::time::Duration;

use super::asm::{jmp_patch, Asm, Target};
use super::process::Process;
use super::sigs::Sigs;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CheatId {
    AutoCollect,
    Range,
    Respawn,
    ItemCap,
}

impl CheatId {
    pub const ALL: [CheatId; 4] = [
        CheatId::AutoCollect,
        CheatId::Range,
        CheatId::Respawn,
        CheatId::ItemCap,
    ];
}

struct Patch {
    addr: u64,
    original: Vec<u8>,
}

/// A cheat that is in the game right now.
pub struct Installed {
    patches: Vec<Patch>,
    allocs: Vec<u64>,
    /// Addresses the trainer keeps writing to while the cheat is on.
    pub vars: Vars,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vars {
    /// Auto-collect range (float).
    pub range: Option<u64>,
    /// Item stack limit (u32).
    pub item_cap: Option<u64>,
    /// Respawn shared block (see `respawn::Block`).
    pub respawn_block: Option<u64>,
    /// Respawn log (128 records of 0x100 bytes).
    pub respawn_log: Option<u64>,
}

impl Installed {
    fn new() -> Self {
        Self {
            patches: Vec::new(),
            allocs: Vec::new(),
            vars: Vars::default(),
        }
    }

    /// Overwrites code, remembering what was there.
    fn patch(&mut self, p: &Process, addr: u64, bytes: &[u8]) -> Result<(), String> {
        let original = p
            .read_vec(addr, bytes.len())
            .ok_or("could not read the game's code")?;
        if !p.write(addr, bytes) {
            return Err("could not write to the game's code".into());
        }
        self.patches.push(Patch { addr, original });
        Ok(())
    }

    /// Puts every original byte back, then frees the allocations once no thread can still be
    /// running inside them.
    pub fn remove(self, p: &Process) {
        for patch in self.patches.iter().rev() {
            p.write(patch.addr, &patch.original);
        }
        if !self.allocs.is_empty() {
            std::thread::sleep(Duration::from_millis(150));
            for a in &self.allocs {
                p.free(*a);
            }
        }
    }

    /// Undoes a half-installed cheat after an error.
    fn fail(self, p: &Process, e: impl Into<String>) -> Result<Installed, String> {
        self.remove(p);
        Err(e.into())
    }
}

pub fn install(id: CheatId, p: &Process, s: &Sigs, settings: &Values) -> Result<Installed, String> {
    match id {
        CheatId::AutoCollect => auto_collect(p, s),
        CheatId::Range => range(p, s, settings.range),
        CheatId::Respawn => respawn(p, s),
        CheatId::ItemCap => item_cap(p, s, settings.item_cap),
    }
}

/// The live values the cheats use.
#[derive(Clone, Copy, Debug)]
pub struct Values {
    pub range: f32,
    pub item_cap: u32,
}

fn missing() -> String {
    "this game version is not supported (signature not found)".into()
}

/// Skips the "chip level == -1" branch after getChipLevel(player, Auto-Collect Items) at both
/// call sites in the dropped-item update.
fn auto_collect(p: &Process, s: &Sigs) -> Result<Installed, String> {
    let (Some(a), Some(b)) = (s.auto_collect_a, s.auto_collect_b) else {
        return Err(missing());
    };
    let mut c = Installed::new();
    for site in [a, b] {
        if let Err(e) = c.patch(p, site + 0x10, &[0x90, 0x90]) {
            return c.fail(p, e);
        }
    }
    Ok(c)
}

/// Points `addss xmm6,[4.0]` (the fixed part of the reach) at our own float, and skips the
/// gate that ignores items whose model was unloaded for being far away.
fn range(p: &Process, s: &Sigs, value: f32) -> Result<Installed, String> {
    let (Some(code), Some(gate)) = (s.range_code, s.model_gate) else {
        return Err(missing());
    };
    let site = code + 0x33;
    let mut c = Installed::new();
    let Some(var) = p.alloc_near(site, 0x1000) else {
        return Err("could not allocate memory in the game".into());
    };
    c.allocs.push(var);
    p.write_f32(var, value);
    c.vars.range = Some(var);

    let mut a = Asm::new(site);
    a.bytes(&[0xF3, 0x0F, 0x58, 0x35])
        .rel32(Target::Abs(var), 0); // addss xmm6,[var]
    let code = match a.finish() {
        Ok(code) => code,
        Err(e) => return c.fail(p, e),
    };
    if let Err(e) = c.patch(p, site, &code) {
        return c.fail(p, e);
    }
    if let Err(e) = c.patch(p, gate + 0x0E, &[0x90; 6]) {
        return c.fail(p, e);
    }
    Ok(c)
}

/// Wraps getItemMaxStack: when it returns 99 (the normal stack limit) it returns our value.
fn item_cap(p: &Process, s: &Sigs, cap: u32) -> Result<Installed, String> {
    let Some(func) = s.item_max_stack else {
        return Err(missing());
    };
    let mut c = Installed::new();
    let Some(mem) = p.alloc_near(func, 0x1000) else {
        return Err("could not allocate memory in the game".into());
    };
    c.allocs.push(mem);
    p.write_u32(mem, cap);
    c.vars.item_cap = Some(mem);

    let hook = mem + 0x10;
    let mut a = Asm::new(hook);
    a.bytes(&[0x48, 0x83, 0xEC, 0x28]); //           sub rsp,28
    a.bytes(&[0xE8]).rel32(Target::Label("tramp"), 0); // call tramp
    a.bytes(&[0x48, 0x83, 0xC4, 0x28]); //           add rsp,28
    a.bytes(&[0x83, 0xF8, 0x63]); //                 cmp eax,99
    a.bytes(&[0x75]).rel8("done"); //                jne done
    a.bytes(&[0x8B, 0x05]).rel32(Target::Abs(mem), 0); // mov eax,[cap]
    a.label("done").bytes(&[0xC3]); //               ret
                                    // The original prologue, then back into the real function.
    a.label("tramp");
    a.bytes(&[0x48, 0x89, 0x5C, 0x24, 0x08]); //     mov [rsp+08],rbx
    a.bytes(&[0x57]); //                             push rdi
    a.bytes(&[0xE9]).rel32(Target::Abs(func + 6), 0); // jmp func+6
    let code = match a.finish() {
        Ok(code) => code,
        Err(e) => return c.fail(p, e),
    };
    if !p.write(hook, &code) {
        return c.fail(p, "could not write the hook");
    }
    let jump = match jmp_patch(func, hook, 6) {
        Ok(j) => j,
        Err(e) => return c.fail(p, e),
    };
    if let Err(e) = c.patch(p, func, &jump) {
        return c.fail(p, e);
    }
    Ok(c)
}

/// The respawn hooks (the logic that decides what to spawn runs in `respawn.rs`):
/// - after CreateEntity returns, enemies (objId 0x2xxxx) are logged with their setInfo;
/// - at the entry of the layout slot update (a game thread), one pending spawn is performed
///   by calling CreateEntity(entityManager, "", objId, spawnBuf).
fn respawn(p: &Process, s: &Sigs) -> Result<Installed, String> {
    use super::respawn::block;
    let (Some(ret), Some(slot), Some(name), Some(mgr)) = (
        s.create_entity_ret,
        s.layout_slot_update,
        s.spawn_name,
        s.entity_manager,
    ) else {
        return Err(missing());
    };
    let mut c = Installed::new();
    let Some(mem) = p.alloc_near(ret, 0x1000) else {
        return Err("could not allocate memory in the game".into());
    };
    c.allocs.push(mem);
    let Some(log) = p.alloc(super::respawn::LOG_BYTES) else {
        return c.fail(p, "could not allocate memory in the game");
    };
    c.allocs.push(log);
    c.vars.respawn_block = Some(mem);
    c.vars.respawn_log = Some(log);

    let create = ret - 0x47;
    p.write_u64(mem + block::MGR, mgr);
    p.write_u64(mem + block::NAME, name);
    p.write_u64(mem + block::FUNC, create);

    // Log hook: runs right after the wrapper's call (rax = entity info,
    // [rsp+28] = objId, [rsp+30] = setInfo).
    let log_hook = mem + block::LOG_HOOK;
    let mut a = Asm::new(log_hook);
    a.bytes(&[0x48, 0x85, 0xC0]); //                         test rax,rax
    a.bytes(&[0x0F, 0x84]).rel32(Target::Label("skip"), 0); // je skip
    a.bytes(&[0x44, 0x8B, 0x54, 0x24, 0x28]); //             mov r10d,[rsp+28]
    a.bytes(&[0x41, 0xC1, 0xEA, 0x10]); //                   shr r10d,10
    a.bytes(&[0x41, 0x83, 0xFA, 0x02]); //                   cmp r10d,2
    a.bytes(&[0x0F, 0x85]).rel32(Target::Label("skip"), 0); // jne skip
    a.bytes(&[0x4C, 0x8B, 0x4C, 0x24, 0x30]); //             mov r9,[rsp+30]
    a.bytes(&[0x4D, 0x85, 0xC9]); //                         test r9,r9
    a.bytes(&[0x0F, 0x84]).rel32(Target::Label("skip"), 0); // je skip
    a.bytes(&[0x41, 0xBA, 0x01, 0x00, 0x00, 0x00]); //       mov r10d,1
    a.bytes(&[0xF0, 0x44, 0x0F, 0xC1, 0x15]) //              lock xadd [logIndex],r10d
        .rel32(Target::Abs(mem + block::LOG_INDEX), 0);
    a.bytes(&[0x41, 0x83, 0xE2, 0x7F]); //                   and r10d,7F
    a.bytes(&[0x49, 0xC1, 0xE2, 0x08]); //                   shl r10,8
    a.bytes(&[0x49, 0xBB]).u64(log); //                      mov r11,log
    a.bytes(&[0x4D, 0x01, 0xDA]); //                         add r10,r11
    a.bytes(&[0x45, 0x31, 0xDB]); //                         xor r11d,r11d
    a.label("copy");
    a.bytes(&[0x43, 0x0F, 0x10, 0x04, 0x19]); //             movups xmm0,[r9+r11]
    a.bytes(&[0x43, 0x0F, 0x11, 0x44, 0x1A, 0x10]); //       movups [r10+r11+10],xmm0
    a.bytes(&[0x49, 0x83, 0xC3, 0x10]); //                   add r11,10
    a.bytes(&[0x49, 0x81, 0xFB, 0xC0, 0x00, 0x00, 0x00]); // cmp r11,C0
    a.bytes(&[0x72]).rel8("copy"); //                        jb copy
    a.bytes(&[0x44, 0x8B, 0x5C, 0x24, 0x28]); //             mov r11d,[rsp+28]
    a.bytes(&[0x45, 0x89, 0x5A, 0x08]); //                   mov [r10+08],r11d
    a.bytes(&[0x44, 0x8B, 0x58, 0x30]); //                   mov r11d,[rax+30]
    a.bytes(&[0x45, 0x89, 0x5A, 0x0C]); //                   mov [r10+0C],r11d
    a.bytes(&[0x49, 0x89, 0x02]); //                         mov [r10],rax
    a.label("skip");
    a.bytes(&[0x48, 0x81, 0xC4, 0x88, 0x00, 0x00, 0x00]); // add rsp,88
    a.bytes(&[0xC3]); //                                     ret
    let log_code = match a.finish() {
        Ok(code) => code,
        Err(e) => return c.fail(p, e),
    };

    // Exec hook: at the entry of the layout slot update.
    let exec_hook = mem + block::EXEC_HOOK;
    let pending = mem + block::PENDING;
    let mut a = Asm::new(exec_hook);
    a.bytes(&[0x83, 0x3D])
        .rel32(Target::Abs(pending), 1)
        .bytes(&[0x01]); // cmp [pending],1
    a.bytes(&[0x0F, 0x84]).rel32(Target::Label("do"), 0); //  je do
    a.label("orig");
    a.bytes(&[0x48, 0x89, 0x5C, 0x24, 0x18]); //             mov [rsp+18],rbx
    a.bytes(&[0x55]); //                                     push rbp
    a.bytes(&[0xE9]).rel32(Target::Abs(slot + 6), 0); //      jmp slot+6
    a.label("do");
    a.bytes(&[0x50, 0x51]); //                               push rax; push rcx
    a.bytes(&[0xB8, 0x01, 0x00, 0x00, 0x00]); //             mov eax,1
    a.bytes(&[0x31, 0xC9]); //                               xor ecx,ecx
    a.bytes(&[0xF0, 0x0F, 0xB1, 0x0D])
        .rel32(Target::Abs(pending), 0); // lock cmpxchg [pending],ecx
    a.bytes(&[0x59, 0x58]); //                               pop rcx; pop rax
    a.bytes(&[0x75]).rel8("orig"); //                        jne orig (someone else took it)
    a.bytes(&[0x51, 0x52, 0x41, 0x50, 0x41, 0x51]); //       push rcx, rdx, r8, r9
    a.bytes(&[0x48, 0x83, 0xEC, 0x28]); //                   sub rsp,28
    a.bytes(&[0x48, 0x8B, 0x0D])
        .rel32(Target::Abs(mem + block::MGR), 0); // mov rcx,[mgr]
    a.bytes(&[0x48, 0x8B, 0x15])
        .rel32(Target::Abs(mem + block::NAME), 0); // mov rdx,[name]
    a.bytes(&[0x44, 0x8B, 0x05])
        .rel32(Target::Abs(mem + block::OBJ_ID), 0); // mov r8d,[objId]
    a.bytes(&[0x4C, 0x8D, 0x0D])
        .rel32(Target::Abs(mem + block::SPAWN_BUF), 0); // lea r9,[buf]
    a.bytes(&[0xFF, 0x15])
        .rel32(Target::Abs(mem + block::FUNC), 0); // call [CreateEntity]
    a.bytes(&[0x48, 0x89, 0x05])
        .rel32(Target::Abs(mem + block::RESULT), 0); // mov [result],rax
    a.bytes(&[0x48, 0x83, 0xC4, 0x28]); //                   add rsp,28
    a.bytes(&[0x41, 0x59, 0x41, 0x58, 0x5A, 0x59]); //       pop r9, r8, rdx, rcx
    a.bytes(&[0xE9]).rel32(Target::Label("orig"), 0); //      jmp orig
    let exec_code = match a.finish() {
        Ok(code) => code,
        Err(e) => return c.fail(p, e),
    };

    if !p.write(log_hook, &log_code) || !p.write(exec_hook, &exec_code) {
        return c.fail(p, "could not write the hooks");
    }
    let ret_site = ret + 0x15;
    let jumps =
        jmp_patch(ret_site, log_hook, 8).and_then(|j1| Ok((j1, jmp_patch(slot, exec_hook, 6)?)));
    let (j1, j2) = match jumps {
        Ok(j) => j,
        Err(e) => return c.fail(p, e),
    };
    if let Err(e) = c.patch(p, ret_site, &j1) {
        return c.fail(p, e);
    }
    if let Err(e) = c.patch(p, slot, &j2) {
        return c.fail(p, e);
    }
    Ok(c)
}
