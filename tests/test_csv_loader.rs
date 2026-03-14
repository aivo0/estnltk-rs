use std::io::Write;

use estnltk_regex_rs::csv_loader::{load_rules_from_csv, ColumnRef, CsvLoadConfig};
use estnltk_regex_rs::tagger::{make_rule, RegexTagger};
use estnltk_regex_rs::types::*;

fn write_temp_csv(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn test_csv_to_regex_tagger_integration() {
    // Load rules from CSV, then use them with RegexTagger
    let csv = "pattern,type\nstring,string\n[0-9]+,number\n[a-z]+,word\n";
    let f = write_temp_csv(csv);
    let csv_rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();

    // Convert CsvRules to ExtractionRules
    let mut rules = Vec::new();
    let mut all_attrs = Vec::new();
    for cr in &csv_rules {
        let rule = make_rule(&cr.pattern, cr.attributes.clone(), cr.group, cr.priority).unwrap();
        for k in rule.attributes.keys() {
            if !all_attrs.contains(k) {
                all_attrs.push(k.clone());
            }
        }
        rules.push(rule);
    }

    let config = TaggerConfig {
        output_layer: "regexes".to_string(),
        output_attributes: all_attrs,
        conflict_strategy: ConflictStrategy::KeepAll,
        lowercase_text: false,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
    };

    let tagger = RegexTagger::new(rules, config).unwrap();
    let result = tagger.tag("abc 123 def");

    // "abc" matches [a-z]+ and "123" matches [0-9]+, "def" matches [a-z]+
    // But resharp is leftmost-longest, so [a-z]+ matches "abc" and [0-9]+ matches "123",
    // and [a-z]+ matches "def"
    assert_eq!(result.spans.len(), 3);

    // Check that type attribute is carried through
    let first_ann = &result.spans[0].annotations[0];
    assert!(
        first_ann.0.get("type") == Some(&AnnotationValue::Str("word".to_string()))
            || first_ann.0.get("type") == Some(&AnnotationValue::Str("number".to_string()))
    );
}

#[test]
fn test_csv_with_priority_resolution() {
    let csv = "pattern,priority,label\nstring,int,string\n[a-z]+,0,high\n[a-z]+,1,low\n";
    let f = write_temp_csv(csv);
    let config = CsvLoadConfig {
        key_column: ColumnRef::Index(0),
        group_column: None,
        priority_column: Some(ColumnRef::Name("priority".to_string())),
    };
    let csv_rules = load_rules_from_csv(f.path(), &config).unwrap();

    assert_eq!(csv_rules.len(), 2);
    assert_eq!(csv_rules[0].priority, 0);
    assert_eq!(csv_rules[1].priority, 1);

    // Build tagger with priority-based conflict resolution
    let mut rules = Vec::new();
    let mut all_attrs = Vec::new();
    for cr in &csv_rules {
        let rule = make_rule(&cr.pattern, cr.attributes.clone(), cr.group, cr.priority).unwrap();
        for k in rule.attributes.keys() {
            if !all_attrs.contains(k) {
                all_attrs.push(k.clone());
            }
        }
        rules.push(rule);
    }

    let tagger_config = TaggerConfig {
        output_layer: "regexes".to_string(),
        output_attributes: all_attrs,
        conflict_strategy: ConflictStrategy::KeepAllExceptPriority,
        lowercase_text: false,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
    };

    let tagger = RegexTagger::new(rules, tagger_config).unwrap();
    let result = tagger.tag("hello");

    // Both rules match "hello". Priority resolver should keep only priority=0 ("high").
    assert_eq!(result.spans.len(), 1);
    assert_eq!(
        result.spans[0].annotations[0].0.get("label"),
        Some(&AnnotationValue::Str("high".to_string()))
    );
}

#[test]
fn test_csv_estonian_patterns() {
    let csv = "pattern,label\nstring,string\nöö,night\ntäht,star\n";
    let f = write_temp_csv(csv);
    let csv_rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();

    let mut rules = Vec::new();
    for cr in &csv_rules {
        rules.push(make_rule(&cr.pattern, cr.attributes.clone(), cr.group, cr.priority).unwrap());
    }

    let config = TaggerConfig {
        output_layer: "regexes".to_string(),
        output_attributes: vec!["label".to_string()],
        conflict_strategy: ConflictStrategy::KeepAll,
        lowercase_text: false,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
    };

    let tagger = RegexTagger::new(rules, config).unwrap();
    let result = tagger.tag("öötaevas on täht");

    assert_eq!(result.spans.len(), 2);
    assert_eq!(
        result.spans[0].annotations[0].0.get("label"),
        Some(&AnnotationValue::Str("night".to_string()))
    );
    assert_eq!(
        result.spans[1].annotations[0].0.get("label"),
        Some(&AnnotationValue::Str("star".to_string()))
    );
}

#[test]
fn test_csv_multiple_attribute_types() {
    let csv =
        "pattern,count,weight,active,category\nstring,int,float,bool,string\nfoo,10,2.5,true,A\nbar,20,3.7,false,B\n";
    let f = write_temp_csv(csv);
    let csv_rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();

    assert_eq!(csv_rules.len(), 2);

    let r0 = &csv_rules[0];
    assert_eq!(r0.pattern, "foo");
    assert_eq!(r0.attributes.get("count"), Some(&AnnotationValue::Int(10)));
    assert_eq!(
        r0.attributes.get("weight"),
        Some(&AnnotationValue::Float(2.5))
    );
    assert_eq!(
        r0.attributes.get("active"),
        Some(&AnnotationValue::Bool(true))
    );
    assert_eq!(
        r0.attributes.get("category"),
        Some(&AnnotationValue::Str("A".to_string()))
    );

    let r1 = &csv_rules[1];
    assert_eq!(r1.pattern, "bar");
    assert_eq!(
        r1.attributes.get("active"),
        Some(&AnnotationValue::Bool(false))
    );
}

#[test]
fn test_csv_nonexistent_file() {
    let result = load_rules_from_csv("/tmp/nonexistent_file_12345.csv", &CsvLoadConfig::default());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot open"));
}
