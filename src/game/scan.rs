//! Array-of-bytes signatures ("48 8B ?? 05") over a copy of the game's module.

pub struct Pattern {
    bytes: Vec<Option<u8>>,
}

impl Pattern {
    pub fn parse(text: &str) -> Self {
        let bytes = text
            .split_whitespace()
            .map(|t| {
                if t.starts_with('?') {
                    None
                } else {
                    Some(u8::from_str_radix(t, 16).expect("bad byte in signature"))
                }
            })
            .collect();
        Self { bytes }
    }

    fn matches_at(&self, hay: &[u8], i: usize) -> bool {
        self.bytes
            .iter()
            .enumerate()
            .all(|(k, b)| b.is_none_or(|b| hay[i + k] == b))
    }

    /// Every offset where the pattern matches.
    pub fn find_all(&self, hay: &[u8]) -> Vec<usize> {
        let n = self.bytes.len();
        if n == 0 || hay.len() < n {
            return Vec::new();
        }
        // Anchor on the first fixed byte to skip quickly.
        let (anchor_at, anchor) = self
            .bytes
            .iter()
            .enumerate()
            .find_map(|(k, b)| b.map(|b| (k, b)))
            .unwrap_or((0, 0));
        let mut out = Vec::new();
        let last = hay.len() - n;
        let mut i = 0;
        while i <= last {
            match hay[i + anchor_at..=last + anchor_at]
                .iter()
                .position(|&c| c == anchor)
            {
                Some(p) => {
                    let start = i + p;
                    if self.matches_at(hay, start) {
                        out.push(start);
                    }
                    i = start + 1;
                }
                None => break,
            }
        }
        out
    }

    /// The only match, or `None` when there are zero or several.
    pub fn find_unique(&self, hay: &[u8]) -> Option<usize> {
        let all = self.find_all(hay);
        (all.len() == 1).then(|| all[0])
    }
}

/// Target of a RIP-relative operand: `instr + len + disp32` where the displacement sits at
/// `instr + disp_at`.
pub fn rip_target(image: &[u8], image_base: u64, instr: usize, disp_at: usize, len: usize) -> u64 {
    let d = i32::from_le_bytes(
        image[instr + disp_at..instr + disp_at + 4]
            .try_into()
            .unwrap(),
    );
    (image_base as i64 + instr as i64 + len as i64 + d as i64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_and_anchor() {
        let hay = [0x90, 0x48, 0x8B, 0x05, 0x11, 0x48, 0x8B, 0x0D, 0x22];
        let p = Pattern::parse("48 8B ?? 11");
        assert_eq!(p.find_all(&hay), vec![1]);
        let p = Pattern::parse("?? 8B");
        assert_eq!(p.find_all(&hay), vec![1, 5]);
        assert_eq!(Pattern::parse("48 8B 0D").find_unique(&hay), Some(5));
    }
}
