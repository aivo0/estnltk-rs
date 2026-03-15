use std::collections::HashMap;

use estnltk_regex_rs::phrase_tagger::{make_phrase_rule, PhraseTagger, PhraseTaggerConfig};
use estnltk_regex_rs::types::{
    Annotation, AnnotationValue, ConflictStrategy, MatchSpan, TagResult, TaggedSpan,
};

fn make_input(
    spans: Vec<(usize, usize, Vec<HashMap<String, AnnotationValue>>)>,
) -> TagResult {
    TagResult {
        name: "morph_analysis".to_string(),
        attributes: vec!["lemma".to_string()],
        ambiguous: true,
        spans: spans
            .into_iter()
            .map(|(start, end, anns)| TaggedSpan {
                span: MatchSpan::new(start, end),
                annotations: anns.into_iter().map(Annotation::from).collect(),
            })
            .collect(),
    }
}

fn ann(lemma: &str) -> HashMap<String, AnnotationValue> {
    HashMap::from([("lemma".to_string(), AnnotationValue::Str(lemma.to_string()))])
}

fn default_config() -> PhraseTaggerConfig {
    PhraseTaggerConfig {
        output_layer: "phrases".to_string(),
        input_attribute: "lemma".to_string(),
        output_attributes: vec!["value".to_string()],
        conflict_strategy: ConflictStrategy::KeepAll,
        ignore_case: false,
        phrase_attribute: Some("phrase".to_string()),
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
        unique_patterns: false,
    }
}

/// Simulate the "Varsti tuleb Euroopa Liidust lahkumine" scenario.
///
/// Input layer (morph_analysis):
///   0-6:  "Varsti"   lemma="varsti"
///   7-12: "tuleb"    lemma="tulema"
///   13-20: "Euroopa" lemma="euroopa"
///   21-28: "Liidust" lemma="liit" | lemma="liidu" (ambiguous)
///   29-38: "lahkumine" lemma="lahkumine"
///
/// Rules:
///   ("euroopa", "liit") → value="ORG"
///   ("euroopa", "liidu") → value="ORG2"
#[test]
fn test_euroopa_liit_scenario() {
    let rules = vec![
        make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            0,
            0,
        ),
        make_phrase_rule(
            vec!["euroopa".to_string(), "liidu".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG2".to_string()))]),
            0,
            0,
        ),
    ];
    let tagger = PhraseTagger::new(rules, default_config()).unwrap();

    let input = make_input(vec![
        (0, 6, vec![ann("varsti")]),
        (7, 12, vec![ann("tulema")]),
        (13, 20, vec![ann("euroopa")]),
        (21, 28, vec![ann("liit"), ann("liidu")]),
        (29, 38, vec![ann("lahkumine")]),
    ]);

    let result = tagger.tag(&input);

    assert_eq!(result.name, "phrases");
    assert!(result.ambiguous);

    // Both phrases match because "Liidust" has both lemmas.
    // They share the same bounding span (13, 28), so they should be merged.
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].bounding_span, MatchSpan::new(13, 28));
    assert_eq!(result.spans[0].spans.len(), 2);
    assert_eq!(result.spans[0].spans[0], MatchSpan::new(13, 20));
    assert_eq!(result.spans[0].spans[1], MatchSpan::new(21, 28));

    // Two annotations on the same enveloping span.
    assert_eq!(result.spans[0].annotations.len(), 2);

    // Check phrase attributes.
    let phrase0 = result.spans[0].annotations[0].get("phrase").unwrap();
    let phrase1 = result.spans[0].annotations[1].get("phrase").unwrap();

    // One should have ("euroopa", "liit") and the other ("euroopa", "liidu").
    let expected_phrases = vec![
        AnnotationValue::List(vec![
            AnnotationValue::Str("euroopa".to_string()),
            AnnotationValue::Str("liit".to_string()),
        ]),
        AnnotationValue::List(vec![
            AnnotationValue::Str("euroopa".to_string()),
            AnnotationValue::Str("liidu".to_string()),
        ]),
    ];
    assert!(
        expected_phrases.contains(phrase0) && expected_phrases.contains(phrase1),
        "Expected both phrase variants to match"
    );
}

/// Test with KEEP_MAXIMAL on overlapping enveloping spans.
#[test]
fn test_keep_maximal_overlapping_phrases() {
    let rules = vec![
        make_phrase_rule(
            vec!["uus".to_string(), "york".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
            0,
            0,
        ),
        make_phrase_rule(
            vec!["uus".to_string(), "york".to_string(), "linn".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("LOC_EXT".to_string()))]),
            0,
            0,
        ),
    ];
    let config = PhraseTaggerConfig {
        conflict_strategy: ConflictStrategy::KeepMaximal,
        ..default_config()
    };
    let tagger = PhraseTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 3, vec![ann("uus")]),
        (4, 8, vec![ann("york")]),
        (9, 13, vec![ann("linn")]),
    ]);

    let result = tagger.tag(&input);
    // "uus york linn" (0..13) contains "uus york" (0..8), so maximal keeps only the 3-word.
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 13));
    assert_eq!(result.spans[0].spans.len(), 3);
}

/// Test case-insensitive matching with Estonian characters.
#[test]
fn test_ignore_case_estonian() {
    let rules = vec![make_phrase_rule(
        vec!["põhja".to_string(), "täht".to_string()],
        HashMap::from([("value".to_string(), AnnotationValue::Str("STAR".to_string()))]),
        0,
        0,
    )];
    let config = PhraseTaggerConfig {
        ignore_case: true,
        ..default_config()
    };
    let tagger = PhraseTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 5, vec![ann("Põhja")]),
        (6, 10, vec![ann("TÄHT")]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
}

/// Test empty input produces empty output.
#[test]
fn test_empty_input() {
    let rules = vec![make_phrase_rule(
        vec!["a".to_string()],
        HashMap::new(),
        0,
        0,
    )];
    let tagger = PhraseTagger::new(rules, default_config()).unwrap();

    let input = TagResult {
        name: "input".to_string(),
        attributes: vec!["lemma".to_string()],
        ambiguous: true,
        spans: vec![],
    };

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 0);
}

/// Test single-word phrase matching.
#[test]
fn test_single_word_phrase() {
    let rules = vec![make_phrase_rule(
        vec!["eesti".to_string()],
        HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
        0,
        0,
    )];
    let tagger = PhraseTagger::new(rules, default_config()).unwrap();

    let input = make_input(vec![
        (0, 5, vec![ann("eesti")]),
        (6, 10, vec![ann("keel")]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].spans.len(), 1);
    assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 5));
}

/// Test with non-ambiguous output layer — only first annotation kept.
#[test]
fn test_non_ambiguous_output() {
    let rules = vec![
        make_phrase_rule(
            vec!["a".to_string(), "b".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
            0,
            0,
        ),
        make_phrase_rule(
            vec!["a".to_string(), "b".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Int(2))]),
            0,
            1,
        ),
    ];
    let config = PhraseTaggerConfig {
        ambiguous_output_layer: false,
        ..default_config()
    };
    let tagger = PhraseTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 1, vec![ann("a")]),
        (2, 3, vec![ann("b")]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].annotations.len(), 1);
    assert!(!result.ambiguous);
}

/// Test that output has correct attributes list.
#[test]
fn test_output_attributes() {
    let rules = vec![make_phrase_rule(
        vec!["a".to_string()],
        HashMap::from([
            ("type".to_string(), AnnotationValue::Str("test".to_string())),
            ("score".to_string(), AnnotationValue::Int(42)),
        ]),
        0,
        0,
    )];
    let config = PhraseTaggerConfig {
        output_attributes: vec!["type".to_string(), "score".to_string()],
        phrase_attribute: None,
        ..default_config()
    };
    let tagger = PhraseTagger::new(rules, config).unwrap();

    let input = make_input(vec![(0, 1, vec![ann("a")])]);

    let result = tagger.tag(&input);
    assert_eq!(result.attributes, vec!["type".to_string(), "score".to_string()]);
}

/// Test multiple non-overlapping phrases in a single input.
#[test]
fn test_multiple_non_overlapping_phrases() {
    let rules = vec![
        make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            0,
            0,
        ),
        make_phrase_rule(
            vec!["eesti".to_string(), "vabariik".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
            0,
            0,
        ),
    ];
    let tagger = PhraseTagger::new(rules, default_config()).unwrap();

    let input = make_input(vec![
        (0, 5, vec![ann("eesti")]),
        (6, 14, vec![ann("vabariik")]),
        (15, 17, vec![ann("ja")]),
        (18, 25, vec![ann("euroopa")]),
        (26, 30, vec![ann("liit")]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 14));
    assert_eq!(
        result.spans[0].annotations[0].get("value"),
        Some(&AnnotationValue::Str("LOC".to_string()))
    );
    assert_eq!(result.spans[1].bounding_span, MatchSpan::new(18, 30));
    assert_eq!(
        result.spans[1].annotations[0].get("value"),
        Some(&AnnotationValue::Str("ORG".to_string()))
    );
}
