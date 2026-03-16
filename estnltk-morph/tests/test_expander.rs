use std::collections::HashMap;
use std::path::Path;

use estnltk_morph::{default_expander, expand_rules, noun_forms_expander};
use estnltk_taggers::{SubstringTagger, SubstringRule};
use estnltk_core::{AnnotationValue, CommonConfig, TaggerConfig};
use vabamorf_rs::Vabamorf;

fn get_vm() -> Vabamorf {
    Vabamorf::from_dct_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../vabamorf-cpp/dct"),
    )
    .expect("Failed to load Vabamorf dicts")
}

fn default_config() -> TaggerConfig {
    TaggerConfig {
        common: CommonConfig {
            output_layer: "test".to_string(),
            ..CommonConfig::default()
        },
        lowercase_text: false,
        overlapped: false,
        match_attribute: None,
    }
}

#[test]
fn test_noun_forms_returns_28() {
    let mut vm = get_vm();
    let forms = noun_forms_expander(&mut vm, "maja").unwrap();
    assert_eq!(forms.len(), 28);
}

#[test]
fn test_known_forms_present() {
    let mut vm = get_vm();
    let forms = noun_forms_expander(&mut vm, "maja").unwrap();
    assert!(
        forms[0].contains("maja"),
        "sg n should contain 'maja', got: {}",
        forms[0]
    );
    assert!(
        forms[8].contains("majas"),
        "sg in should contain 'majas', got: {}",
        forms[8]
    );
}

#[test]
fn test_default_equals_noun_forms() {
    let mut vm = get_vm();
    let noun = noun_forms_expander(&mut vm, "kala").unwrap();
    let def = default_expander(&mut vm, "kala").unwrap();
    assert_eq!(noun, def);
}

#[test]
fn test_expand_rules_multiplies() {
    let mut vm = get_vm();
    let rules = vec![SubstringRule::new("maja", HashMap::new(), 0, 0)];
    let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();
    assert!(
        expanded.len() > 1,
        "Should expand to multiple rules, got {}",
        expanded.len()
    );
    assert!(expanded.len() <= 28);
}

#[test]
fn test_expanded_tagger_matches_forms() {
    let mut vm = get_vm();
    let mut attrs = HashMap::new();
    attrs.insert(
        "type".to_string(),
        AnnotationValue::Str("building".to_string()),
    );
    let rules = vec![SubstringRule::new("maja", attrs, 0, 0)];
    let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();

    let tagger = SubstringTagger::new(expanded, "", default_config()).unwrap();

    let result = tagger.tag("majas on soe");
    assert!(
        !result.spans.is_empty(),
        "Should match 'majas' as an expanded form of 'maja'"
    );
}

#[test]
fn test_empty_forms_filtered() {
    let mut vm = get_vm();
    let rules = vec![SubstringRule::new("maja", HashMap::new(), 0, 0)];
    let expanded = expand_rules(rules, "noun_forms", &mut vm, false).unwrap();
    for rule in &expanded {
        assert!(
            !rule.pattern_str.is_empty(),
            "Empty patterns should be filtered out"
        );
    }
}
