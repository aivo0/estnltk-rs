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
}
