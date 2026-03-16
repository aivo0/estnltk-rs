use std::io::Write;

use estnltk_csv::{load_rules_from_csv, ColumnRef, CsvLoadConfig};
use estnltk_core::*;
use estnltk_taggers::{RegexTagger, make_rule};

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
        unique_patterns: false,
        overlapped: false,
        match_attribute: None,
    };

    let tagger = RegexTagger::new(rules, config).unwrap();
    let result = tagger.tag("abc 123 def");

    assert_eq!(result.spans.len(), 3);

    let first_ann = &result.spans[0].annotations[0];
    assert!(
        first_ann.get("type") == Some(&AnnotationValue::Str("word".to_string()))
            || first_ann.get("type") == Some(&AnnotationValue::Str("number".to_string()))
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
        unique_patterns: false,
        overlapped: false,
        match_attribute: None,
    };

    let tagger = RegexTagger::new(rules, tagger_config).unwrap();
    let result = tagger.tag("hello");

    assert_eq!(result.spans.len(), 1);
    assert_eq!(
        result.spans[0].annotations[0].get("label"),
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
        unique_patterns: false,
        overlapped: false,
        match_attribute: None,
    };

    let tagger = RegexTagger::new(rules, config).unwrap();
    let result = tagger.tag("öötaevas on täht");

    assert_eq!(result.spans.len(), 2);
    assert_eq!(
        result.spans[0].annotations[0].get("label"),
        Some(&AnnotationValue::Str("night".to_string()))
    );
    assert_eq!(
        result.spans[1].annotations[0].get("label"),
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
    assert!(result.unwrap_err().to_string().contains("Cannot open"));
}

#[test]
fn test_csv_regex_type_column_with_tagger() {
    let csv = "pattern,filter_re,label\nstring,regex,string\n[0-9]+,\\d{3},number\n[a-z]+,[A-Z],word\n";
    let f = write_temp_csv(csv);
    let csv_rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();

    assert_eq!(csv_rules.len(), 2);
    assert_eq!(
        csv_rules[0].attributes.get("filter_re"),
        Some(&AnnotationValue::Str("\\d{3}".to_string()))
    );
    assert_eq!(
        csv_rules[1].attributes.get("filter_re"),
        Some(&AnnotationValue::Str("[A-Z]".to_string()))
    );

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
        unique_patterns: false,
        overlapped: false,
        match_attribute: None,
    };

    let tagger = RegexTagger::new(rules, config).unwrap();
    let result = tagger.tag("abc 123");

    assert_eq!(result.spans.len(), 2);
    let has_filter_re = result
        .spans
        .iter()
        .any(|s| s.annotations[0].contains_key("filter_re"));
    assert!(has_filter_re, "filter_re attribute should be present in annotations");
}

#[test]
fn test_csv_regex_type_invalid_pattern_rejected() {
    let csv = "pattern,filter_re\nstring,regex\nhello,[unclosed(\n";
    let f = write_temp_csv(csv);
    let result = load_rules_from_csv(f.path(), &CsvLoadConfig::default());
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid regex pattern"),
        "Should report invalid regex: {}",
        err
    );
}
