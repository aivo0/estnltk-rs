use estnltk_core::TaggerError;

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
pub fn build_choice_group_pattern(patterns: &[String]) -> Result<String, TaggerError> {
    if patterns.is_empty() {
        return Err(TaggerError::PatternComposition("patterns list must not be empty".to_string()));
    }

    // Validate each pattern compiles
    for pattern in patterns {
        resharp::Regex::new(pattern)
            .map_err(|e| TaggerError::InvalidRegex(format!("Invalid regex pattern '{}': {}", pattern, e)))?;
    }

    if patterns.len() == 1 {
        Ok(format!("(?:{})", patterns[0]))
    } else {
        Ok(format!("(?:{})", patterns.join("|")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }

    #[test]
    fn test_choice_group_invalid_pattern() {
        let patterns = vec![r"\d+".to_string(), r"[unclosed".to_string()];
        let result = build_choice_group_pattern(&patterns);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("[unclosed"));
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
}
