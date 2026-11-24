//! A tiny x86-64 code builder: raw instruction bytes plus labels and relative fixups.
//!
//! The cheats need only a handful of instructions, so they are written as bytes (with the
//! mnemonic next to each one) and this only resolves the jump targets and RIP-relative
//! operands once the code's final address is known.

use std::collections::HashMap;

pub enum Target {
    Label(&'static str),
    Abs(u64),
}

enum Kind {
    Rel32,
    Rel8,
}

struct Fixup {
    /// Where the displacement goes.
    at: usize,
    /// Offset of the end of the instruction (what the displacement is relative to).
    end: usize,
    target: Target,
    kind: Kind,
}

pub struct Asm {
    base: u64,
    code: Vec<u8>,
    labels: HashMap<&'static str, usize>,
    fixups: Vec<Fixup>,
}

impl Asm {
    /// Code that will live at `base`.
    pub fn new(base: u64) -> Self {
        Self {
            base,
            code: Vec::new(),
            labels: HashMap::new(),
            fixups: Vec::new(),
        }
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.code.extend_from_slice(b);
        self
    }

    pub fn label(&mut self, name: &'static str) -> &mut Self {
        self.labels.insert(name, self.code.len());
        self
    }

    /// A 32-bit displacement relative to the end of the instruction; `trailing` is the size
    /// of any immediate that follows it inside the same instruction.
    pub fn rel32(&mut self, target: Target, trailing: usize) -> &mut Self {
        let at = self.code.len();
        self.code.extend_from_slice(&[0; 4]);
        self.fixups.push(Fixup {
            at,
            end: at + 4 + trailing,
            target,
            kind: Kind::Rel32,
        });
        self
    }

    /// An 8-bit displacement (short jumps), always the last byte of its instruction.
    pub fn rel8(&mut self, label: &'static str) -> &mut Self {
        let at = self.code.len();
        self.code.push(0);
        self.fixups.push(Fixup {
            at,
            end: at + 1,
            target: Target::Label(label),
            kind: Kind::Rel8,
        });
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    /// Resolves every fixup. Fails if a target is out of reach or a label is missing.
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        for f in &self.fixups {
            let target = match &f.target {
                Target::Label(name) => {
                    let off = *self
                        .labels
                        .get(name)
                        .ok_or_else(|| format!("missing label {name}"))?;
                    self.base + off as u64
                }
                Target::Abs(a) => *a,
            };
            let from = self.base as i64 + f.end as i64;
            let delta = target as i64 - from;
            match f.kind {
                Kind::Rel32 => {
                    let d = i32::try_from(delta).map_err(|_| "jump out of 32-bit reach")?;
                    self.code[f.at..f.at + 4].copy_from_slice(&d.to_le_bytes());
                }
                Kind::Rel8 => {
                    let d = i8::try_from(delta).map_err(|_| "short jump out of reach")?;
                    self.code[f.at] = d as u8;
                }
            }
        }
        Ok(self.code)
    }
}

/// `jmp target` (5 bytes) followed by NOPs up to `len` bytes, to overwrite an instruction.
pub fn jmp_patch(at: u64, target: u64, len: usize) -> Result<Vec<u8>, String> {
    let mut a = Asm::new(at);
    a.bytes(&[0xE9]).rel32(Target::Abs(target), 0);
    let mut code = a.finish()?;
    code.resize(len, 0x90);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_labels_and_absolute_targets() {
        let mut a = Asm::new(0x1000);
        a.bytes(&[0xEB]).rel8("end"); // jmp short end
        a.bytes(&[0x90]);
        a.label("end");
        a.bytes(&[0xE9]).rel32(Target::Abs(0x2000), 0); // jmp 0x2000
        let code = a.finish().unwrap();
        assert_eq!(code[1], 1);
        // jmp at 0x1003, ends at 0x1008: 0x2000 - 0x1008 = 0xFF8
        assert_eq!(&code[4..8], &0xFF8i32.to_le_bytes());
    }

    #[test]
    fn rip_operand_with_trailing_immediate() {
        // cmp dword ptr [rip+x],1 at 0x1000 targeting 0x900: 83 3D disp32 01, 7 bytes.
        let mut a = Asm::new(0x1000);
        a.bytes(&[0x83, 0x3D])
            .rel32(Target::Abs(0x900), 1)
            .bytes(&[0x01]);
        let code = a.finish().unwrap();
        assert_eq!(&code[2..6], &(0x900i32 - 0x1007).to_le_bytes());
    }

    #[test]
    fn jmp_patch_pads_with_nops() {
        let p = jmp_patch(0x1000, 0x1100, 8).unwrap();
        assert_eq!(p, vec![0xE9, 0xFB, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90]);
    }
}
