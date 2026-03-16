use std::collections::HashMap;

use estnltk_taggers::{SpanRule, SpanTagger, SpanTaggerConfig};
use estnltk_core::{
    Annotation, AnnotationValue, CommonConfig, ConflictStrategy, MatchSpan, TagResult, TaggedSpan,
};

fn make_input(
    spans: Vec<(usize, usize, Vec<HashMap<String, AnnotationValue>>)>,
) -> TagResult {
    TagResult {
        name: "input_layer".to_string(),
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

fn default_config() -> SpanTaggerConfig {
    SpanTaggerConfig {
        common: CommonConfig {
            output_layer: "tagged".to_string(),
            output_attributes: vec!["value".to_string()],
            ..CommonConfig::default()
        },
        input_attribute: "lemma".to_string(),
        ignore_case: false,
    }
}

#[test]
fn test_estonian_lemma_vocabulary() {
    let rules = vec![
        SpanRule::new(
            "tundma",
            HashMap::from([("value".to_string(), AnnotationValue::Str("T".to_string()))]),
            0, 1,
        ),
        SpanRule::new(
            "päike",
            HashMap::from([("value".to_string(), AnnotationValue::Str("P".to_string()))]),
            0, 2,
        ),
        SpanRule::new(
            "inimene",
            HashMap::from([("value".to_string(), AnnotationValue::Str("K".to_string()))]),
            0, 2,
        ),
    ];

    let tagger = SpanTagger::new(rules, default_config()).unwrap();

    let input = make_input(vec![
        (0, 6, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("tundma".to_string()))])]),
        (7, 12, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("päike".to_string()))])]),
        (13, 20, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("inimene".to_string()))])]),
        (21, 24, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("ja".to_string()))])]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.name, "tagged");
    assert_eq!(result.spans.len(), 3);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 6));
    assert_eq!(result.spans[0].annotations[0].get("value"), Some(&AnnotationValue::Str("T".to_string())));
    assert_eq!(result.spans[1].span, MatchSpan::new(7, 12));
    assert_eq!(result.spans[1].annotations[0].get("value"), Some(&AnnotationValue::Str("P".to_string())));
    assert_eq!(result.spans[2].span, MatchSpan::new(13, 20));
    assert_eq!(result.spans[2].annotations[0].get("value"), Some(&AnnotationValue::Str("K".to_string())));
}

#[test]
fn test_ambiguous_input_one_match() {
    let rules = vec![
        SpanRule::new(
            "pank",
            HashMap::from([("value".to_string(), AnnotationValue::Str("finance".to_string()))]),
            0, 0,
        ),
    ];

    let tagger = SpanTagger::new(rules, default_config()).unwrap();

    let input = make_input(vec![(
        0, 4,
        vec![
            HashMap::from([("lemma".to_string(), AnnotationValue::Str("pangema".to_string()))]),
            HashMap::from([("lemma".to_string(), AnnotationValue::Str("pank".to_string()))]),
        ],
    )]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].annotations.len(), 1);
    assert_eq!(result.spans[0].annotations[0].get("value"), Some(&AnnotationValue::Str("finance".to_string())));
}

#[test]
fn test_estonian_ignore_case() {
    let rules = vec![
        SpanRule::new(
            "õun",
            HashMap::from([("value".to_string(), AnnotationValue::Str("fruit".to_string()))]),
            0, 0,
        ),
    ];

    let config = SpanTaggerConfig {
        ignore_case: true,
        ..default_config()
    };
    let tagger = SpanTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("Õun".to_string()))])]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
}

#[test]
fn test_pipeline_regex_to_span() {
    let regex_output = TagResult {
        name: "tokens".to_string(),
        attributes: vec!["type".to_string(), "match".to_string()],
        ambiguous: true,
        spans: vec![
            TaggedSpan {
                span: MatchSpan::new(0, 5),
                annotations: vec![Annotation::from(HashMap::from([
                    ("type".to_string(), AnnotationValue::Str("word".to_string())),
                    ("match".to_string(), AnnotationValue::Str("tundi".to_string())),
                ]))],
            },
            TaggedSpan {
                span: MatchSpan::new(6, 12),
                annotations: vec![Annotation::from(HashMap::from([
                    ("type".to_string(), AnnotationValue::Str("email".to_string())),
                    ("match".to_string(), AnnotationValue::Str("a@b.ee".to_string())),
                ]))],
            },
        ],
    };

    let rules = vec![
        SpanRule::new(
            "word",
            HashMap::from([("category".to_string(), AnnotationValue::Str("text".to_string()))]),
            0, 0,
        ),
        SpanRule::new(
            "email",
            HashMap::from([("category".to_string(), AnnotationValue::Str("contact".to_string()))]),
            0, 0,
        ),
    ];

    let config = SpanTaggerConfig {
        common: CommonConfig {
            output_attributes: vec!["category".to_string()],
            ..CommonConfig::default()
        },
        input_attribute: "type".to_string(),
        ..default_config()
    };
    let tagger = SpanTagger::new(rules, config).unwrap();

    let result = tagger.tag(&regex_output);
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].annotations[0].get("category"), Some(&AnnotationValue::Str("text".to_string())));
    assert_eq!(result.spans[1].annotations[0].get("category"), Some(&AnnotationValue::Str("contact".to_string())));
}

#[test]
fn test_conflict_keep_maximal_overlapping() {
    let rules = vec![
        SpanRule::new("a", HashMap::from([("v".to_string(), AnnotationValue::Int(1))]), 0, 0),
        SpanRule::new("b", HashMap::from([("v".to_string(), AnnotationValue::Int(2))]), 0, 0),
    ];

    let config = SpanTaggerConfig {
        common: CommonConfig {
            conflict_strategy: ConflictStrategy::KeepMaximal,
            output_attributes: vec!["v".to_string()],
            ..CommonConfig::default()
        },
        ..default_config()
    };
    let tagger = SpanTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 10, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("a".to_string()))])]),
        (2, 5, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("b".to_string()))])]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 10));
}

#[test]
fn test_pattern_attribute_recorded() {
    let rules = vec![
        SpanRule::new(
            "koer",
            HashMap::from([("v".to_string(), AnnotationValue::Str("dog".to_string()))]),
            0, 0,
        ),
    ];

    let config = SpanTaggerConfig {
        common: CommonConfig {
            output_attributes: vec!["v".to_string()],
            pattern_attribute: Some("_pattern_".to_string()),
            ..CommonConfig::default()
        },
        ..default_config()
    };
    let tagger = SpanTagger::new(rules, config).unwrap();

    let input = make_input(vec![
        (0, 4, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("koer".to_string()))])]),
    ]);

    let result = tagger.tag(&input);
    assert_eq!(
        result.spans[0].annotations[0].get("_pattern_"),
        Some(&AnnotationValue::Str("koer".to_string()))
    );
}

#[test]
fn test_empty_input() {
    let rules = vec![SpanRule::new("x", HashMap::new(), 0, 0)];
    let tagger = SpanTagger::new(rules, default_config()).unwrap();

    let input = TagResult {
        name: "input".to_string(),
        attributes: vec!["lemma".to_string()],
        ambiguous: true,
        spans: vec![],
    };

    let result = tagger.tag(&input);
    assert_eq!(result.spans.len(), 0);
}
