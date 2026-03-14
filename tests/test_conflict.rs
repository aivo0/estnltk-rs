use estnltk_regex_rs::conflict::*;
use estnltk_regex_rs::types::MatchSpan;

fn span(s: usize, e: usize) -> MatchSpan {
    MatchSpan::new(s, e)
}

// ---- keep_maximal_matches integration tests ----

#[test]
fn test_maximal_chain_of_enclosures() {
    // (0,20) covers (1,19) covers (2,18) → keep only (0,20)
    let input = vec![(span(0, 20), 0), (span(1, 19), 1), (span(2, 18), 2)];
    let result = keep_maximal_matches(&input);
    assert_eq!(result, vec![(span(0, 20), 0)]);
}

#[test]
fn test_maximal_adjacent_non_overlapping() {
    let input = vec![(span(0, 5), 0), (span(5, 10), 1), (span(10, 15), 2)];
    let result = keep_maximal_matches(&input);
    assert_eq!(result, input);
}

#[test]
fn test_maximal_partial_overlap_no_enclosure() {
    // (0,7) and (5,12) overlap but neither encloses the other → keep both
    let input = vec![(span(0, 7), 0), (span(5, 12), 1)];
    let result = keep_maximal_matches(&input);
    assert_eq!(result, input);
}

// ---- keep_minimal_matches integration tests ----

#[test]
fn test_minimal_chain_of_enclosures() {
    // (0,20) encloses (1,19) encloses (2,18) → keep only (2,18)
    let input = vec![(span(0, 20), 0), (span(1, 19), 1), (span(2, 18), 2)];
    let result = keep_minimal_matches(&input);
    assert_eq!(result, vec![(span(2, 18), 2)]);
}

#[test]
fn test_minimal_adjacent_non_overlapping() {
    let input = vec![(span(0, 5), 0), (span(5, 10), 1), (span(10, 15), 2)];
    let result = keep_minimal_matches(&input);
    assert_eq!(result, input);
}

#[test]
fn test_minimal_partial_overlap_no_enclosure() {
    // (0,7) and (5,12) overlap but neither encloses the other → keep both
    let input = vec![(span(0, 7), 0), (span(5, 12), 1)];
    let result = keep_minimal_matches(&input);
    assert_eq!(result, input);
}

// ---- conflict_priority_resolver integration tests ----

#[test]
fn test_priority_three_way_overlap() {
    // Three overlapping spans, same group, priorities 0, 1, 2.
    // Priority 2 removed by both 0 and 1; priority 1 removed by 0.
    // Only priority 0 survives.
    let input = vec![(span(0, 10), 0), (span(5, 15), 1), (span(8, 20), 2)];
    let groups = vec![0, 0, 0];
    let priorities = vec![0, 1, 2];
    let result = conflict_priority_resolver(&input, &groups, &priorities);
    assert_eq!(result, vec![(span(0, 10), 0)]);
}

#[test]
fn test_priority_equal_priority_no_removal() {
    // Same priority → no removal.
    let input = vec![(span(0, 10), 0), (span(5, 15), 1)];
    let groups = vec![0, 0];
    let priorities = vec![0, 0];
    let result = conflict_priority_resolver(&input, &groups, &priorities);
    assert_eq!(result, input);
}
