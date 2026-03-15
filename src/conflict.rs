use std::borrow::Cow;

use crate::types::{ConflictStrategy, MatchSpan};

/// Index of the rule that produced a match, used to look up group/priority.
pub type RuleIndex = usize;

/// A match with its source rule index.
pub type MatchEntry = (MatchSpan, RuleIndex);

/// Remove spans that are covered by another span.
///
/// Input must be canonically sorted: by (start ASC, end ASC).
/// A span A covers span B if A.start <= B.start && B.end <= A.end.
///
/// Direct port of `keep_maximal_matches` from helper_methods.py.
pub fn keep_maximal_matches(sorted: &[MatchEntry]) -> Vec<MatchEntry> {
    if sorted.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut iter = sorted.iter();
    let mut current = *iter.next().unwrap();

    loop {
        let next = match iter.next() {
            Some(&e) => e,
            None => {
                result.push(current);
                break;
            }
        };

        // If next shares start with current, next has end >= current.end
        // (canonical order), so current is covered by next → skip current.
        if current.0.start == next.0.start {
            current = next;
            continue;
        }

        // Current is not subsumed at the start. Emit it.
        result.push(current);

        // Skip following spans whose end <= current.end (covered by current).
        let mut candidate = next;
        while candidate.0.end <= current.0.end {
            match iter.next() {
                Some(&e) => candidate = e,
                None => return result,
            }
        }

        current = candidate;
    }

    result
}

/// Remove spans that enclose another smaller span.
///
/// Input must be canonically sorted: by (start ASC, end ASC).
///
/// Direct port of `keep_minimal_matches` from helper_methods.py.
pub fn keep_minimal_matches(sorted: &[MatchEntry]) -> Vec<MatchEntry> {
    if sorted.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut work_list: Vec<MatchEntry> = Vec::new();
    let mut next_work_list: Vec<MatchEntry> = Vec::new();

    for &current in sorted {
        next_work_list.clear();
        let mut add_current = true;

        for &candidate in &work_list {
            // Candidate has same start as current → candidate is shorter or equal,
            // so candidate is inside current. Keep candidate, skip current.
            if current.0.start == candidate.0.start {
                next_work_list.push(candidate);
                add_current = false;
                break;
            }

            // No further span can be inside the candidate (candidate ends before current starts).
            if candidate.0.end < current.0.start {
                result.push(candidate);
                continue;
            }

            // Current is NOT inside the candidate (candidate ends before current ends).
            // Keep candidate in worklist.
            if candidate.0.end < current.0.end {
                next_work_list.push(candidate);
            }
            // else: candidate.0.end >= current.0.end → current IS inside candidate → drop candidate
        }

        if add_current {
            next_work_list.push(current);
        }
        std::mem::swap(&mut work_list, &mut next_work_list);
    }

    // Flush remaining worklist.
    result.extend(work_list);
    result
}

/// For overlapping spans in the same group, remove the one with higher
/// priority number (lower precedence).
///
/// Direct port of `conflict_priority_resolver` from helper_methods.py.
/// O(n^2) matching Python behavior.
pub fn conflict_priority_resolver(
    sorted: &[MatchEntry],
    groups: &[i32],    // group for each entry
    priorities: &[i32], // priority for each entry
) -> Vec<MatchEntry> {
    assert_eq!(sorted.len(), groups.len());
    assert_eq!(sorted.len(), priorities.len());

    let n = sorted.len();
    let mut deleted = vec![false; n];

    for i in 0..n {
        if deleted[i] {
            continue;
        }
        for j in 0..n {
            if i == j || deleted[j] {
                continue;
            }
            // Same group?
            if groups[i] != groups[j] {
                continue;
            }
            // Overlapping? (start1 <= end2 && end1 >= start2)
            let a = &sorted[i].0;
            let b = &sorted[j].0;
            if a.start <= b.end && a.end >= b.start {
                // Remove the one with higher priority number
                if priorities[i] > priorities[j] {
                    deleted[i] = true;
                    break;
                }
            }
        }
    }

    sorted
        .iter()
        .enumerate()
        .filter(|(i, _)| !deleted[*i])
        .map(|(_, e)| *e)
        .collect()
}

/// Unified conflict resolution for all tagger types.
///
/// Returns `Cow::Borrowed(sorted)` for `KeepAll` (zero-copy), `Cow::Owned`
/// for all other strategies.
///
/// `group_priority_fn` maps each entry's index field to `(group, priority)`.
/// The index semantics vary by tagger:
/// - RegexTagger/SpanTagger: index is a rule index
/// - SubstringTagger: index is an AC pattern_id
pub fn resolve_conflicts<'a, F>(
    strategy: ConflictStrategy,
    sorted: &'a [MatchEntry],
    group_priority_fn: F,
) -> Cow<'a, [MatchEntry]>
where
    F: Fn(usize) -> (i32, i32),
{
    match strategy {
        ConflictStrategy::KeepAll => Cow::Borrowed(sorted),
        ConflictStrategy::KeepMaximal => Cow::Owned(keep_maximal_matches(sorted)),
        ConflictStrategy::KeepMinimal => Cow::Owned(keep_minimal_matches(sorted)),
        ConflictStrategy::KeepAllExceptPriority => {
            let (groups, priorities) = extract_group_priority(sorted, &group_priority_fn);
            Cow::Owned(conflict_priority_resolver(sorted, &groups, &priorities))
        }
        ConflictStrategy::KeepMaximalExceptPriority => {
            let (groups, priorities) = extract_group_priority(sorted, &group_priority_fn);
            let after_priority = conflict_priority_resolver(sorted, &groups, &priorities);
            Cow::Owned(keep_maximal_matches(&after_priority))
        }
        ConflictStrategy::KeepMinimalExceptPriority => {
            let (groups, priorities) = extract_group_priority(sorted, &group_priority_fn);
            let after_priority = conflict_priority_resolver(sorted, &groups, &priorities);
            Cow::Owned(keep_minimal_matches(&after_priority))
        }
    }
}

fn extract_group_priority<F>(entries: &[MatchEntry], f: &F) -> (Vec<i32>, Vec<i32>)
where
    F: Fn(usize) -> (i32, i32),
{
    let mut groups = Vec::with_capacity(entries.len());
    let mut priorities = Vec::with_capacity(entries.len());
    for &(_, idx) in entries {
        let (g, p) = f(idx);
        groups.push(g);
        priorities.push(p);
    }
    (groups, priorities)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(s: usize, e: usize) -> MatchSpan {
        MatchSpan::new(s, e)
    }

    // ---- keep_maximal_matches tests ----

    #[test]
    fn test_maximal_empty() {
        assert_eq!(keep_maximal_matches(&[]), vec![]);
    }

    #[test]
    fn test_maximal_no_overlap() {
        let input = vec![(span(0, 3), 0), (span(5, 8), 1)];
        let result = keep_maximal_matches(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_maximal_covered_span() {
        // (0,10) covers (2,5) → remove (2,5)
        let input = vec![(span(0, 10), 0), (span(2, 5), 1)];
        let result = keep_maximal_matches(&input);
        assert_eq!(result, vec![(span(0, 10), 0)]);
    }

    #[test]
    fn test_maximal_same_start() {
        // same start: keep the longer one
        let input = vec![(span(0, 5), 0), (span(0, 10), 1)];
        let result = keep_maximal_matches(&input);
        assert_eq!(result, vec![(span(0, 10), 1)]);
    }

    #[test]
    fn test_maximal_muna_ja_kana() {
        // From test_custom_conflict_resolver.py:
        // Patterns m..a.ja, ja, ja.k..a on "muna ja kana." (lowercase)
        // KEEP_ALL: (0,7), (5,7), (5,12)
        // KEEP_MAXIMAL should keep (0,7) and (5,12)
        // (5,7) is covered by (5,12) since same start → skip shorter
        let input = vec![(span(0, 7), 0), (span(5, 7), 1), (span(5, 12), 2)];
        let result = keep_maximal_matches(&input);
        assert_eq!(result, vec![(span(0, 7), 0), (span(5, 12), 2)]);
    }

    // ---- keep_minimal_matches tests ----

    #[test]
    fn test_minimal_empty() {
        assert_eq!(keep_minimal_matches(&[]), vec![]);
    }

    #[test]
    fn test_minimal_no_overlap() {
        let input = vec![(span(0, 3), 0), (span(5, 8), 1)];
        let result = keep_minimal_matches(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_minimal_enclosing_span() {
        // (0,10) encloses (2,5) → remove (0,10)
        let input = vec![(span(0, 10), 0), (span(2, 5), 1)];
        let result = keep_minimal_matches(&input);
        assert_eq!(result, vec![(span(2, 5), 1)]);
    }

    #[test]
    fn test_minimal_same_start() {
        // same start: (0,5) is inside (0,10) → keep (0,5)
        let input = vec![(span(0, 5), 0), (span(0, 10), 1)];
        let result = keep_minimal_matches(&input);
        assert_eq!(result, vec![(span(0, 5), 0)]);
    }

    #[test]
    fn test_minimal_muna_ja_kana() {
        // From test_custom_conflict_resolver.py:
        // KEEP_MINIMAL on (0,7), (5,7), (5,12):
        // - (0,7) encloses (5,7) → drop (0,7)
        // - (5,12) encloses (5,7) → drop (5,12)
        // Result: (5,7) only
        let input = vec![(span(0, 7), 0), (span(5, 7), 1), (span(5, 12), 2)];
        let result = keep_minimal_matches(&input);
        assert_eq!(result, vec![(span(5, 7), 1)]);
    }

    // ---- conflict_priority_resolver tests ----

    #[test]
    fn test_priority_empty() {
        assert_eq!(conflict_priority_resolver(&[], &[], &[]), vec![]);
    }

    #[test]
    fn test_priority_higher_number_removed() {
        // Two overlapping spans in same group, priorities 0 and 1.
        // Priority 1 (higher number = lower precedence) should be removed.
        let input = vec![(span(0, 5), 0), (span(3, 8), 1)];
        let groups = vec![0, 0];
        let priorities = vec![0, 1];
        let result = conflict_priority_resolver(&input, &groups, &priorities);
        assert_eq!(result, vec![(span(0, 5), 0)]);
    }

    #[test]
    fn test_priority_different_groups() {
        // Different groups — no conflict resolution.
        let input = vec![(span(0, 5), 0), (span(3, 8), 1)];
        let groups = vec![0, 1];
        let priorities = vec![1, 0];
        let result = conflict_priority_resolver(&input, &groups, &priorities);
        assert_eq!(result, input);
    }

    #[test]
    fn test_priority_no_overlap() {
        // Same group, but no overlap.
        let input = vec![(span(0, 3), 0), (span(5, 8), 1)];
        let groups = vec![0, 0];
        let priorities = vec![1, 0];
        let result = conflict_priority_resolver(&input, &groups, &priorities);
        assert_eq!(result, input);
    }
}
