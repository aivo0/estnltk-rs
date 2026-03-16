use std::collections::HashMap;

use estnltk_core::TaggerError;

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
/// use estnltk_patterns::build_regex_pattern;
/// let mut components = HashMap::new();
/// components.insert("prefix".to_string(), "Mr|Mrs|Dr".to_string());
/// components.insert("main".to_string(), "[A-Z][a-z]+".to_string());
/// let result = build_regex_pattern("(?:{prefix}\\s+)?{main}", &components).unwrap();
/// assert_eq!(result, "(?:(?:Mr|Mrs|Dr)\\s+)?(?:[A-Z][a-z]+)");
/// ```
pub fn build_regex_pattern(
    template: &str,
    components: &HashMap<String, String>,
) -> Result<String, TaggerError> {
    if template.is_empty() {
        return Err(TaggerError::PatternComposition("template must not be empty".to_string()));
    }

    // Parse template and substitute placeholders.
    // We scan for `{name}` sequences. Literal `{{` and `}}` are escaped braces.
    // Since `{` and `}` are ASCII, we can use byte-level indexing directly.
    let mut result = String::with_capacity(template.len() * 2);
    let bytes = template.as_bytes();
    let mut i = 0;
    // Start of the current literal run (copied verbatim to result).
    let mut literal_start = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Flush preceding literal text.
            result.push_str(&template[literal_start..i]);
            // Check for escaped brace `{{`
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                result.push('{');
                i += 2;
                literal_start = i;
                continue;
            }
            // Find the closing brace
            let name_start = i + 1;
            let end = bytes[name_start..]
                .iter()
                .position(|&b| b == b'}')
                .map(|pos| name_start + pos)
                .ok_or_else(|| {
                    TaggerError::PatternComposition(format!(
                        "Unclosed placeholder '{{' at position {}", i
                    ))
                })?;
            let name = &template[name_start..end];
            if name.is_empty() {
                return Err(TaggerError::PatternComposition(
                    "Empty placeholder name '{}' in template".to_string()
                ));
            }
            let pattern = components.get(name).ok_or_else(|| {
                TaggerError::PatternComposition(format!(
                    "No component provided for placeholder '{{{}}}'",
                    name
                ))
            })?;
            // Wrap in non-capture group to isolate from surrounding syntax
            result.push_str("(?:");
            result.push_str(pattern);
            result.push(')');
            i = end + 1;
            literal_start = i;
        } else if bytes[i] == b'}' {
            // Flush preceding literal text.
            result.push_str(&template[literal_start..i]);
            // Check for escaped brace `}}`
            if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                result.push('}');
                i += 2;
                literal_start = i;
                continue;
            }
            return Err(TaggerError::PatternComposition(format!(
                "Unexpected '}}' at position {} without matching '{{'",
                i
            )));
        } else {
            i += 1;
        }
    }
    // Flush remaining literal text.
    result.push_str(&template[literal_start..]);

    // Validate the composed pattern with resharp
    resharp::Regex::new(&result)
        .map_err(|e| TaggerError::InvalidRegex(format!("Composed pattern is invalid: {}", e)))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string_list::build_string_list_pattern;

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
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[test]
    fn test_regex_pattern_empty_template() {
        let components = HashMap::new();
        let result = build_regex_pattern("", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }

    #[test]
    fn test_regex_pattern_unclosed_brace() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc{def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unclosed"));
    }

    #[test]
    fn test_regex_pattern_empty_placeholder() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc{}def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty placeholder"));
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
        assert!(result.unwrap_err().to_string().contains("Composed pattern is invalid"));
    }

    #[test]
    fn test_regex_pattern_unmatched_closing_brace() {
        let components = HashMap::new();
        let result = build_regex_pattern("abc}def", &components);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected '}'"));
    }
}
