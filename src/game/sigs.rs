//! Every address the trainer needs, found by signature in the game's module (so ASLR and the
//! load address don't matter). See `docs/RESEARCH.md` for what each place does.

use super::process::Process;
use super::scan::{rip_target, Pattern};

/// Dropped items: `mov edx,2F (Auto-Collect Items); mov rcx,rax; call getChipLevel;
/// cmp eax,-1; jle ...` — two call sites, told apart by the jump distance.
const AUTO_COLLECT_A: &str = "BA 2F 00 00 00 48 8B C8 E8 ?? ?? ?? ?? 83 F8 FF 7E 48";
const AUTO_COLLECT_B: &str = "BA 2F 00 00 00 48 8B C8 E8 ?? ?? ?? ?? 83 F8 FF 7E 33";
/// Auto-collect distance check; `addss xmm6,[4.0]` sits at +0x33.
const RANGE_CODE: &str = "F3 0F 58 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 4C 8D 43 10 48 8D 54 24 30 48 8D 4C 24 30 E8 ?? ?? ?? ?? 4C 8D 47 50 48 8D 54 24 30 48 8D 4C 24 40 E8 ?? ?? ?? ?? F3 0F 58 35";
/// "Item model streamed in" gate in the item update; the `je` sits at +0x0E (6 bytes).
const MODEL_GATE: &str =
    "48 8D 8B 18 0D 00 00 E8 ?? ?? ?? ?? 85 C0 0F 84 ?? ?? ?? ?? 83 BB 30 0D 00 00 00";
/// getItemMaxStack(itemTable, itemId).
const ITEM_MAX_STACK: &str = "48 89 5C 24 08 57 48 83 EC 20 8B FA 48 8B D9 E8 ?? ?? ?? ?? 83 F8 07 0F 87 ?? ?? ?? ?? 48 8D 0D ?? ?? ?? ?? 48 98 44 8B 84 81 ?? ?? ?? ?? 4C 03 C1 41 FF E0 48 8D 93 B0 01 00 00 44 8B C7 48 8B CB E8 ?? ?? ?? ?? 8B 40 08";
/// The tail of CreateEntity's wrapper: `call create; add rsp,88; ret` (the hook goes on the
/// `add` at +0x15; the wrapper itself starts 0x47 bytes before the match).
const CREATE_ENTITY_RET: &str =
    "41 8B 81 88 00 00 00 89 44 24 40 48 8D 54 24 20 E8 ?? ?? ?? ?? 48 81 C4 88 00 00 00 C3";
/// The layout system's per-slot update: runs many times per frame on a game thread.
const LAYOUT_SLOT_UPDATE: &str = "48 89 5C 24 18 55 56 57 41 56 41 57 48 8D 6C 24 C9 48 81 EC F0 00 00 00 48 8B 05 ?? ?? ?? ?? 48 33 C4 48 89 45 17 8B 41 04";
/// getChipLevel(entity, type): called many times per frame by the player's update, in every
/// area, so respawns also go out where the layout system is idle.
const CHIP_LEVEL: &str = "44 8B C2 4C 8B C9 83 FA 1D 74 05 83 FA 24 75 0F F6 05 ?? ?? ?? ?? 20 74 06 B8 08 00 00 00 C3 48 8B 89 10 66 01 00";
/// The layout spawner: `lea rdx,[name]; ...; lea rcx,[entityManager]; call CreateEntity`.
const LAYOUT_SPAWNER: &str = "4C 8D 4D 87 48 8D 15 ?? ?? ?? ?? 89 45 FB 48 8D 0D ?? ?? ?? ?? E8";
/// Entity handle lookup: the table's count and pointer globals.
const HANDLE_TABLE: &str =
    "8B 11 85 D2 74 ?? 8B C2 C1 F8 08 0F B7 C8 3B 0D ?? ?? ?? ?? 73 ?? 48 C1 E1 04 48 03 0D";

#[derive(Clone, Copy, Debug, Default)]
pub struct Sigs {
    pub auto_collect_a: Option<u64>,
    pub auto_collect_b: Option<u64>,
    pub range_code: Option<u64>,
    pub model_gate: Option<u64>,
    pub item_max_stack: Option<u64>,
    pub create_entity_ret: Option<u64>,
    pub layout_slot_update: Option<u64>,
    pub chip_level: Option<u64>,
    /// Empty name string the layout passes to CreateEntity.
    pub spawn_name: Option<u64>,
    pub entity_manager: Option<u64>,
    pub handle_count: Option<u64>,
    pub handle_table: Option<u64>,
}

impl Sigs {
    /// Scans a copy of the module. Returns `None` if it couldn't be read.
    pub fn resolve(p: &Process) -> Option<Self> {
        let image = read_image(p)?;
        let base = p.base;
        let at = |pat: &str| Pattern::parse(pat).find_unique(&image);
        let abs = |off: Option<usize>| off.map(|o| base + o as u64);

        let spawner = at(LAYOUT_SPAWNER);
        let handles = at(HANDLE_TABLE).map(|o| o + 6);
        Some(Self {
            auto_collect_a: abs(at(AUTO_COLLECT_A)),
            auto_collect_b: abs(at(AUTO_COLLECT_B)),
            range_code: abs(at(RANGE_CODE)),
            model_gate: abs(at(MODEL_GATE)),
            item_max_stack: abs(at(ITEM_MAX_STACK)),
            create_entity_ret: abs(at(CREATE_ENTITY_RET)),
            layout_slot_update: abs(at(LAYOUT_SLOT_UPDATE)),
            chip_level: abs(at(CHIP_LEVEL)),
            spawn_name: spawner.map(|s| rip_target(&image, base, s + 4, 3, 7)),
            entity_manager: spawner.map(|s| rip_target(&image, base, s + 14, 3, 7)),
            handle_count: handles.map(|h| rip_target(&image, base, h + 8, 2, 6)),
            handle_table: handles.map(|h| rip_target(&image, base, h + 20, 3, 7)),
        })
    }

    pub fn auto_collect_ok(&self) -> bool {
        self.auto_collect_a.is_some() && self.auto_collect_b.is_some()
    }
    pub fn range_ok(&self) -> bool {
        self.range_code.is_some() && self.model_gate.is_some()
    }
    pub fn item_cap_ok(&self) -> bool {
        self.item_max_stack.is_some()
    }
    pub fn respawn_ok(&self) -> bool {
        self.create_entity_ret.is_some()
            && self.layout_slot_update.is_some()
            && self.chip_level.is_some()
            && self.spawn_name.is_some()
            && self.entity_manager.is_some()
            && self.handle_count.is_some()
            && self.handle_table.is_some()
    }
}

/// The module image, read in chunks (unreadable pages stay zero).
fn read_image(p: &Process) -> Option<Vec<u8>> {
    const CHUNK: usize = 0x1_0000;
    let mut image = vec![0u8; p.size];
    let mut any = false;
    for (i, chunk) in image.chunks_mut(CHUNK).enumerate() {
        any |= p.read_into(p.base + (i * CHUNK) as u64, chunk);
    }
    any.then_some(image)
}
