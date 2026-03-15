use std::collections::{HashMap, HashSet};

/// Escape regex metacharacters in a string (like Python's `regex.escape()`).
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if is_regex_meta(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn is_regex_meta(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '.'
            | '+'
            | '*'
            | '?'
            | '('
            | ')'
            | '|'
            | '['
            | ']'
            | '{'
            | '}'
            | '^'
            | '$'
            | '#'
            | '&'
            | '-'
            | '~'
    )
}

/// Convert a pattern string to case-insensitive form by replacing each
/// alphabetic character with `[Xx]` (uppercase + lowercase).
///
/// Preserves characters preceded by a backslash (escaped characters).
/// Matches EstNLTK's `StringList.make_case_insensitive()`.
fn make_case_insensitive(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len() * 4);
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            // Escaped character — copy both the backslash and the next char verbatim
            result.push(chars[i]);
            result.push(chars[i + 1]);
            i += 2;
        } else if chars[i].is_alphabetic() {
            let upper = chars[i].to_uppercase().to_string();
            let lower = chars[i].to_lowercase().to_string();
            if upper != lower {
                result.push('[');
                result.push_str(&upper);
                result.push_str(&lower);
                result.push(']');
            } else {
                // Non-cased alphabetic character (e.g., some Unicode symbols)
                result.push(chars[i]);
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Apply character replacements to an escaped pattern string.
///
/// Each replacement maps a single character (after escaping) to a regex pattern.
/// The replacement value is wrapped in a non-capture group `(?:...)`.
///
/// Matches EstNLTK's replacement logic in `StringList.__make_choice_group()`.
fn apply_replacements(pattern: &str, replacements: &HashMap<String, String>) -> String {
    if replacements.is_empty() {
        return pattern.to_string();
    }

    // Build the escaped replacement map: escaped_key → (?:value)
    let mut escaped_map: HashMap<String, String> = HashMap::new();
    for (key, value) in replacements {
        let escaped_key = regex_escape(key);
        escaped_map.insert(escaped_key, format!("(?:{})", value));
    }

    // Sort escaped keys by length (longest first) so longer sequences are replaced first
    let mut keys: Vec<&String> = escaped_map.keys().collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

    // Build a compound regex pattern that matches any escaped replacement key
    let compound = keys
        .iter()
        .map(|k| regex_escape(k))
        .collect::<Vec<_>>()
        .join("|");

    let re = match regex::Regex::new(&compound) {
        Ok(r) => r,
        Err(_) => return pattern.to_string(),
    };

    re.replace_all(pattern, |caps: &regex::Captures| {
        let matched = caps.get(0).unwrap().as_str();
        escaped_map.get(matched).cloned().unwrap_or_else(|| matched.to_string())
    })
    .into_owned()
}

/// Build a regex alternation pattern from a list of strings.
///
/// This is the Rust port of EstNLTK's `StringList` from the `regex_library` subpackage.
///
/// The produced pattern:
/// - Wraps choices in a non-capture group: `(?:choice1|choice2|...)`
/// - Sorts strings by length (longest first), then alphabetically
/// - Escapes regex metacharacters
/// - Optionally applies case-insensitive conversion (`[Xx]` style)
/// - Optionally applies character replacement maps
/// - Deduplicates strings
///
/// # Arguments
/// * `strings` - List of literal strings to match
/// * `replacements` - Character-to-regex replacement map (e.g., `{" ": r"\s+"}`)
/// * `ignore_case` - Global flag: convert all strings to case-insensitive form
/// * `ignore_case_flags` - Per-string case sensitivity flags (overrides `ignore_case`)
///
/// # Returns
/// A regex pattern string like `(?:longest|medium|short)`, or an error message.
pub fn build_string_list_pattern(
    strings: &[String],
    replacements: &HashMap<String, String>,
    ignore_case: bool,
    ignore_case_flags: Option<&[bool]>,
) -> Result<String, String> {
    if strings.is_empty() {
        return Err("strings list must not be empty".to_string());
    }

    // Resolve per-string case flags
    let case_flags: Vec<bool> = match ignore_case_flags {
        Some(flags) => {
            if flags.len() != strings.len() {
                return Err(format!(
                    "ignore_case_flags length ({}) must match strings length ({})",
                    flags.len(),
                    strings.len()
                ));
            }
            flags.to_vec()
        }
        None => vec![ignore_case; strings.len()],
    };

    // Validate replacements: keys must be single characters
    for key in replacements.keys() {
        if key.chars().count() != 1 {
            return Err(format!(
                "Replacement key '{}' must be a single character",
                key
            ));
        }
    }

    // Sort by length (longest first), then alphabetically — preserving original indices
    let mut indexed: Vec<(usize, &String)> = strings.iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| a.1.cmp(b.1))
    });

    let mut choices: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (orig_idx, string) in indexed {
        // Deduplicate
        let dedup_key = if case_flags[orig_idx] {
            string.to_lowercase()
        } else {
            string.clone()
        };
        if seen.contains(&dedup_key) {
            continue;
        }
        seen.insert(dedup_key);

        // Escape regex metacharacters
        let mut choice = regex_escape(string);

        // Apply case-insensitive conversion if flagged
        if case_flags[orig_idx] {
            choice = make_case_insensitive(&choice);
        }

        choices.push(choice);
    }

    // Apply character replacements to all choices
    if !replacements.is_empty() {
        for choice in &mut choices {
            *choice = apply_replacements(choice, replacements);
        }
    }

    if choices.len() == 1 {
        Ok(format!("(?:{})", choices[0]))
    } else {
        Ok(format!("(?:{})", choices.join("|")))
    }
}

/// Build a regex choice group (alternation) from multiple regex patterns.
///
/// This is the Rust port of EstNLTK's `ChoiceGroup` from the `regex_library` subpackage.
///
/// The produced pattern:
/// - Wraps choices in a non-capture group: `(?:pattern1|pattern2|...)`
/// - Validates each pattern is a compilable regex (using resharp DFA engine)
///
/// When all sub-expressions are compatible `StringList`-s (same replacements),
/// use `build_merged_string_lists_pattern` instead to get longest-first sorting
/// guarantees across all lists.
///
/// # Arguments
/// * `patterns` - List of regex pattern strings to combine via alternation
///
/// # Returns
/// A regex pattern string like `(?:pattern1|pattern2|...)`, or an error message.
pub fn build_choice_group_pattern(patterns: &[String]) -> Result<String, String> {
    if patterns.is_empty() {
        return Err("patterns list must not be empty".to_string());
    }

    // Validate each pattern compiles
    for pattern in patterns {
        resharp::Regex::new(pattern)
            .map_err(|e| format!("Invalid regex pattern '{}': {}", pattern, e))?;
    }

    if patterns.len() == 1 {
        Ok(format!("(?:{})", patterns[0]))
    } else {
        Ok(format!("(?:{})", patterns.join("|")))
    }
}

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
) -> Result<String, String> {
    if string_lists.is_empty() {
        return Err("string_lists must not be empty".to_string());
    }

    if let Some(flags_lists) = ignore_case_flags_per_list {
        if flags_lists.len() != string_lists.len() {
            return Err(format!(
                "ignore_case_flags_per_list length ({}) must match string_lists length ({})",
                flags_lists.len(),
                string_lists.len()
            ));
        }
        for (i, (strings, flags)) in string_lists.iter().zip(flags_lists.iter()).enumerate() {
            if flags.len() != strings.len() {
                return Err(format!(
                    "ignore_case_flags_per_list[{}] length ({}) must match string_lists[{}] length ({})",
                    i, flags.len(), i, strings.len()
                ));
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

/// Build a regex pattern from a template with named placeholders.
///
/// This is the Rust port of EstNLTK's `RegexPattern` from the `regex_library` subpackage.
///
/// The template uses `{name}` syntax for placeholders. Each placeholder is replaced
/// with the corresponding pattern from the `components` map, wrapped in a non-capture
/// group `(?:...)` to prevent operator precedence issues with surrounding syntax.
///
/// The final composed pattern is validated with resharp to ensure it compiles.
///
/// # Arguments
/// * `template` - Template string with `{name}` placeholders (e.g., `"(?:{prefix}\\s+)?{main}"`)
/// * `components` - Map of placeholder names to regex pattern strings
///
/// # Returns
/// The composed regex pattern string, or an error if:
/// - A placeholder in the template has no corresponding entry in `components`
/// - The composed pattern fails to compile with resharp
///
/// # Examples
/// ```
/// use std::collections::HashMap;
/// use estnltk_regex_rs::string_list::build_regex_pattern;
/// let mut components = HashMap::new();
/// components.insert("prefix".to_string(), "Mr|Mrs|Dr".to_string());
/// components.insert("main".to_string(), "[A-Z][a-z]+".to_string());
/// let result = build_regex_pattern("(?:{prefix}\\s+)?{main}", &components).unwrap();
/// assert_eq!(result, "(?:(?:Mr|Mrs|Dr)\\s+)?(?:[A-Z][a-z]+)");
/// ```
pub fn build_regex_pattern(
    template: &str,
    components: &HashMap<String, String>,
) -> Result<String, String> {
    if template.is_empty() {
        return Err("template must not be empty".to_string());
    }

    // Parse template and substitute placeholders.
    // We scan for `{name}` sequences. Literal `{{` and `}}` are escaped braces.
    let mut result = String::with_capacity(template.len() * 2);
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            // Check for escaped brace `{{`
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                result.push('{');
                i += 2;
                continue;
            }
            // Find the closing brace
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            if end >= chars.len() {
                return Err(format!(
                    "Unclosed placeholder '{{' at position {}",
                    i
                ));
            }
            let name: String = chars[start..end].iter().collect();
            if name.is_empty() {
                return Err("Empty placeholder name '{}' in template".to_string());
            }
            let pattern = components.get(&name).ok_or_else(|| {
                format!(
                    "No component provided for placeholder '{{{}}}'",
                    name
                )
            })?;
            // Wrap in non-capture group to isolate from surrounding syntax
            result.push_str(&format!("(?:{})", pattern));
            i = end + 1;
        } else if chars[i] == '}' {
            // Check for escaped brace `}}`
            if i + 1 < chars.len() && chars[i + 1] == '}' {
                result.push('}');
                i += 2;
                continue;
            }
            return Err(format!(
                "Unexpected '}}' at position {} without matching '{{'",
                i
            ));
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    // Validate the composed pattern with resharp
    resharp::Regex::new(&result)
        .map_err(|e| format!("Composed pattern is invalid: {}", e))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_pattern() {
        let strings = vec!["cat".to_string(), "dog".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        // Sorted by length (equal), then alphabetically: cat, dog
        assert_eq!(result, "(?:cat|dog)");
    }

    #[test]
    fn test_longest_first_sorting() {
        let strings = vec![
            "p".to_string(),
            "palli".to_string(),
            "punkt".to_string(),
        ];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        // "palli" and "punkt" are both 5 chars, "palli" < "punkt" alphabetically
        assert_eq!(result, "(?:palli|punkt|p)");
    }

    #[test]
    fn test_special_chars_escaped() {
        let strings = vec!["a.b".to_string(), "c+d".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        assert_eq!(result, "(?:a\\.b|c\\+d)");
    }

    #[test]
    fn test_ignore_case_global() {
        let strings = vec!["Ab".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), true, None).unwrap();
        assert_eq!(result, "(?:[Aa][Bb])");
    }

    #[test]
    fn test_ignore_case_per_string() {
        let strings = vec!["Ab".to_string(), "Cd".to_string()];
        let flags = vec![true, false];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, Some(&flags))
                .unwrap();
        // Both 2 chars. "Ab" → case insensitive, "Cd" → literal
        assert_eq!(result, "(?:[Aa][Bb]|Cd)");
    }

    #[test]
    fn test_replacements() {
        let strings = vec![
            " punkt".to_string(),
            " pall".to_string(),
            " p".to_string(),
        ];
        let mut replacements = HashMap::new();
        replacements.insert(" ".to_string(), r"\s+".to_string());
        let result =
            build_string_list_pattern(&strings, &replacements, false, None).unwrap();
        assert_eq!(result, "(?:(?:\\s+)punkt|(?:\\s+)pall|(?:\\s+)p)");
    }

    #[test]
    fn test_deduplication() {
        let strings = vec![
            "cat".to_string(),
            "cat".to_string(),
            "dog".to_string(),
        ];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        assert_eq!(result, "(?:cat|dog)");
    }

    #[test]
    fn test_case_insensitive_deduplication() {
        let strings = vec!["Cat".to_string(), "cat".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), true, None).unwrap();
        // "Cat" and "cat" should deduplicate (same lowercase), keep first seen after sort
        assert_eq!(result, "(?:[Cc][Aa][Tt])");
    }

    #[test]
    fn test_empty_strings_error() {
        let result =
            build_string_list_pattern(&[], &HashMap::new(), false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_flags_length_mismatch() {
        let strings = vec!["a".to_string()];
        let flags = vec![true, false];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, Some(&flags));
        assert!(result.is_err());
    }

    #[test]
    fn test_replacement_key_not_single_char() {
        let strings = vec!["hello".to_string()];
        let mut replacements = HashMap::new();
        replacements.insert("ab".to_string(), "x".to_string());
        let result = build_string_list_pattern(&strings, &replacements, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_single_string() {
        let strings = vec!["only".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        assert_eq!(result, "(?:only)");
    }

    #[test]
    fn test_estonian_multibyte() {
        let strings = vec![
            "täna".to_string(),
            "õhtu".to_string(),
            "ööbik".to_string(),
        ];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), false, None).unwrap();
        // "ööbik" is 5 chars → first, then "täna" and "õhtu" (4 chars each, t < õ)
        assert_eq!(result, "(?:ööbik|täna|õhtu)");
    }

    #[test]
    fn test_estonian_case_insensitive() {
        let strings = vec!["Õhtu".to_string()];
        let result =
            build_string_list_pattern(&strings, &HashMap::new(), true, None).unwrap();
        assert_eq!(result, "(?:[Õõ][Hh][Tt][Uu])");
    }

    #[test]
    fn test_replacement_with_case_insensitive() {
        let strings = vec!["a b".to_string()];
        let mut replacements = HashMap::new();
        replacements.insert(" ".to_string(), r"\s+".to_string());
        let result =
            build_string_list_pattern(&strings, &replacements, true, None).unwrap();
        // 'a' → [Aa], ' ' → (?:\s+), 'b' → [Bb]
        assert_eq!(result, "(?:[Aa](?:\\s+)[Bb])");
    }

    #[test]
    fn test_multiple_replacements() {
        let strings = vec!["a-b c".to_string()];
        let mut replacements = HashMap::new();
        replacements.insert("-".to_string(), r"[\-\s]".to_string());
        replacements.insert(" ".to_string(), r"\s+".to_string());
        let result =
            build_string_list_pattern(&strings, &replacements, false, None).unwrap();
        // 'a' stays, '-' (escaped to '\-') replaced, 'b' stays, ' ' (escaped to '\ ') replaced
        assert!(result.contains("(?:[\\-\\s])"));
        assert!(result.contains("(?:\\s+)"));
    }

    // --- ChoiceGroup tests ---

    #[test]
    fn test_choice_group_basic() {
        let patterns = vec![r"\d+".to_string(), r"[a-z]+".to_string()];
        let result = build_choice_group_pattern(&patterns).unwrap();
        assert_eq!(result, r"(?:\d+|[a-z]+)");
    }

    #[test]
    fn test_choice_group_single_pattern() {
        let patterns = vec![r"\w+".to_string()];
        let result = build_choice_group_pattern(&patterns).unwrap();
        assert_eq!(result, r"(?:\w+)");
    }

    #[test]
    fn test_choice_group_empty_error() {
        let result = build_choice_group_pattern(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_choice_group_invalid_pattern() {
        let patterns = vec![r"\d+".to_string(), r"[unclosed".to_string()];
        let result = build_choice_group_pattern(&patterns);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("[unclosed"));
    }

    #[test]
    fn test_choice_group_three_patterns() {
        let patterns = vec![
            r"[A-Z][a-z]+".to_string(),
            r"\d{4}-\d{2}-\d{2}".to_string(),
            r"[a-z]+@[a-z]+\.[a-z]+".to_string(),
        ];
        let result = build_choice_group_pattern(&patterns).unwrap();
        assert_eq!(
            result,
            r"(?:[A-Z][a-z]+|\d{4}-\d{2}-\d{2}|[a-z]+@[a-z]+\.[a-z]+)"
        );
    }

    #[test]
    fn test_choice_group_estonian_patterns() {
        let patterns = vec![
            r"[öäüõ]+".to_string(),
            r"[A-ZÖÄÜÕ][a-zöäüõ]+".to_string(),
        ];
        let result = build_choice_group_pattern(&patterns).unwrap();
        assert_eq!(result, r"(?:[öäüõ]+|[A-ZÖÄÜÕ][a-zöäüõ]+)");
    }

    #[test]
    fn test_choice_group_with_lookaround() {
        // resharp supports lookaround natively
        let patterns = vec![r"(?<=\s)\d+".to_string(), r"\d+(?=\s)".to_string()];
        let result = build_choice_group_pattern(&patterns).unwrap();
        assert_eq!(result, r"(?:(?<=\s)\d+|\d+(?=\s))");
    }

    // --- Merged StringList tests ---

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

    // --- RegexPattern tests ---

    #[test]
    fn test_regex_pattern_basic() {
        let mut components = HashMap::new();
        components.insert("prefix".to_string(), "Mr|Mrs|Dr".to_string());
        components.insert("main".to_string(), "[A-Z][a-z]+".to_string());
        let result =
            build_regex_pattern(r"(?:{prefix}\s+)?{main}", &components).unwrap();
        assert_eq!(result, r"(?:(?:Mr|Mrs|Dr)\s+)?(?:[A-Z][a-z]+)");
    }

    #[test]
    fn test_regex_pattern_single_placeholder() {
        let mut components = HashMap::new();
        components.insert("digits".to_string(), r"\d+".to_string());
        let result = build_regex_pattern("{digits}", &components).unwrap();
        assert_eq!(result, r"(?:\d+)");
    }

    #[test]
    fn test_regex_pattern_no_placeholders() {
        let components = HashMap::new();
        let result = build_regex_pattern(r"\d+\s+\w+", &components).unwrap();
        assert_eq!(result, r"\d+\s+\w+");
    }

    #[test]
    fn test_regex_pattern_missing_component() {
        let components = HashMap::new();
        let result = build_regex_pattern("{missing}", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing"));
    }

    #[test]
    fn test_regex_pattern_empty_template() {
        let components = HashMap::new();
        let result = build_regex_pattern("", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_regex_pattern_unclosed_brace() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc{def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unclosed"));
    }

    #[test]
    fn test_regex_pattern_empty_placeholder() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc{}def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty placeholder"));
    }

    #[test]
    fn test_regex_pattern_escaped_braces() {
        let components = HashMap::new();
        let result = build_regex_pattern(r"\d{{3}}", &components).unwrap();
        // `{{` → `{`, `}}` → `}`, so result is `\d{3}`
        assert_eq!(result, r"\d{3}");
    }

    #[test]
    fn test_regex_pattern_with_string_list() {
        // Compose with a StringList pattern
        let titles = build_string_list_pattern(
            &["Mr".to_string(), "Mrs".to_string(), "Dr".to_string()],
            &HashMap::new(),
            false,
            None,
        )
        .unwrap();
        let mut components = HashMap::new();
        components.insert("title".to_string(), titles);
        components.insert("name".to_string(), "[A-Z][a-z]+".to_string());
        let result =
            build_regex_pattern(r"(?:{title}\s+)?{name}", &components).unwrap();
        assert_eq!(
            result,
            r"(?:(?:(?:Mrs|Dr|Mr))\s+)?(?:[A-Z][a-z]+)"
        );
    }

    #[test]
    fn test_regex_pattern_estonian() {
        let mut components = HashMap::new();
        components.insert("eesnimi".to_string(), "[A-ZÖÄÜÕ][a-zöäüõ]+".to_string());
        components.insert("perenimi".to_string(), "[A-ZÖÄÜÕ][a-zöäüõ]+".to_string());
        let result =
            build_regex_pattern(r"{eesnimi}\s+{perenimi}", &components).unwrap();
        assert_eq!(
            result,
            r"(?:[A-ZÖÄÜÕ][a-zöäüõ]+)\s+(?:[A-ZÖÄÜÕ][a-zöäüõ]+)"
        );
    }

    #[test]
    fn test_regex_pattern_multiple_same_placeholder() {
        let mut components = HashMap::new();
        components.insert("word".to_string(), r"\w+".to_string());
        let result =
            build_regex_pattern(r"{word}\s+{word}", &components).unwrap();
        assert_eq!(result, r"(?:\w+)\s+(?:\w+)");
    }

    #[test]
    fn test_regex_pattern_invalid_composed() {
        let mut components = HashMap::new();
        components.insert("bad".to_string(), "[unclosed".to_string());
        let result = build_regex_pattern("{bad}", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Composed pattern is invalid"));
    }

    #[test]
    fn test_regex_pattern_unmatched_closing_brace() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc}def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unexpected '}'"));
    }
}
