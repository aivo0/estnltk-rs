use crate::substring_tagger::SubstringRule;
use crate::types::TaggerError;
use vabamorf_rs::Vabamorf;

/// The 14 Estonian noun cases (abbreviation, full name).
pub const ESTONIAN_NOUN_CASES: [(&str, &str); 14] = [
    ("n", "nimetav"),
    ("g", "omastav"),
    ("p", "osastav"),
    ("ill", "sisseütlev"),
    ("in", "seesütlev"),
    ("el", "seestütlev"),
    ("all", "alaleütlev"),
    ("ad", "alalütlev"),
    ("abl", "alaltütlev"),
    ("tr", "saav"),
    ("ter", "rajav"),
    ("es", "olev"),
    ("ab", "ilmaütlev"),
    ("kom", "kaasaütlev"),
];

/// Generate all 28 noun case forms (14 cases x sg/pl) via Vabamorf synthesis.
///
/// For each case, calls `synthesize(word, "sg {case}", "S", ...)` and
/// `synthesize(word, "pl {case}", "S", ...)`, joining multiple results
/// with `", "` to match EstNLTK's `noun_forms_expander` behavior.
///
/// Returns a Vec of 28 strings. Some may be empty if synthesis produces no results.
pub fn noun_forms_expander(vm: &mut Vabamorf, word: &str) -> Result<Vec<String>, TaggerError> {
    let mut expanded = Vec::with_capacity(28);
    for &(case_abbr, _) in &ESTONIAN_NOUN_CASES {
        let sg_form = format!("sg {}", case_abbr);
        let sg_results = vm
            .synthesize(word, &sg_form, "S", "", true, false)
            .map_err(|e| TaggerError::Config(e.to_string()))?;
        expanded.push(sg_results.join(", "));

        let pl_form = format!("pl {}", case_abbr);
        let pl_results = vm
            .synthesize(word, &pl_form, "S", "", true, false)
            .map_err(|e| TaggerError::Config(e.to_string()))?;
        expanded.push(pl_results.join(", "));
    }
    Ok(expanded)
}

/// Default expander — currently delegates to `noun_forms_expander`.
///
/// Matches EstNLTK's `default_expander` which has a TODO to auto-detect
/// noun vs verb and dispatch accordingly.
pub fn default_expander(vm: &mut Vabamorf, word: &str) -> Result<Vec<String>, TaggerError> {
    noun_forms_expander(vm, word)
}

/// Expand a set of SubstringRules using the named expander.
///
/// For each rule, expands its pattern via the expander function. Each non-empty
/// expanded form becomes a new SubstringRule inheriting the original rule's
/// attributes, group, and priority. If `lowercase` is true, patterns are
/// lowercased before expansion.
pub fn expand_rules(
    rules: Vec<SubstringRule>,
    expander_name: &str,
    vm: &mut Vabamorf,
    lowercase: bool,
) -> Result<Vec<SubstringRule>, TaggerError> {
    let expander_fn = match expander_name {
        "noun_forms" => noun_forms_expander,
        "default" => default_expander,
        other => return Err(TaggerError::Config(format!(
            "Unknown expander: '{}'. Use 'noun_forms' or 'default'", other
        ))),
    };

    let mut expanded_rules = Vec::new();
    for rule in &rules {
        let pattern = if lowercase {
            rule.pattern_str.to_lowercase()
        } else {
            rule.pattern_str.clone()
        };

        let forms = expander_fn(vm, &pattern)?;
        for form in forms {
            if !form.is_empty() {
                expanded_rules.push(SubstringRule {
                    pattern_str: form,
                    attributes: rule.attributes.clone(),
                    group: rule.group,
                    priority: rule.priority,
                });
            }
        }
    }
    Ok(expanded_rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn get_vm() -> Vabamorf {
        Vabamorf::from_dct_dir(Path::new("vabamorf-cpp/dct")).expect("Failed to load Vabamorf dicts")
    }

    #[test]
    fn test_noun_forms_expander_returns_28() {
        let mut vm = get_vm();
        let forms = noun_forms_expander(&mut vm, "maja").unwrap();
        assert_eq!(forms.len(), 28, "Expected 28 forms (14 cases x sg/pl)");
    }

    #[test]
    fn test_noun_forms_known_forms() {
        let mut vm = get_vm();
        let forms = noun_forms_expander(&mut vm, "maja").unwrap();
        // sg n (nominative singular) should contain "maja"
        assert!(forms[0].contains("maja"), "sg n should contain 'maja', got: {}", forms[0]);
        // sg g (genitive singular) should contain "maja"
        assert!(forms[2].contains("maja"), "sg g should contain 'maja', got: {}", forms[2]);
        // sg in (inessive singular) should contain "majas"
        assert!(forms[8].contains("majas"), "sg in should contain 'majas', got: {}", forms[8]);
    }

    #[test]
    fn test_default_expander_delegates() {
        let mut vm = get_vm();
        let noun_forms = noun_forms_expander(&mut vm, "maja").unwrap();
        let default_forms = default_expander(&mut vm, "maja").unwrap();
        assert_eq!(noun_forms, default_forms);
    }

    #[test]
    fn test_expand_rules_multiplies() {
        let mut vm = get_vm();
        let rules = vec![SubstringRule {
            pattern_str: "maja".to_string(),
            attributes: HashMap::new(),
            group: 0,
            priority: 0,
        }];
        let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();
        // Should have multiple rules (non-empty forms from 28 slots)
        assert!(expanded.len() > 0, "Should have expanded rules");
        assert!(expanded.len() <= 28, "Should have at most 28 expanded rules");
    }

    #[test]
    fn test_expand_rules_preserves_attributes() {
        let mut vm = get_vm();
        let mut attrs = HashMap::new();
        attrs.insert(
            "type".to_string(),
            crate::types::AnnotationValue::Str("building".to_string()),
        );
        let rules = vec![SubstringRule {
            pattern_str: "maja".to_string(),
            attributes: attrs.clone(),
            group: 1,
            priority: 5,
        }];
        let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();
        for rule in &expanded {
            assert_eq!(rule.attributes, attrs);
            assert_eq!(rule.group, 1);
            assert_eq!(rule.priority, 5);
        }
    }

    #[test]
    fn test_expand_rules_unknown_expander() {
        let mut vm = get_vm();
        let rules = vec![SubstringRule {
            pattern_str: "maja".to_string(),
            attributes: HashMap::new(),
            group: 0,
            priority: 0,
        }];
        let result = expand_rules(rules, "unknown", &mut vm, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown expander"));
    }

    #[test]
    fn test_expand_rules_filters_empty() {
        let mut vm = get_vm();
        let rules = vec![SubstringRule {
            pattern_str: "maja".to_string(),
            attributes: HashMap::new(),
            group: 0,
            priority: 0,
        }];
        let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();
        for rule in &expanded {
            assert!(!rule.pattern_str.is_empty(), "Empty patterns should be filtered out");
        }
    }
}
