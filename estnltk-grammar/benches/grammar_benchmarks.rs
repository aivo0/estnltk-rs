//! Criterion benchmarks for the grammar tagger.
//!
//! Measures throughput across varying input sizes, rule counts,
//! grammar depth, and the full `grammar_tag` pipeline.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use estnltk_core::{Annotation, AnnotationValue, MatchSpan, TagResult, TaggedSpan};
use estnltk_grammar::{
    DepthLimit, Grammar, GrammarBuilder, GrammarTagConfig, ParseConfig, Rule,
    grammar_tag, parse_graph, tag_result_to_graph,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a TagResult simulating pre-tagged address components.
/// Cycles through terminal symbol names to create `n_spans` spans.
fn make_address_input(n_spans: usize) -> (TagResult, String) {
    let symbols = ["TÄNAV", "MAJA", "ASULA", "MAAKOND", "INDEKS"];
    let words = ["Veski", "5", "Elva", "Tartumaa", "51003"];

    let mut raw = String::new();
    let mut spans = Vec::with_capacity(n_spans);
    let mut offset = 0;

    for i in 0..n_spans {
        let sym = symbols[i % symbols.len()];
        let word = words[i % words.len()];
        let end = offset + word.len();

        let mut ann = Annotation::new();
        ann.insert(
            "grammar_symbol".to_string(),
            AnnotationValue::Str(sym.to_string()),
        );
        spans.push(TaggedSpan {
            span: MatchSpan::new(offset, end),
            annotations: vec![ann],
        });

        raw.push_str(word);
        raw.push(' ');
        offset = end + 1;
    }

    let result = TagResult {
        name: "address_parts".to_string(),
        attributes: vec!["grammar_symbol".to_string()],
        ambiguous: false,
        spans,
    };

    (result, raw)
}

/// Build an address grammar with `n_rules` rules of increasing length.
fn build_address_grammar(n_rules: usize) -> Grammar {
    let symbols = ["TÄNAV", "MAJA", "ASULA", "MAAKOND", "INDEKS"];
    let mut builder = GrammarBuilder::new()
        .start_symbols(vec!["ADDRESS"])
        .depth_limit(DepthLimit::Finite(6))
        .legal_attributes(HashSet::new());

    for i in 0..n_rules {
        let rhs_len = (i % 4) + 2; // 2..5 symbols per rule
        let rhs: Vec<&str> = (0..rhs_len).map(|j| symbols[(i + j) % symbols.len()]).collect();
        let rhs_str = rhs.join(" ");
        builder.add_rule(
            Rule::new("ADDRESS", rhs_str.as_str())
                .unwrap()
                .with_priority(i as i32),
        );
    }

    builder.build().unwrap()
}

/// Build a deeper grammar for depth-scaling benchmarks.
/// Creates a chain: S -> A B, A -> C D, C -> E F, ... down to terminals.
fn build_chain_grammar(depth: u32) -> (Grammar, Vec<String>) {
    let mut builder = GrammarBuilder::new()
        .depth_limit(DepthLimit::Finite(depth + 1));

    let mut terminals = Vec::new();
    let mut current_lhs = "S".to_string();

    for d in 0..depth {
        let left = format!("N{}L", d);
        let right = format!("N{}R", d);
        builder.add_rule(
            Rule::new(current_lhs.as_str(), format!("{} {}", left, right).as_str()).unwrap(),
        );
        if d + 1 == depth {
            terminals.push(left);
            terminals.push(right);
        } else {
            current_lhs = left;
            // right becomes terminal
            terminals.push(right);
        }
    }

    builder = builder.start_symbols(vec!["S"]);
    let grammar = builder.build().unwrap();
    (grammar, terminals)
}

/// Build a TagResult whose symbols match the terminals from `build_chain_grammar`.
fn make_chain_input(terminals: &[String]) -> (TagResult, String) {
    let mut raw = String::new();
    let mut spans = Vec::with_capacity(terminals.len());
    let mut offset = 0;

    for sym in terminals {
        let word = "tok"; // generic token
        let end = offset + word.len();

        let mut ann = Annotation::new();
        ann.insert(
            "grammar_symbol".to_string(),
            AnnotationValue::Str(sym.clone()),
        );
        spans.push(TaggedSpan {
            span: MatchSpan::new(offset, end),
            annotations: vec![ann],
        });

        raw.push_str(word);
        raw.push(' ');
        offset = end + 1;
    }

    let result = TagResult {
        name: "chain_input".to_string(),
        attributes: vec!["grammar_symbol".to_string()],
        ambiguous: false,
        spans,
    };

    (result, raw)
}

// ---------------------------------------------------------------------------
// Benchmarks: tag_result_to_graph (graph construction)
// ---------------------------------------------------------------------------

fn bench_graph_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/graph_construction");

    for &n_spans in &[20, 100, 500, 2000] {
        let (input, raw_text) = make_address_input(n_spans);
        let label = format!("{}_spans", n_spans);

        group.bench_with_input(BenchmarkId::new("build", &label), &(), |b, _| {
            b.iter(|| {
                let graph = tag_result_to_graph(
                    black_box(&input),
                    black_box(&raw_text),
                    "grammar_symbol",
                    None,
                    None,
                );
                black_box(&graph);
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: parse_graph (parsing only)
// ---------------------------------------------------------------------------

fn bench_parse_input_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/parse_input_size");
    let grammar = build_address_grammar(4);
    let config = ParseConfig::default();

    for &n_spans in &[20, 100, 500] {
        let (input, raw_text) = make_address_input(n_spans);
        let label = format!("{}_spans", n_spans);

        group.bench_with_input(BenchmarkId::new("parse", &label), &(), |b, _| {
            b.iter(|| {
                let mut graph = tag_result_to_graph(&input, &raw_text, "grammar_symbol", None, None);
                parse_graph(black_box(&mut graph), black_box(&grammar), &config).unwrap();
                black_box(&graph);
            });
        });
    }
    group.finish();
}

fn bench_parse_rule_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/parse_rule_count");
    let config = ParseConfig::default();
    let (input, raw_text) = make_address_input(100);

    for &n_rules in &[2, 4, 8, 12] {
        let grammar = build_address_grammar(n_rules);
        let label = format!("{}_rules", n_rules);

        group.bench_with_input(BenchmarkId::new("parse", &label), &(), |b, _| {
            b.iter(|| {
                let mut graph = tag_result_to_graph(&input, &raw_text, "grammar_symbol", None, None);
                parse_graph(black_box(&mut graph), black_box(&grammar), &config).unwrap();
                black_box(&graph);
            });
        });
    }
    group.finish();
}

fn bench_parse_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/parse_depth");
    let config = ParseConfig::default();

    for &depth in &[2, 4, 8] {
        let (grammar, terminals) = build_chain_grammar(depth);
        let (input, raw_text) = make_chain_input(&terminals);
        let label = format!("depth_{}", depth);

        group.bench_with_input(BenchmarkId::new("parse", &label), &(), |b, _| {
            b.iter(|| {
                let mut graph = tag_result_to_graph(&input, &raw_text, "grammar_symbol", None, None);
                parse_graph(black_box(&mut graph), black_box(&grammar), &config).unwrap();
                black_box(&graph);
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: conflict resolution strategies
// ---------------------------------------------------------------------------

fn bench_parse_conflict_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/conflict_resolution");
    let grammar = build_address_grammar(4);
    let (input, raw_text) = make_address_input(200);

    let configs = [
        ("all_enabled", ParseConfig::default()),
        (
            "no_support",
            ParseConfig {
                resolve_support_conflicts: false,
                ..Default::default()
            },
        ),
        (
            "none",
            ParseConfig {
                resolve_support_conflicts: false,
                resolve_start_end_conflicts: false,
                resolve_terminals_conflicts: false,
                ignore_validators: false,
            },
        ),
    ];

    for (name, config) in &configs {
        group.bench_with_input(BenchmarkId::new("parse", *name), &(), |b, _| {
            b.iter(|| {
                let mut graph = tag_result_to_graph(&input, &raw_text, "grammar_symbol", None, None);
                parse_graph(black_box(&mut graph), black_box(&grammar), config).unwrap();
                black_box(&graph);
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: full grammar_tag pipeline
// ---------------------------------------------------------------------------

fn bench_grammar_tag_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/full_pipeline");
    let grammar = build_address_grammar(4);

    for &n_spans in &[20, 100, 500] {
        let (input, raw_text) = make_address_input(n_spans);
        let label = format!("{}_spans", n_spans);

        let config = GrammarTagConfig {
            name_attribute: "grammar_symbol".to_string(),
            output_layer: "addresses".to_string(),
            output_attributes: vec![],
            output_nodes: Some(HashSet::from(["ADDRESS".into()])),
            ambiguous: false,
            force_resolving_by_priority: false,
            ..Default::default()
        };

        group.bench_with_input(BenchmarkId::new("tag", &label), &(), |b, _| {
            b.iter(|| {
                let result = grammar_tag(
                    black_box(&input),
                    black_box(&raw_text),
                    black_box(&grammar),
                    &config,
                );
                black_box(&result);
            });
        });
    }
    group.finish();
}

fn bench_grammar_tag_with_priority(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/priority_resolution");
    let grammar = build_address_grammar(4);
    let (input, raw_text) = make_address_input(200);

    for &force_priority in &[false, true] {
        let config = GrammarTagConfig {
            name_attribute: "grammar_symbol".to_string(),
            output_layer: "addresses".to_string(),
            output_attributes: vec![],
            output_nodes: Some(HashSet::from(["ADDRESS".into()])),
            ambiguous: true,
            force_resolving_by_priority: force_priority,
            ..Default::default()
        };
        let label = if force_priority {
            "force_priority=true"
        } else {
            "force_priority=false"
        };

        group.bench_with_input(BenchmarkId::new("tag", label), &(), |b, _| {
            b.iter(|| {
                let result = grammar_tag(
                    black_box(&input),
                    black_box(&raw_text),
                    black_box(&grammar),
                    &config,
                );
                black_box(&result);
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: decorator overhead
// ---------------------------------------------------------------------------

fn bench_grammar_tag_decorator(c: &mut Criterion) {
    let mut group = c.benchmark_group("grammar/decorator_overhead");
    let (input, raw_text) = make_address_input(200);

    // Grammar without decorators
    let grammar_plain = build_address_grammar(4);

    // Grammar with decorators
    let mut builder = GrammarBuilder::new()
        .start_symbols(vec!["ADDRESS"])
        .depth_limit(DepthLimit::Finite(6))
        .legal_attributes(HashSet::from([
            "grammar_symbol".into(),
            "first".into(),
            "last".into(),
        ]));

    let decorator: estnltk_grammar::DecoratorFn =
        Arc::new(|nodes: &[&estnltk_grammar::GrammarNode]| {
            let mut attrs = HashMap::new();
            attrs.insert(
                "grammar_symbol".to_string(),
                AnnotationValue::Str("ADDRESS".to_string()),
            );
            if let Some(first) = nodes.first() {
                attrs.insert(
                    "first".to_string(),
                    AnnotationValue::Str(first.text.clone().unwrap_or_default()),
                );
            }
            if let Some(last) = nodes.last() {
                attrs.insert(
                    "last".to_string(),
                    AnnotationValue::Str(last.text.clone().unwrap_or_default()),
                );
            }
            attrs
        });

    let symbols = ["TÄNAV", "MAJA", "ASULA", "MAAKOND", "INDEKS"];
    for i in 0..4 {
        let rhs_len = (i % 4) + 2;
        let rhs: Vec<&str> = (0..rhs_len).map(|j| symbols[(i + j) % symbols.len()]).collect();
        let rhs_str = rhs.join(" ");
        builder.add_rule(
            Rule::new("ADDRESS", rhs_str.as_str())
                .unwrap()
                .with_priority(i as i32)
                .with_decorator(decorator.clone()),
        );
    }
    let grammar_decorated = builder.build().unwrap();

    let config = GrammarTagConfig {
        name_attribute: "grammar_symbol".to_string(),
        output_layer: "addresses".to_string(),
        output_attributes: vec!["grammar_symbol".into(), "first".into(), "last".into()],
        output_nodes: Some(HashSet::from(["ADDRESS".into()])),
        ambiguous: false,
        force_resolving_by_priority: false,
        ..Default::default()
    };

    let config_plain = GrammarTagConfig {
        output_attributes: vec![],
        ..Default::default()
    };

    group.bench_with_input(BenchmarkId::new("tag", "no_decorator"), &(), |b, _| {
        b.iter(|| {
            let result = grammar_tag(
                black_box(&input),
                black_box(&raw_text),
                black_box(&grammar_plain),
                &config_plain,
            );
            black_box(&result);
        });
    });

    group.bench_with_input(BenchmarkId::new("tag", "with_decorator"), &(), |b, _| {
        b.iter(|| {
            let result = grammar_tag(
                black_box(&input),
                black_box(&raw_text),
                black_box(&grammar_decorated),
                &config,
            );
            black_box(&result);
        });
    });

    group.finish();
}

criterion_group!(
    grammar_bench_group,
    bench_graph_construction,
    bench_parse_input_size,
    bench_parse_rule_count,
    bench_parse_depth,
    bench_parse_conflict_resolution,
    bench_grammar_tag_pipeline,
    bench_grammar_tag_with_priority,
    bench_grammar_tag_decorator,
);

criterion_main!(grammar_bench_group);
