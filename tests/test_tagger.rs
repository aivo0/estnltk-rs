use estnltk_regex_rs::tagger::{make_rule, RegexTagger};
use estnltk_regex_rs::types::*;
use std::collections::HashMap;

fn default_config() -> TaggerConfig {
    TaggerConfig {
        output_layer: "regexes".to_string(),
        output_attributes: vec![],
        conflict_strategy: ConflictStrategy::KeepAll,
        lowercase_text: false,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
        unique_patterns: false,
    }
}

#[test]
fn test_email_pattern() {
    // Matches email in "Aadressilt bla@bla.ee tuli"
    let pattern = r"[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+";
    let rule = make_rule(pattern, HashMap::new(), 0, 0).unwrap();
    let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
    let result = tagger.tag("Aadressilt bla@bla.ee tuli");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(11, 21));
}

#[test]
fn test_number_pattern() {
    // resharp-compatible number pattern (no capture groups, no lazy quantifiers)
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
    // Verify byte→char conversion with Estonian chars
    let text = "Tüüpiline näide öökülma kohta";
    let pattern = "öökülma";
    let rule = make_rule(pattern, HashMap::new(), 0, 0).unwrap();
    let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
    let result = tagger.tag(text);
    assert_eq!(result.spans.len(), 1);
    // Count chars: T(0) ü(1) ü(2) p(3) i(4) l(5) i(6) n(7) e(8) ' '(9)
    //              n(10) ä(11) i(12) d(13) e(14) ' '(15)
    //              ö(16) ö(17) k(18) ü(19) l(20) m(21) a(22) ' '(23) k(24) o(25) h(26) t(27) a(28)
    assert_eq!(result.spans[0].span, MatchSpan::new(16, 23));
}

#[test]
fn test_conflict_strategies_on_overlapping() {
    let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
    let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
    let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

    // KEEP_ALL
    let mut cfg = default_config();
    cfg.lowercase_text = true;
    cfg.conflict_strategy = ConflictStrategy::KeepAll;
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
    cfg.conflict_strategy = ConflictStrategy::KeepMaximal;
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
    cfg.conflict_strategy = ConflictStrategy::KeepMinimal;
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
    // Two overlapping patterns with different priorities.
    // Priority 0 (higher precedence) should survive, priority 1 removed.
    let mut attrs1 = HashMap::new();
    attrs1.insert("label".to_string(), AnnotationValue::Str("high".to_string()));
    let r1 = make_rule("[a-z]+", attrs1, 0, 0).unwrap();

    let mut attrs2 = HashMap::new();
    attrs2.insert("label".to_string(), AnnotationValue::Str("low".to_string()));
    let r2 = make_rule("[a-z]+", attrs2, 0, 1).unwrap();

    let mut cfg = default_config();
    cfg.output_attributes = vec!["label".to_string()];
    cfg.conflict_strategy = ConflictStrategy::KeepAllExceptPriority;

    let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
    let result = tagger.tag("hello");

    // Both patterns match "hello" at (0,5). Same group=0, priority 1 > priority 0,
    // so priority=1 is removed. Only one span with label "high" remains.
    assert_eq!(result.spans.len(), 1);
    assert_eq!(
        result.spans[0].annotations[0].0.get("label"),
        Some(&AnnotationValue::Str("high".to_string()))
    );
}

#[test]
fn test_pattern_attribute() {
    let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
    let mut cfg = default_config();
    cfg.pattern_attribute = Some("_pattern_".to_string());
    cfg.output_attributes = vec!["_pattern_".to_string()];

    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("say hello");

    assert_eq!(
        result.spans[0].annotations[0].0.get("_pattern_"),
        Some(&AnnotationValue::Str("hello".to_string()))
    );
}

// ── Capture group integration tests ──────────────────────────────────

#[test]
fn test_capture_group_email_domain() {
    // Extract just the domain from an email pattern.
    // Pattern: (\w+)@(\w+\.\w+)  — group 2 = domain
    let mut attrs = HashMap::new();
    attrs.insert(
        "type".to_string(),
        AnnotationValue::Str("domain".to_string()),
    );
    let rule = make_rule(r"([a-zA-Z0-9_.+-]+)@([a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+)", attrs, 2, 0).unwrap();
    let mut cfg = default_config();
    cfg.output_attributes = vec!["type".to_string()];
    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("Kirjuta aadressile info@example.com kohe");
    assert_eq!(result.spans.len(), 1);
    // "info@example.com" starts at char 19
    // group 2 = "example.com" = chars 24..35
    assert_eq!(result.spans[0].span, MatchSpan::new(24, 35));
    assert_eq!(
        result.spans[0].annotations[0].0.get("type"),
        Some(&AnnotationValue::Str("domain".to_string()))
    );
}

#[test]
fn test_capture_group_estonian_name_extraction() {
    // Extract Estonian names after honorific.
    // "Proua Kärner ja härra Põldmäe tulid"
    let rule = make_rule(r"([Pp]roua|[Hh]ärra)\s+(\w+)", HashMap::new(), 2, 0).unwrap();
    let mut cfg = default_config();
    cfg.conflict_strategy = ConflictStrategy::KeepAll;
    let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
    let result = tagger.tag("Proua Kärner ja härra Põldmäe tulid");

    assert_eq!(result.spans.len(), 2);
    // "Kärner" and "Põldmäe" — verify both names are extracted
    // P(0) r(1) o(2) u(3) a(4) ' '(5) K(6) ä(7) r(8) n(9) e(10) r(11)
    // ' '(12) j(13) a(14) ' '(15) h(16) ä(17) r(18) r(19) a(20) ' '(21)
    // P(22) õ(23) l(24) d(25) m(26) ä(27) e(28) ' '(29) ...
    assert_eq!(result.spans[0].span, MatchSpan::new(6, 12)); // "Kärner"
    assert_eq!(result.spans[1].span, MatchSpan::new(22, 29)); // "Põldmäe"
}
