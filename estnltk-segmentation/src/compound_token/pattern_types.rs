use regex::Regex;

/// How to normalize the matched text of a compound token.
#[derive(Debug, Clone)]
pub enum NormalizationAction {
    /// No normalization (lambda m: None)
    None,
    /// Remove all whitespace from the full match (lambda m: WHITESPACEPAT.sub('', m.group(0)))
    StripWhitespace,
    /// Remove all whitespace from a specific capture group
    StripWhitespaceGroup(usize),
    /// Smart period handling for dates/numerics (_numeric_with_period_normalizer)
    NumericWithPeriodNormalizer,
    /// Remove all periods from the match
    StripPeriods,
    /// Compact periods with spaces: remove spaces around periods (for abbreviations)
    CompactPeriods,
    /// Collapse multiple whitespace to single space
    CollapseWhitespace,
}

/// A compiled compound token pattern.
#[derive(Debug)]
pub struct CompoundTokenPattern {
    /// The compiled regex
    pub regex: Regex,
    /// Pattern type label (e.g., "email", "numeric_date", "emoticon")
    pub pattern_type: String,
    /// Which capture group to use (0 = full match)
    pub group: usize,
    /// Priority tuple flattened to a single sortable integer
    pub priority: i64,
    /// How to normalize the matched text
    pub normalization: NormalizationAction,
    /// Whether this is a negative (filtering) pattern
    pub is_negative: bool,
    /// For 2nd level patterns: left boundary strictness
    pub left_strict: Option<bool>,
    /// For 2nd level patterns: right boundary strictness
    pub right_strict: Option<bool>,
}

/// Flatten a priority tuple (up to 4 elements) into a single sortable i64.
/// Each element occupies 16 bits. E.g. (2, 0, 3) -> 0x000200000003
pub fn flatten_priority(parts: &[i32]) -> i64 {
    let mut result: i64 = 0;
    for (i, &part) in parts.iter().enumerate() {
        if i >= 4 {
            break;
        }
        result |= (part as i64 & 0xFFFF) << (48 - 16 * i);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        let p1 = flatten_priority(&[0, 0, 1]);
        let p2 = flatten_priority(&[0, 0, 2]);
        let p3 = flatten_priority(&[1, 0, 1]);
        assert!(p1 < p2);
        assert!(p2 < p3);
    }
}
