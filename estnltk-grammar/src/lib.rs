pub mod consecutive;
pub mod grammar;
pub mod graph;
pub mod node;
pub mod operations;
pub mod parsing;

use std::collections::HashSet;

use estnltk_core::{
    Annotation, AnnotationValue, EnvelopingTaggedSpan, MatchSpan, PhraseTagResult, TagResult,
};

pub use consecutive::{iterate_consecutive_spans, tag_result_to_graph};
pub use grammar::{
    DecoratorFn, DepthLimit, GapValidatorFn, Grammar, GrammarBuilder, GrammarError, Rule,
    RhsSpec, ScoringFn, SyntheticRule, ValidatorFn, WidthLimit, match_mseq_pattern,
    match_seq_pattern,
};
pub use graph::ParseGraph;
pub use node::{GrammarNode, NodeId, NodeKey, NodeKind, compute_group_hash};
pub use operations::{ngram_fingerprint, phrase_list_generator};
pub use parsing::{ParseConfig, parse_graph};

/// Configuration for the top-level [`grammar_tag`] function.
pub struct GrammarTagConfig {
    /// Which annotation attribute to read as the terminal symbol name.
    pub name_attribute: String,
    /// Name of the output layer.
    pub output_layer: String,
    /// Which attributes to include in output annotations.
    pub output_attributes: Vec<String>,
    /// Which nonterminal names to output. `None` = grammar's start_symbols.
    pub output_nodes: Option<HashSet<String>>,
    /// Gap validator for consecutive span detection.
    pub gap_validator: Option<GapValidatorFn>,
    /// Parsing conflict resolution config.
    pub parse_config: ParseConfig,
    /// Whether the output layer allows ambiguous annotations.
    pub ambiguous: bool,
    /// If true, resolve remaining conflicts by priority after parsing.
    pub force_resolving_by_priority: bool,
    /// Attribute name for priority-based conflict resolution.
    pub priority_attribute: String,
}

impl Default for GrammarTagConfig {
    fn default() -> Self {
        Self {
            name_attribute: "grammar_symbol".to_string(),
            output_layer: "parse".to_string(),
            output_attributes: Vec::new(),
            output_nodes: None,
            gap_validator: None,
            parse_config: ParseConfig::default(),
            ambiguous: false,
            force_resolving_by_priority: false,
            priority_attribute: "_priority".to_string(),
        }
    }
}

/// Top-level grammar tagging function.
///
/// Port of `GrammarParsingTagger._make_layer()`. Runs the full pipeline:
/// 1. Convert input `TagResult` to a `ParseGraph`
/// 2. Parse the graph using the grammar
/// 3. Collect output nodes and build a `PhraseTagResult`
/// 4. Optionally resolve remaining conflicts by priority
pub fn grammar_tag(
    input: &TagResult,
    raw_text: &str,
    grammar: &Grammar,
    config: &GrammarTagConfig,
) -> Result<PhraseTagResult, GrammarError> {
    // 1. Build the parse graph from input layer
    let mut graph = tag_result_to_graph(
        input,
        raw_text,
        &config.name_attribute,
        None,
        config.gap_validator.as_ref(),
    );

    // 2. Parse
    parse_graph(&mut graph, grammar, &config.parse_config)?;

    // 3. Collect output
    let output_nodes: HashSet<&str> = match &config.output_nodes {
        Some(nodes) => nodes.iter().map(|s| s.as_str()).collect(),
        None => grammar.start_symbols().iter().map(|s| s.as_str()).collect(),
    };

    // Determine effective output attributes
    let mut effective_attrs = config.output_attributes.clone();
    if config.force_resolving_by_priority
        && !effective_attrs.contains(&config.priority_attribute)
    {
        effective_attrs.push(config.priority_attribute.clone());
    }

    // Collect matching nodes sorted by position
    let mut output_entries: Vec<(Vec<MatchSpan>, Annotation)> = Vec::new();

    for (_, gnode) in graph.alive_nodes_sorted() {
        if !output_nodes.contains(gnode.name.as_str()) {
            continue;
        }

        let mut annotation = Annotation::new();
        for attr in &effective_attrs {
            if attr == "_group_" {
                annotation.insert(
                    "_group_".to_string(),
                    AnnotationValue::Int(gnode.group),
                );
            } else if attr == "name" {
                annotation.insert(
                    "name".to_string(),
                    AnnotationValue::Str(gnode.name.clone()),
                );
            } else if attr == "_priority_" {
                annotation.insert(
                    "_priority_".to_string(),
                    AnnotationValue::Int(gnode.priority as i64),
                );
            } else if attr == &config.priority_attribute && config.force_resolving_by_priority {
                annotation.insert(
                    config.priority_attribute.clone(),
                    AnnotationValue::Int(gnode.priority as i64),
                );
            } else if let Some(val) = gnode.attributes.get(attr) {
                annotation.insert(attr.clone(), val.clone());
            } else {
                annotation.insert(attr.clone(), AnnotationValue::Null);
            }
        }

        // Get the terminal spans (the enveloped input spans)
        let spans: Vec<MatchSpan> = gnode
            .terminals
            .iter()
            .map(|&tid| {
                let t = graph.node(tid);
                MatchSpan::new(t.start, t.end)
            })
            .collect();

        output_entries.push((spans, annotation));
    }

    // 4. Build PhraseTagResult
    let ambiguous = config.ambiguous || config.force_resolving_by_priority;

    let mut result_spans: Vec<EnvelopingTaggedSpan> = Vec::new();
    for (spans, annotation) in output_entries {
        let bounding = MatchSpan::new(
            spans.first().map_or(0, |s| s.start),
            spans.last().map_or(0, |s| s.end),
        );

        // Check if we can merge with the last span (same enveloping span)
        if let Some(last) = result_spans.last_mut() {
            if last.spans == spans {
                if ambiguous {
                    last.annotations.push(annotation);
                }
                continue;
            }
        }

        result_spans.push(EnvelopingTaggedSpan {
            spans,
            bounding_span: bounding,
            annotations: vec![annotation],
        });
    }

    // 5. Resolve conflicts by priority if requested
    if config.force_resolving_by_priority {
        resolve_by_priority(&mut result_spans, &config.priority_attribute);

        // Remove priority attribute if it wasn't in the original output_attributes
        if !config.output_attributes.contains(&config.priority_attribute) {
            for span in &mut result_spans {
                for ann in &mut span.annotations {
                    ann.remove(&config.priority_attribute);
                }
            }
        }
    }

    let final_attrs = config.output_attributes.clone();

    Ok(PhraseTagResult {
        name: config.output_layer.clone(),
        attributes: final_attrs,
        ambiguous: config.ambiguous,
        spans: result_spans,
    })
}

/// Resolve overlapping enveloping spans by priority.
///
/// For spans that overlap (share any terminal positions), keep only the
/// annotation(s) with the lowest priority value (highest priority).
fn resolve_by_priority(spans: &mut Vec<EnvelopingTaggedSpan>, priority_attr: &str) {
    // Simple O(n²) approach: for each pair of overlapping spans,
    // remove the one with higher priority value.
    let mut to_remove: HashSet<usize> = HashSet::new();

    for i in 0..spans.len() {
        if to_remove.contains(&i) {
            continue;
        }
        for j in (i + 1)..spans.len() {
            if to_remove.contains(&j) {
                continue;
            }
            // Check if spans overlap (bounding spans intersect)
            let a = &spans[i].bounding_span;
            let b = &spans[j].bounding_span;
            if a.overlaps(b) {
                // Get priorities
                let pri_i = get_min_priority(&spans[i], priority_attr);
                let pri_j = get_min_priority(&spans[j], priority_attr);
                if pri_i < pri_j {
                    to_remove.insert(j);
                } else if pri_j < pri_i {
                    to_remove.insert(i);
                    break; // i is removed, no need to check more
                }
            }
        }
    }

    // Remove marked spans in reverse order
    let mut indices: Vec<usize> = to_remove.into_iter().collect();
    indices.sort_unstable();
    for &idx in indices.iter().rev() {
        spans.remove(idx);
    }
}

fn get_min_priority(span: &EnvelopingTaggedSpan, priority_attr: &str) -> i64 {
    span.annotations
        .iter()
        .filter_map(|ann| match ann.get(priority_attr) {
            Some(AnnotationValue::Int(p)) => Some(*p),
            _ => None,
        })
        .min()
        .unwrap_or(i64::MAX)
}
