/// Build a mapping from byte index → character index for a UTF-8 string.
///
/// Returns a Vec of length `text.len() + 1` where `map[byte_idx]` gives
/// the character index at that byte position. Only positions that are
/// char boundaries have meaningful values; intermediate bytes within a
/// multi-byte char carry the *next* char index.
///
/// This lets us convert resharp's byte-offset matches to EstNLTK's
/// character-offset spans in O(1) per match after O(n) precomputation.
pub fn byte_to_char_map(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut map = vec![0usize; bytes.len() + 1];
    let mut char_idx = 0usize;
    let mut byte_idx = 0usize;
    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        for offset in 0..ch_len {
            map[byte_idx + offset] = char_idx;
        }
        byte_idx += ch_len;
        char_idx += 1;
    }
    // Sentinel: map[text.len()] = total char count
    map[byte_idx] = char_idx;
    map
}

/// Build a mapping from character index → byte index for a UTF-8 string.
///
/// Returns a Vec of length `char_count + 1` where `map[char_idx]` gives
/// the byte offset of that character.  The sentinel `map[char_count]`
/// equals `text.len()`.
///
/// Useful for converting character-offset spans back to byte offsets
/// for O(1) string slicing (e.g., `&text[c2b[start]..c2b[end]]`).
pub fn char_to_byte_map(text: &str) -> Vec<usize> {
    let char_count = text.chars().count();
    let mut map = Vec::with_capacity(char_count + 1);
    for (byte_idx, _) in text.char_indices() {
        map.push(byte_idx);
    }
    map.push(text.len());
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_only() {
        let text = "hello";
        let map = byte_to_char_map(text);
        assert_eq!(map.len(), 6);
        for i in 0..=5 {
            assert_eq!(map[i], i);
        }
    }

    #[test]
    fn test_estonian_multibyte() {
        // 'ä' is 2 bytes in UTF-8 (0xC3 0xA4)
        let text = "Tüüpiline";
        let map = byte_to_char_map(text);
        // T(1) ü(2) ü(2) p(1) i(1) l(1) i(1) n(1) e(1) = 11 bytes, 9 chars
        assert_eq!(text.len(), 11);
        assert_eq!(map[text.len()], 9);
        // 'T' at byte 0 → char 0
        assert_eq!(map[0], 0);
        // 'ü' at bytes 1,2 → char 1
        assert_eq!(map[1], 1);
        assert_eq!(map[2], 1);
        // second 'ü' at bytes 3,4 → char 2
        assert_eq!(map[3], 2);
        assert_eq!(map[4], 2);
        // 'p' at byte 5 → char 3
        assert_eq!(map[5], 3);
    }

    #[test]
    fn test_mixed_estonian() {
        let text = "öökülma";
        // ö(2) ö(2) k(1) ü(2) l(1) m(1) a(1) = 10 bytes, 7 chars
        let map = byte_to_char_map(text);
        assert_eq!(text.len(), 10);
        assert_eq!(map[text.len()], 7);
    }

    #[test]
    fn test_empty() {
        let map = byte_to_char_map("");
        assert_eq!(map.len(), 1);
        assert_eq!(map[0], 0);
    }

    // ---- char_to_byte_map tests ----

    #[test]
    fn test_c2b_ascii() {
        let text = "hello";
        let map = char_to_byte_map(text);
        assert_eq!(map, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_c2b_estonian_multibyte() {
        let text = "Tüüpiline";
        // T(1) ü(2) ü(2) p(1) i(1) l(1) i(1) n(1) e(1) = 11 bytes, 9 chars
        let map = char_to_byte_map(text);
        assert_eq!(map.len(), 10); // 9 chars + sentinel
        assert_eq!(map[0], 0);  // T
        assert_eq!(map[1], 1);  // ü at byte 1
        assert_eq!(map[2], 3);  // ü at byte 3
        assert_eq!(map[3], 5);  // p at byte 5
        assert_eq!(map[9], 11); // sentinel = text.len()
    }

    #[test]
    fn test_c2b_empty() {
        let map = char_to_byte_map("");
        assert_eq!(map, vec![0]);
    }

    #[test]
    fn test_c2b_roundtrip() {
        let text = "öökülma";
        let b2c = byte_to_char_map(text);
        let c2b = char_to_byte_map(text);
        // Slicing via char offsets should produce correct substrings
        let char_start = 2; // 'k'
        let char_end = 5;   // 'l', 'm', 'a' → "kül"
        let slice = &text[c2b[char_start]..c2b[char_end]];
        assert_eq!(slice, "kül");
        // And the byte offsets should round-trip through b2c
        assert_eq!(b2c[c2b[char_start]], char_start);
        assert_eq!(b2c[c2b[char_end]], char_end);
    }
}
