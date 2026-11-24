# How the cheats work

Notes from reverse engineering `NieRAutomata.exe` (Steam, the 2021 build, image size
`0x18BE000`). Offsets are RVAs from the module base; the trainer finds every place by signature
(`src/game/sigs.rs`), so they are only here for orientation.

## Plug-in chips

- The save block lives in the module's data (`0x148C830`). Chip records start at `+0x1F50`,
  300 records of `0x30` bytes: base code at `+0x1F54` (`-1` = empty), **type** at `+0x1F58`,
  level at `+0x1F5C`, weight at `+0x1F60`, and the slot in chip sets A/B/C at
  `+0x1F64/68/6C`.
- When the chip set changes, `0x814E60` walks the equipped chips of the current set and calls
  `0x4F0B60(player, slot, type, level)`, which fills a table of 100 `{type, level + 1}` pairs
  pointed to by `player + 0x16610` (count at `+0x16618`).
- Code that wants a chip effect either loops over that table inline (`cmp dword ptr [r9], TYPE`)
  or calls the helper `getChipLevel(entity, type)` at `0x4927B0`, which returns the level or
  `-1`.
- **Auto-Collect Items is type 47 (`0x2F`)**: it is the entry that disappeared from the table
  when the chip was unequipped.

## Auto-Collect without the chip

The dropped-item update (`0x5EB990` and a second item class at `0x5EC92F`) does:

```
mov edx, 2F          ; Auto-Collect Items
mov rcx, rax         ; player
call getChipLevel
cmp eax, -1
jle no_chip          ; <- patched to two NOPs
```

With the chip path taken, the item calls its virtual "in range?" check and then starts flying to
the player.

## Range

The "in range?" check (`0x590880`) builds the reach as
`player radius (player + 0x1062C, 0.5) + 4.0` (a constant in `.rdata`), squares it and compares
it with the squared distance. The trainer re-points that `addss xmm6, [4.0]` at its own float.

Far items also fail an earlier gate: if `item + 0x598` has bit 0 set, the update requires the
resource handle at `item + 0xD18` to be loaded, and the game unloads it for distant items. The
range cheat skips that `je` too.

## Item stack limit

The add-item code (`0x7CA0E0`) clamps the new quantity to `getItemMaxStack(itemTable, itemId)`
(`0x7E49D0`), which reads the max from the item's data entry (`+0x08`, 99 for normal items). The
trainer wraps that function: when it returns 99, it returns the chosen limit instead.

## Enemy respawn

- Entities live in a handle table (count at `0x1029850`, pointer at `0x1029860`, 16-byte
  entries `{handle, pad, info*}`). An entity info has the object id at `+0x28`, its handle at
  `+0x30` and the entity at `+0x60`. Enemies have object ids `0x2xxxx`; their HP is at
  `entity + 0x858` (max at `+0x85C`), position at `+0x50`.
- `CreateEntity(entityManager, name, objId, setInfo)` (wrapper at `0x745C80`) creates them. The
  layout spawner (`0x35A083`) passes an empty name and a `setInfo` of `0xC0` bytes: rotation
  matrix at `+0x00`, **position at `+0x40`**, rotation at `+0x50`, scale at `+0x60`, flags at
  `+0x70..+0x8C`.
- `CreateEntity` must run on a game thread: calling it from a thread created by an external
  tool crashes the game. The trainer hooks the layout system's per-slot update (`0x359DC0`,
  called a few hundred times per second) and performs one queued spawn from there.
- A second hook, right after `CreateEntity` returns, logs every enemy with its `setInfo`, so a
  killed enemy can come back with its original level and spawn point (or where it died).

- Spawning right next to the player, on screen, works: the enemy is live (full HP) on the
  first read. `setInfo + 0x88` must be 1, or `CreateEntity` returns 0; the other flags at
  `+0x70..+0x8C` didn't change anything in testing.
- Respawns that "didn't come back" while the camera faced the spot were being killed the
  instant they appeared (they landed in the player's attacks and the Pod's fire). Some areas
  also respawn enemies on their own, so the trainer only fills an area back up to the
  population it had when the enemy died.

## Pitfalls found on the way

- Cheat Engine's Auto Assembler encodes `imul r10, r10, D0` with an 8-bit immediate, i.e.
  `-0x30`. The log then wrote over the hook code. Use `shl` with a power-of-two stride.
- `alloc(name, 8000)` in Cheat Engine is **decimal** (8000 bytes, not `0x8000`).
