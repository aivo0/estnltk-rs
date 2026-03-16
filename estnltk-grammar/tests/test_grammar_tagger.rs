use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use estnltk_core::{Annotation, AnnotationValue, MatchSpan, TagResult, TaggedSpan};
use estnltk_grammar::{
    DepthLimit, GrammarBuilder, GrammarTagConfig, Rule, grammar_tag,
};

/// Build a simple address-parts input layer.
fn make_address_input() -> (TagResult, String) {
    let raw_text = "Veski 5, Elva, Tartumaa".to_string();

    let make_span = |start: usize, end: usize, symbol: &str| {
        let mut ann = Annotation::new();
        ann.insert(
            "grammar_symbol".to_string(),
            AnnotationValue::Str(symbol.to_string()),
        );
        TaggedSpan {
            span: MatchSpan::new(start, end),
            annotations: vec![ann],
        }
    };

    let result = TagResult {
        name: "address_parts".to_string(),
        attributes: vec!["grammar_symbol".to_string()],
        ambiguous: false,
        spans: vec![
            make_span(0, 5, "TÄNAV"),  // "Veski"
            make_span(6, 7, "MAJA"),    // "5"
            make_span(9, 13, "ASULA"), // "Elva"
            make_span(15, 23, "MAAKOND"), // "Tartumaa"
        ],
    };

    (result, raw_text)
}

#[test]
fn test_grammar_tag_address() {
    let (input, raw_text) = make_address_input();

    // Build address grammar
    let address_decorator: estnltk_grammar::DecoratorFn =
        Arc::new(|nodes: &[&estnltk_grammar::GrammarNode]| {
            let mut attrs = HashMap::new();
            let mut asula = String::new();
            let mut tanav = String::new();
            let mut maja = String::new();
            let mut maakond = String::new();

            for node in nodes {
                match node.name.as_str() {
                    "ASULA" => asula = node.text.clone().unwrap_or_default(),
                    "TÄNAV" => tanav = node.text.clone().unwrap_or_default(),
                    "MAJA" => maja = node.text.clone().unwrap_or_default(),
                    "MAAKOND" => maakond = node.text.clone().unwrap_or_default(),
                    _ => {}
                }
            }

            attrs.insert(
                "grammar_symbol".to_string(),
                AnnotationValue::Str("ADDRESS".to_string()),
            );
            attrs.insert("ASULA".to_string(), AnnotationValue::Str(asula));
            attrs.insert("TÄNAV".to_string(), AnnotationValue::Str(tanav));
            attrs.insert("MAJA".to_string(), AnnotationValue::Str(maja));
            attrs.insert("MAAKOND".to_string(), AnnotationValue::Str(maakond));
            attrs
        });

    let mut builder = GrammarBuilder::new()
        .start_symbols(vec!["ADDRESS"])
        .depth_limit(DepthLimit::Finite(4))
        .legal_attributes(HashSet::from([
            "grammar_symbol".into(),
            "ASULA".into(),
            "TÄNAV".into(),
            "MAJA".into(),
            "MAAKOND".into(),
            "INDEKS".into(),
        ]));

    builder.add_rule(
        Rule::new("ADDRESS", "TÄNAV MAJA")
            .unwrap()
            .with_priority(5)
            .with_decorator(address_decorator.clone()),
    );
    builder.add_rule(
        Rule::new("ADDRESS", "TÄNAV MAJA ASULA")
            .unwrap()
            .with_priority(3)
            .with_decorator(address_decorator.clone()),
    );
    builder.add_rule(
        Rule::new("ADDRESS", "TÄNAV MAJA ASULA MAAKOND")
            .unwrap()
            .with_priority(1)
            .with_decorator(address_decorator.clone()),
    );

    let grammar = builder.build().unwrap();

    let config = GrammarTagConfig {
        name_attribute: "grammar_symbol".to_string(),
        output_layer: "addresses".to_string(),
        output_attributes: vec![
            "grammar_symbol".into(),
            "TÄNAV".into(),
            "MAJA".into(),
            "ASULA".into(),
            "MAAKOND".into(),
        ],
        output_nodes: Some(HashSet::from(["ADDRESS".into()])),
        ambiguous: true,
        force_resolving_by_priority: false,
        ..Default::default()
    };

    let result = grammar_tag(&input, &raw_text, &grammar, &config).unwrap();

    assert_eq!(result.name, "addresses");
    // Should find addresses — at least the full one
    assert!(!result.spans.is_empty());

    // Check the most complete address match
    // Find the ADDRESS with MAAKOND="Tartumaa"
    let full_address = result.spans.iter().find(|s| {
        s.annotations.iter().any(|ann| {
            ann.get("MAAKOND") == Some(&AnnotationValue::Str("Tartumaa".to_string()))
        })
    });
    assert!(full_address.is_some(), "Expected full address with MAAKOND");

    let full = full_address.unwrap();
    let ann = &full.annotations[0];
    assert_eq!(
        ann.get("TÄNAV"),
        Some(&AnnotationValue::Str("Veski".to_string()))
    );
    assert_eq!(
        ann.get("MAJA"),
        Some(&AnnotationValue::Str("5".to_string()))
    );
    assert_eq!(
        ann.get("ASULA"),
        Some(&AnnotationValue::Str("Elva".to_string()))
    );
}

#[test]
fn test_grammar_tag_force_priority() {
    let (input, raw_text) = make_address_input();

    let mut builder = GrammarBuilder::new()
        .start_symbols(vec!["ADDRESS"])
        .depth_limit(DepthLimit::Finite(4))
        .legal_attributes(HashSet::new());

    builder.add_rule(Rule::new("ADDRESS", "TÄNAV MAJA").unwrap().with_priority(5));
    builder.add_rule(
        Rule::new("ADDRESS", "TÄNAV MAJA ASULA")
            .unwrap()
            .with_priority(3),
    );
    builder.add_rule(
        Rule::new("ADDRESS", "TÄNAV MAJA ASULA MAAKOND")
            .unwrap()
            .with_priority(1),
    );

    let grammar = builder.build().unwrap();

    let config = GrammarTagConfig {
        name_attribute: "grammar_symbol".to_string(),
        output_layer: "addresses".to_string(),
        output_attributes: vec![],
        output_nodes: Some(HashSet::from(["ADDRESS".into()])),
        ambiguous: false,
        force_resolving_by_priority: true,
        ..Default::default()
    };

    let result = grammar_tag(&input, &raw_text, &grammar, &config).unwrap();

    // With force_resolving_by_priority, only the highest priority (lowest value)
    // should survive: the full address (priority=1)
    assert_eq!(result.spans.len(), 1);
    let span = &result.spans[0];
    // The full address covers all 4 terminal spans
    assert_eq!(span.spans.len(), 4);
}

#[test]
fn test_grammar_tag_with_validator() {
    let raw_text = "Tere, maailm!".to_string();

    let make_span = |start: usize, end: usize, symbol: &str| {
        let mut ann = Annotation::new();
        ann.insert(
            "gs".to_string(),
            AnnotationValue::Str(symbol.to_string()),
        );
        TaggedSpan {
            span: MatchSpan::new(start, end),
            annotations: vec![ann],
        }
    };

    let input = TagResult {
        name: "test".to_string(),
        attributes: vec!["gs".to_string()],
        ambiguous: false,
        spans: vec![
            make_span(0, 4, "A"),
            make_span(4, 5, "B"),
            make_span(6, 12, "C"),
            make_span(12, 13, "D"),
        ],
    };

    // Validator that rejects matches where first support starts at 0
    let validator: estnltk_grammar::ValidatorFn =
        Arc::new(|nodes: &[&estnltk_grammar::GrammarNode]| {
            nodes.first().map_or(true, |n| n.start != 0)
        });

    let mut builder = GrammarBuilder::new().start_symbols(vec!["E"]);
    builder.add_rule(
        Rule::new("E", "A B")
            .unwrap()
            .with_validator(validator.clone()),
    );
    builder.add_rule(Rule::new("E", "C D").unwrap().with_validator(validator));
    let grammar = builder.build().unwrap();

    let config = GrammarTagConfig {
        name_attribute: "gs".to_string(),
        output_layer: "result".to_string(),
        output_attributes: vec![],
        ambiguous: false,
        ..Default::default()
    };

    let result = grammar_tag(&input, &raw_text, &grammar, &config).unwrap();

    // "A B" starts at 0 → rejected by validator
    // "C D" starts at 6 → accepted
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].bounding_span.start, 6);
}
