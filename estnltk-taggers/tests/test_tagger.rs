use estnltk_taggers::{make_rule, RegexTagger};
use estnltk_core::*;
use std::collections::HashMap;

fn default_config() -> TaggerConfig {
    TaggerConfig {
        common: CommonConfig {
            output_layer: "regexes".to_string(),
            output_attributes: vec![],
            conflict_strategy: ConflictStrategy::KeepAll,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
            ambiguous_output_layer: true,
            unique_patterns: false,
        },
        lowercase_text: false,
        overlapped: false,
        match_attribute: None,
    }
}

#[test]
fn test_email_pattern() {
    let pattern = r"[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+";
    let rule = make_rule(pattern, HashMap::new(), 0, 0).unwrap();
    let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
    let result = tagger.tag("Aadressilt bla@bla.ee tuli");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(11, 21));
}

#[test]
fn test_number_pattern() {
    let pattern = r"-?[0-9]+";
    let rule = make_rule(pattern, HashMap::new(), 0, 0).unwrap();
    let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
    let result = tagger.tag("abc 123 def -45 ghi");
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].span, MatchSpan::new(4, 7));
    assert_eq!(result.spans[1].span, MatchSpan::new(12, 15));
}

#[test]
fn test_estonian_multibyte_offsets() {
    let text = "Tüüpiline näide öökülma kohta";
    let pattern = "öökülma";
    let rule = make_rule(pattern, HashMap::new(), 0, 0).unwrap();
    let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
    let result = tagger.tag(text);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(16, 23));
}

#[test]
fn test_conflict_strategies_on_overlapping() {
    // KEEP_ALL
    let mut cfg = default_config();
    cfg.lowercase_text = true;
    cfg.common.conflict_strategy = ConflictStrategy::KeepAll;
    let tagger = RegexTagger::new(
        vec![
            make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap(),
        ],
        cfg,
    )
    .unwrap();
    let result = tagger.tag("Muna ja kana.");
    assert_eq!(result.spans.len(), 3);

    // KEEP_MAXIMAL
    let mut cfg = default_config();
    cfg.lowercase_text = true;
    cfg.common.conflict_strategy = ConflictStrategy::KeepMaximal;
    let tagger = RegexTagger::new(
        vec![
            make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap(),
        ],
        cfg,
    )
    .unwrap();
    let result = tagger.tag("Muna ja kana.");
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 7));
    assert_eq!(result.spans[1].span, MatchSpan::new(5, 12));

    // KEEP_MINIMAL
    let mut cfg = default_config();
    cfg.lowercase_text = true;
    cfg.common.conflict_strategy = ConflictStrategy::KeepMinimal;
    let tagger = RegexTagger::new(
        vec![
            make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja", HashMap::new(), 0, 0).unwrap(),
            make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap(),
        ],
        cfg,
    )
    .unwrap();
    let result = tagger.tag("Muna ja kana.");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(5, 7));
}

#[test]
fn test_priority_based_resolution() {
    let mut attrs1 = HashMap::new();
    attrs1.insert("label".to_string(), AnnotationValue::Str("high".to_string()));
    let r1 = make_rule("[a-z]+", attrs1, 0, 0).unwrap();

    let mut attrs2 = HashMap::new();
    attrs2.insert("label".to_string(), AnnotationValue::Str("low".to_string()));
    let r2 = make_rule("[a-z]+", attrs2, 0, 1).unwrap();

    let mut cfg = default_config();
    cfg.common.output_attributes = vec!["label".to_string()];
    cfg.common.conflict_strategy = ConflictStrategy::KeepAllExceptPriority;

    let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
    let result = tagger.tag("hello");

    assert_eq!(result.spans.len(), 1);
    assert_eq!(
        result.spans[0].annotations[0].get("label"),
        Some(&AnnotationValue::Str("high".to_string()))
    );
}

#[test]
fn test_pattern_attribute() {
    let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
    let mut cfg = default_config();
    cfg.common.pattern_attribute = Some("_pattern_".to_string());
    cfg.common.output_attributes = vec!["_pattern_".to_string()];

    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("say hello");

    assert_eq!(
        result.spans[0].annotations[0].get("_pattern_"),
        Some(&AnnotationValue::Str("hello".to_string()))
    );
}

#[test]
fn test_capture_group_email_domain() {
    let mut attrs = HashMap::new();
    attrs.insert(
        "type".to_string(),
        AnnotationValue::Str("domain".to_string()),
    );
    let rule = make_rule(r"([a-zA-Z0-9_.+-]+)@([a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+)", attrs, 2, 0).unwrap();
    let mut cfg = default_config();
    cfg.common.output_attributes = vec!["type".to_string()];
    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("Kirjuta aadressile info@example.com kohe");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(24, 35));
    assert_eq!(
        result.spans[0].annotations[0].get("type"),
        Some(&AnnotationValue::Str("domain".to_string()))
    );
}

#[test]
fn test_capture_group_estonian_name_extraction() {
    let rule = make_rule(r"([Pp]roua|[Hh]ärra)\s+(\w+)", HashMap::new(), 2, 0).unwrap();
    let mut cfg = default_config();
    cfg.common.conflict_strategy = ConflictStrategy::KeepAll;
    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("Proua Kärner ja härra Põldmäe tulid");

    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].span, MatchSpan::new(6, 12));
    assert_eq!(result.spans[1].span, MatchSpan::new(22, 29));
}
