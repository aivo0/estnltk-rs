use std::collections::HashMap;

use estnltk_core::TaggerError;

use crate::string_list::build_string_list_pattern;

/// Merge multiple string lists into a single choice group with longest-first sorting.
///
/// This is the Rust port of EstNLTK's `ChoiceGroup` optimized merge for compatible
/// `StringList` children from the `regex_library` subpackage.
///
/// When all sub-expressions are `StringList`-s with the same character replacements,
/// `ChoiceGroup` merges all strings into a single list and sorts by length (longest
/// first) to guarantee that the longest match is found first. This function
/// implements that merge.
///
/// # Arguments
/// * `string_lists` - List of string lists to merge
/// * `replacements` - Shared character-to-regex replacement map (must be the same for
///   all string lists — matching EstNLTK's compatibility requirement)
/// * `ignore_case` - Global flag: convert all strings to case-insensitive form
/// * `ignore_case_flags_per_list` - Optional per-list case sensitivity flags. Each
///   inner `Vec<bool>` must match the length of its corresponding string list.
///
/// # Returns
/// A regex pattern string with all strings merged and sorted longest-first.
pub fn build_merged_string_lists_pattern(
    string_lists: &[Vec<String>],
    replacements: &HashMap<String, String>,
    ignore_case: bool,
    ignore_case_flags_per_list: Option<&[Vec<bool>]>,
) -> Result<String, TaggerError> {
    if string_lists.is_empty() {
        return Err(TaggerError::PatternComposition("string_lists must not be empty".to_string()));
    }

    if let Some(flags_lists) = ignore_case_flags_per_list {
        if flags_lists.len() != string_lists.len() {
            return Err(TaggerError::PatternComposition(format!(
                "ignore_case_flags_per_list length ({}) must match string_lists length ({})",
                flags_lists.len(),
                string_lists.len()
            )));
        }
        for (i, (strings, flags)) in string_lists.iter().zip(flags_lists.iter()).enumerate() {
            if flags.len() != strings.len() {
                return Err(TaggerError::PatternComposition(format!(
                    "ignore_case_flags_per_list[{}] length ({}) must match string_lists[{}] length ({})",
                    i, flags.len(), i, strings.len()
                )));
            }
        }
    }

    // Merge all strings and flags into flat lists
    let mut all_strings: Vec<String> = Vec::new();
    let mut all_flags: Vec<bool> = Vec::new();

    for (i, strings) in string_lists.iter().enumerate() {
        all_strings.extend(strings.iter().cloned());
        if let Some(flags_lists) = ignore_case_flags_per_list {
            all_flags.extend(flags_lists[i].iter().cloned());
        }
    }

    let flags_ref = if all_flags.is_empty() {
        None
    } else {
        Some(all_flags.as_slice())
    };

    build_string_list_pattern(&all_strings, replacements, ignore_case, flags_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merged_string_lists_basic() {
        let lists = vec![
            vec!["cat".to_string(), "dog".to_string()],
            vec!["elephant".to_string(), "ant".to_string()],
        ];
        let result =
            build_merged_string_lists_pattern(&lists, &HashMap::new(), false, None).unwrap();
        // All merged and sorted by length (longest first), then alphabetically: elephant, ant, cat, dog
        assert_eq!(result, "(?:elephant|ant|cat|dog)");
    }

    #[test]
    fn test_merged_string_lists_empty_error() {
        let result =
            build_merged_string_lists_pattern(&[], &HashMap::new(), false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_merged_string_lists_with_case_flags() {
        let lists = vec![
            vec!["Ab".to_string()],
            vec!["Cd".to_string()],
        ];
        let flags = vec![vec![true], vec![false]];
        let result = build_merged_string_lists_pattern(
            &lists,
            &HashMap::new(),
            false,
            Some(&flags),
        )
        .unwrap();
        // "Ab" case-insensitive, "Cd" literal; both 2 chars, "Ab" < "Cd" alphabetically
        assert_eq!(result, "(?:[Aa][Bb]|Cd)");
    }

    #[test]
    fn test_merged_string_lists_with_replacements() {
        let lists = vec![
            vec![" punkt".to_string()],
            vec![" pall".to_string()],
        ];
        let mut replacements = HashMap::new();
        replacements.insert(" ".to_string(), r"\s+".to_string());
        let result =
            build_merged_string_lists_pattern(&lists, &replacements, false, None).unwrap();
        assert_eq!(result, "(?:(?:\\s+)punkt|(?:\\s+)pall)");
    }

    #[test]
    fn test_merged_string_lists_deduplication() {
        let lists = vec![
            vec!["cat".to_string(), "dog".to_string()],
            vec!["cat".to_string(), "fish".to_string()],
        ];
        let result =
            build_merged_string_lists_pattern(&lists, &HashMap::new(), false, None).unwrap();
        // "cat" appears in both lists but should be deduplicated
        assert_eq!(result, "(?:fish|cat|dog)");
    }

    #[test]
    fn test_merged_string_lists_flags_length_mismatch() {
        let lists = vec![
            vec!["a".to_string()],
            vec!["b".to_string()],
        ];
        let flags = vec![vec![true]]; // only 1 flags list for 2 string lists
        let result = build_merged_string_lists_pattern(
            &lists,
            &HashMap::new(),
            false,
            Some(&flags),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_merged_string_lists_inner_flags_length_mismatch() {
        let lists = vec![vec!["a".to_string(), "b".to_string()]];
        let flags = vec![vec![true]]; // 1 flag for 2 strings
        let result = build_merged_string_lists_pattern(
            &lists,
            &HashMap::new(),
            false,
            Some(&flags),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_merged_string_lists_global_ignore_case() {
        let lists = vec![
            vec!["Ab".to_string()],
            vec!["Cd".to_string()],
        ];
        let result =
            build_merged_string_lists_pattern(&lists, &HashMap::new(), true, None).unwrap();
        assert_eq!(result, "(?:[Aa][Bb]|[Cc][Dd])");
    }

    #[test]
    fn test_merged_string_lists_estonian() {
        let lists = vec![
            vec!["täna".to_string(), "homme".to_string()],
            vec!["üleeile".to_string(), "eile".to_string()],
        ];
        let result =
            build_merged_string_lists_pattern(&lists, &HashMap::new(), false, None).unwrap();
        // Sorted by byte length: üleeile (9), homme (5), täna (5, ä=2 bytes), eile (4)
        assert_eq!(result, "(?:üleeile|homme|täna|eile)");
    }
}
