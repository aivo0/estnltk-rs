use std::collections::HashMap;

use estnltk_core::{Annotation, AnnotationValue, MatchSpan, TagResult, TaggedSpan};

use crate::grammar::GapValidatorFn;
use crate::graph::ParseGraph;
use crate::node::{GrammarNode, NodeKind};

/// Yield pairs of span indices that are positionally consecutive.
///
/// Two spans are consecutive if:
/// 1. The second starts at or after the first ends (`span2.start >= span1.end`)
/// 2. No other span falls entirely between them
/// 3. The gap text passes `gap_validator` (if provided)
///
/// Port of Python `iterate_consecutive_spans()` from `consecutive.py`.
pub fn iterate_consecutive_spans(
    spans: &[TaggedSpan],
    raw_text: &str,
    max_gap: usize,
    gap_validator: Option<&GapValidatorFn>,
) -> Vec<(usize, usize)> {
    let mut result = Vec::new();

    // Assume spans are already sorted by (start, end)
    for i in 0..spans.len() {
        let span_i = &spans[i].span;
        let mut checked: Vec<&MatchSpan> = Vec::new();

        for j in (i + 1)..spans.len() {
            let span_j = &spans[j].span;
            let gap = if span_j.start >= span_i.end {
                span_j.start - span_i.end
            } else {
                continue; // overlapping, skip
            };

            if gap > max_gap {
                break;
            }

            // Check if any previously checked span falls entirely between i and j
            let falls_in_gap = checked.iter().any(|cs| {
                span_i.end <= cs.start && cs.end <= span_j.start
            });

            if falls_in_gap {
                break;
            }

            // Validate gap text
            let valid = match gap_validator {
                Some(f) => {
                    let gap_text = &raw_text[span_i.end..span_j.start];
                    f(gap_text)
                }
                None => true,
            };

            if valid {
                result.push((i, j));
            }

            checked.push(span_j);
        }
    }

    result
}

/// Build a [`ParseGraph`] from a [`TagResult`] (port of `layer_to_graph()`).
///
/// Each span's `name_attribute` annotation value becomes the terminal symbol
/// name. For ambiguous layers, multiple annotations per span create multiple
/// terminal nodes.
pub fn tag_result_to_graph(
    input: &TagResult,
    raw_text: &str,
    name_attribute: &str,
    attributes: Option<&[String]>,
    gap_validator: Option<&GapValidatorFn>,
) -> ParseGraph {
    let mut graph = ParseGraph::new();

    let attr_list: Vec<&str> = match attributes {
        Some(attrs) => attrs.iter().map(|s| s.as_str()).collect(),
        None => input.attributes.iter().map(|s| s.as_str()).collect(),
    };

    // Index: span_index → Vec<NodeId> for O(1) edge construction
    let mut span_to_ids: HashMap<usize, Vec<u32>> = HashMap::new();

    for (span_idx, tagged_span) in input.spans.iter().enumerate() {
        let annotations: Vec<&Annotation> = if input.ambiguous {
            tagged_span.annotations.iter().collect()
        } else {
            tagged_span.annotations.first().into_iter().collect()
        };

        let mut names_seen = std::collections::HashSet::new();
        for annotation in annotations {
            let name = match annotation.get(name_attribute) {
                Some(AnnotationValue::Str(s)) => s.clone(),
                _ => continue,
            };
            if !names_seen.insert(name.clone()) {
                continue;
            }

            let node = make_terminal_node(
                span_idx, tagged_span, &name, annotation, &attr_list, raw_text,
            );

            if let Some(id) = graph.add_node(node) {
                graph.node_mut(id).terminals = vec![id];
                span_to_ids.entry(span_idx).or_default().push(id);
            }
        }
    }

    // Add sequence edges between consecutive spans
    let consecutive_pairs =
        iterate_consecutive_spans(&input.spans, raw_text, usize::MAX, gap_validator);

    for (i, j) in consecutive_pairs {
        let empty = Vec::new();
        let ids_i = span_to_ids.get(&i).unwrap_or(&empty);
        let ids_j = span_to_ids.get(&j).unwrap_or(&empty);
        for &id_a in ids_i {
            for &id_b in ids_j {
                graph.add_seq_edge(id_a, id_b);
            }
        }
    }

    graph
}

fn make_terminal_node(
    span_idx: usize,
    tagged_span: &TaggedSpan,
    name: &str,
    annotation: &Annotation,
    attr_list: &[&str],
    raw_text: &str,
) -> GrammarNode {
    let mut node_attrs = HashMap::new();
    for &attr in attr_list {
        if let Some(val) = annotation.get(attr) {
            node_attrs.insert(attr.to_string(), val.clone());
        }
    }
    GrammarNode {
        id: 0,
        name: name.to_string(),
        start: tagged_span.span.start,
        end: tagged_span.span.end,
        kind: NodeKind::Terminal { span_index: span_idx },
        support: vec![],
        terminals: vec![],
        group: 0,
        priority: 0,
        score: 0.0,
        attributes: node_attrs,
        text: Some(raw_text[tagged_span.span.start..tagged_span.span.end].to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estnltk_core::{Annotation, MatchSpan, TagResult, TaggedSpan};

    fn make_tag_result(
        spans: Vec<(usize, usize, &str)>,
    ) -> TagResult {
        let tagged_spans: Vec<TaggedSpan> = spans
            .iter()
            .map(|(start, end, name)| {
                let mut ann = Annotation::new();
                ann.insert("grammar_symbol".to_string(), AnnotationValue::Str(name.to_string()));
                TaggedSpan {
                    span: MatchSpan::new(*start, *end),
                    annotations: vec![ann],
                }
            })
            .collect();

        TagResult {
            name: "test_layer".to_string(),
            attributes: vec!["grammar_symbol".to_string()],
            ambiguous: false,
            spans: tagged_spans,
        }
    }

    #[test]
    fn test_consecutive_basic() {
        let result = make_tag_result(vec![
            (0, 4, "A"),
            (4, 5, "B"),
            (6, 12, "C"),
            (12, 13, "D"),
        ]);
        let pairs = iterate_consecutive_spans(&result.spans, "Tere, maailm!", usize::MAX, None);
        // A→B (adjacent), B→C (gap ","), C→D (adjacent)
        // A→C is not consecutive because B falls between them
        assert_eq!(pairs, vec![(0, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn test_tag_result_to_graph() {
        let raw = "Tere, maailm!";
        let result = make_tag_result(vec![
            (0, 4, "A"),
            (4, 5, "B"),
            (6, 12, "C"),
            (12, 13, "D"),
        ]);
        let graph = tag_result_to_graph(&result, raw, "grammar_symbol", None, None);
        assert_eq!(graph.len(), 4);

        // Check edges: A→B, B→C, C→D
        let edges = graph.seq_edges_sorted();
        let edge_names: Vec<(&str, &str)> = edges
            .iter()
            .map(|(a, b)| {
                (
                    graph.node(*a).name.as_str(),
                    graph.node(*b).name.as_str(),
                )
            })
            .collect();
        assert_eq!(edge_names, vec![("A", "B"), ("B", "C"), ("C", "D")]);
    }

    #[test]
    fn test_ambiguous_layer() {
        // One span with two grammar symbols
        let mut ann1 = Annotation::new();
        ann1.insert("gs".to_string(), AnnotationValue::Str("X".to_string()));
        let mut ann2 = Annotation::new();
        ann2.insert("gs".to_string(), AnnotationValue::Str("Y".to_string()));

        let result = TagResult {
            name: "test".to_string(),
            attributes: vec!["gs".to_string()],
            ambiguous: true,
            spans: vec![
                TaggedSpan {
                    span: MatchSpan::new(0, 3),
                    annotations: vec![ann1, ann2],
                },
            ],
        };

        let graph = tag_result_to_graph(&result, "abc", "gs", None, None);
        assert_eq!(graph.len(), 2); // X and Y nodes for same span
    }
}
