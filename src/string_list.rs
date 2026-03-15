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
}
