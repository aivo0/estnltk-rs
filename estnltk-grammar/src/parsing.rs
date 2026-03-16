use std::collections::HashSet;

use crate::grammar::{Grammar, GrammarError, Rule, SyntheticRule, WidthLimit};
use crate::graph::ParseGraph;
use crate::node::{GrammarNode, NodeId, NodeKind, compute_group_hash};

/// Configuration for [`parse_graph`] conflict resolution.
#[derive(Debug, Clone)]
pub struct ParseConfig {
    pub resolve_support_conflicts: bool,
    pub resolve_start_end_conflicts: bool,
    pub resolve_terminals_conflicts: bool,
    pub ignore_validators: bool,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            resolve_support_conflicts: true,
            resolve_start_end_conflicts: true,
            resolve_terminals_conflicts: true,
            ignore_validators: false,
        }
    }
}

// ---------------------------------------------------------------------------
// get_match: find all consecutive-node sequences matching a rule RHS
// ---------------------------------------------------------------------------

/// Collect all backward paths from `node_id` to position 0 in `names`.
fn get_match_up(
    graph: &ParseGraph,
    path: &mut Vec<NodeId>,
    names: &[String],
    pos: usize,
    results: &mut Vec<Vec<NodeId>>,
) {
    if pos == 0 {
        results.push(path.clone());
        return;
    }
    let last = *path.last().unwrap();
    let preds: Vec<NodeId> = graph.seq_pred(last).collect();
    for pred_id in preds {
        if graph.node(pred_id).name == names[pos - 1] {
            path.push(pred_id);
            get_match_up(graph, path, names, pos - 1, results);
            path.pop();
        }
    }
}

/// Collect all forward paths from `node_id` to the last position in `names`.
fn get_match_down(
    graph: &ParseGraph,
    path: &mut Vec<NodeId>,
    names: &[String],
    pos: usize,
    results: &mut Vec<Vec<NodeId>>,
) {
    if pos + 1 == names.len() {
        results.push(path.clone());
        return;
    }
    let last = *path.last().unwrap();
    let succs: Vec<NodeId> = graph.seq_succ(last).collect();
    for succ_id in succs {
        if graph.node(succ_id).name == names[pos + 1] {
            path.push(succ_id);
            get_match_down(graph, path, names, pos + 1, results);
            path.pop();
        }
    }
}

/// Find all consecutive-node sequences in `graph` where:
/// - The sequence has length `names.len()`
/// - `node_id` is at position `pos`
/// - Each node's name matches the corresponding element in `names`
fn get_match(
    graph: &ParseGraph,
    node_id: NodeId,
    names: &[String],
    pos: usize,
) -> Vec<Vec<NodeId>> {
    let mut up_results = Vec::new();
    let mut up_path = vec![node_id];
    get_match_up(graph, &mut up_path, names, pos, &mut up_results);

    let mut down_results = Vec::new();
    let mut down_path = vec![node_id];
    get_match_down(graph, &mut down_path, names, pos, &mut down_results);

    // Cartesian product: up[1..].rev() ++ down
    let mut matches = Vec::with_capacity(up_results.len() * down_results.len());
    for up in &up_results {
        for down in &down_results {
            let combined: Vec<NodeId> = up[1..]
                .iter()
                .rev()
                .chain(down.iter())
                .copied()
                .collect();
            matches.push(combined);
        }
    }
    matches
}

// ---------------------------------------------------------------------------
// Node construction helpers
// ---------------------------------------------------------------------------

/// Collect support_refs from a support slice.
fn support_refs<'a>(support: &[NodeId], graph: &'a ParseGraph) -> Vec<&'a GrammarNode> {
    support.iter().map(|&id| graph.node(id)).collect()
}

/// Build a NonTerminalNode from a regular rule and support.
fn make_nonterminal(
    rule: &Rule,
    support: &[NodeId],
    graph: &ParseGraph,
) -> GrammarNode {
    let terminals: Vec<NodeId> = support
        .iter()
        .flat_map(|&id| graph.node(id).terminals.iter().copied())
        .collect();

    let start = graph.node(*terminals.first().unwrap()).start;
    let end = graph.node(*terminals.last().unwrap()).end;

    let refs = support_refs(support, graph);
    let score = rule.score(&refs);

    GrammarNode {
        id: 0,
        name: rule.lhs.clone(),
        start,
        end,
        kind: NodeKind::NonTerminal,
        support: support.to_vec(),
        terminals,
        group: rule.group,
        priority: rule.priority,
        score,
        attributes: Default::default(),
        text: None,
    }
}

/// Build a Plus or MSeq node from a synthetic rule and support.
/// Flattens nested nodes of the same `flatten_kind`.
fn make_seq_variant_node(
    rule: &SyntheticRule,
    support: &[NodeId],
    graph: &ParseGraph,
    flatten_kind: &NodeKind,
    output_kind: NodeKind,
) -> GrammarNode {
    let mut flat_support = Vec::new();
    for &id in support {
        let node = graph.node(id);
        if &node.kind == flatten_kind {
            flat_support.extend_from_slice(&node.support);
        } else {
            flat_support.push(id);
        }
    }

    let terminals: Vec<NodeId> = flat_support
        .iter()
        .flat_map(|&id| graph.node(id).terminals.iter().copied())
        .collect();

    let start = graph.node(*terminals.first().unwrap()).start;
    let end = graph.node(*terminals.last().unwrap()).end;
    let group = compute_group_hash(&rule.lhs, &flat_support);

    GrammarNode {
        id: 0,
        name: rule.lhs.clone(),
        start,
        end,
        kind: output_kind,
        support: flat_support,
        terminals,
        group,
        priority: rule.priority,
        score: 0.0,
        attributes: Default::default(),
        text: None,
    }
}

// ---------------------------------------------------------------------------
// try_add_node: conflict resolution + insertion
// ---------------------------------------------------------------------------

/// Try to add a node with conflict resolution. Returns `Some(NodeId)` if added.
fn try_add_node(
    graph: &mut ParseGraph,
    node: GrammarNode,
    config: &ParseConfig,
) -> Option<NodeId> {
    let key = node.identity_key();
    if graph.contains_key(&key) {
        return None;
    }

    let mut nodes_to_remove: Vec<NodeId> = Vec::new();

    // Support conflicts: only for NonTerminal (not Plus or MSeq)
    if matches!(node.kind, NodeKind::NonTerminal) {
        if config.resolve_support_conflicts {
            let mut fellows: HashSet<NodeId> = HashSet::new();
            for &s in &node.support {
                for parent in graph.tree_parents(s) {
                    fellows.insert(parent);
                }
            }
            for fellow_id in fellows {
                let fellow = graph.node(fellow_id);
                if node.group == fellow.group {
                    if fellow.priority < node.priority {
                        return None;
                    } else if node.priority < fellow.priority {
                        nodes_to_remove.push(fellow_id);
                    }
                }
            }
        }

        if config.resolve_start_end_conflicts || config.resolve_terminals_conflicts {
            let at_pos: Vec<NodeId> = graph.nodes_at(node.start, node.end).collect();
            for n_id in at_pos {
                let n = graph.node(n_id);
                if n.name == node.name {
                    let terminal_match = if config.resolve_terminals_conflicts {
                        n.terminals == node.terminals
                    } else {
                        false
                    };
                    if config.resolve_start_end_conflicts || terminal_match {
                        if n.score > node.score {
                            return None;
                        } else if n.score < node.score {
                            nodes_to_remove.push(n_id);
                        }
                    }
                }
            }
        }
    }

    // MSeq-specific conflicts: sorted-slice subset comparison (avoids HashSet)
    if matches!(node.kind, NodeKind::MSeq) {
        let mut supp_sorted: Vec<NodeId> = node.support.clone();
        supp_sorted.sort_unstable();

        let mut fellows: HashSet<NodeId> = HashSet::new();
        for &s in &node.support {
            for parent in graph.tree_parents(s) {
                fellows.insert(parent);
            }
        }
        for fellow_id in fellows {
            let fellow = graph.node(fellow_id);
            if matches!(fellow.kind, NodeKind::MSeq) {
                let mut fellow_sorted: Vec<NodeId> = fellow.support.clone();
                fellow_sorted.sort_unstable();
                if is_strict_subset(&fellow_sorted, &supp_sorted) {
                    nodes_to_remove.push(fellow_id);
                } else if is_subset(&supp_sorted, &fellow_sorted) {
                    return None;
                }
            }
        }
    }

    if !nodes_to_remove.is_empty() {
        graph.remove_with_ancestors(&nodes_to_remove);
    }

    let node_support_first = node.support.first().copied();
    let node_support_last = node.support.last().copied();
    let id = graph.add_node(node)?;

    if let Some(first_supp) = node_support_first {
        let preds: Vec<NodeId> = graph.seq_pred(first_supp).collect();
        for pred in preds {
            graph.add_seq_edge(pred, id);
        }
    }
    if let Some(last_supp) = node_support_last {
        let succs: Vec<NodeId> = graph.seq_succ(last_supp).collect();
        for succ in succs {
            graph.add_seq_edge(id, succ);
        }
    }

    Some(id)
}

/// Check if `a` is a subset of `b` (both sorted).
fn is_subset(a: &[NodeId], b: &[NodeId]) -> bool {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if a[i] > b[j] {
            j += 1;
        } else {
            return false;
        }
    }
    i == a.len()
}

/// Check if `a` is a strict subset of `b` (both sorted).
fn is_strict_subset(a: &[NodeId], b: &[NodeId]) -> bool {
    a.len() < b.len() && is_subset(a, b)
}

// ---------------------------------------------------------------------------
// expand_fragment: expand a node against a rule map
// ---------------------------------------------------------------------------

/// Expand with regular grammar rules → NonTerminalNode candidates.
fn expand_regular(
    node_id: NodeId,
    graph: &ParseGraph,
    grammar: &Grammar,
    node_name: &str,
    width_limit: WidthLimit,
    ignore_validators: bool,
) -> Result<Vec<GrammarNode>, GrammarError> {
    let entries = match grammar.rule_map().get(node_name) {
        Some(e) => e.as_slice(),
        None => return Ok(Vec::new()),
    };

    let mut candidates = Vec::new();
    for &(rule_idx, pos) in entries {
        let rule = &grammar.rules()[rule_idx];
        for support in get_match(graph, node_id, &rule.rhs, pos) {
            debug_assert!(support.windows(2).all(|w| graph.has_seq_edge(w[0], w[1])));

            let refs = support_refs(&support, graph);

            if !ignore_validators && !rule.validate(&refs) {
                continue;
            }

            let mut new_node = make_nonterminal(rule, &support, graph);

            if width_limit.exceeds(new_node.terminals.len()) {
                continue;
            }

            let attrs = rule.decorate(&refs);

            if !grammar.legal_attributes().is_empty() {
                let illegal: Vec<String> = attrs
                    .keys()
                    .filter(|k| !grammar.legal_attributes().contains(*k))
                    .cloned()
                    .collect();
                if !illegal.is_empty() {
                    return Err(GrammarError::IllegalAttributes(illegal));
                }
            }

            new_node.attributes = attrs;
            candidates.push(new_node);
        }
    }
    Ok(candidates)
}

/// Expand with synthetic (SEQ or MSEQ) rules.
fn expand_synthetic(
    node_id: NodeId,
    graph: &ParseGraph,
    node_name: &str,
    rules: &[SyntheticRule],
    rule_map: &std::collections::HashMap<String, Vec<(usize, usize)>>,
    width_limit: WidthLimit,
    flatten_kind: &NodeKind,
    output_kind: NodeKind,
) -> Vec<GrammarNode> {
    let entries = match rule_map.get(node_name) {
        Some(e) => e.as_slice(),
        None => return Vec::new(),
    };

    let mut candidates = Vec::new();
    for &(rule_idx, pos) in entries {
        let rule = &rules[rule_idx];
        for support in get_match(graph, node_id, &rule.rhs, pos) {
            let new_node =
                make_seq_variant_node(rule, &support, graph, flatten_kind, output_kind.clone());
            if !width_limit.exceeds(new_node.terminals.len()) {
                candidates.push(new_node);
            }
        }
    }
    candidates
}

// ---------------------------------------------------------------------------
// parse_graph: main entry point
// ---------------------------------------------------------------------------

/// Run bottom-up chart parsing on a [`ParseGraph`] using a [`Grammar`].
///
/// Modifies the graph in place by adding NonTerminal, Plus, and MSeq nodes
/// according to the grammar rules. Conflict resolution strategies are
/// controlled by [`ParseConfig`].
///
/// Port of Python `parse_graph()` from `parsing.py`.
pub fn parse_graph(
    graph: &mut ParseGraph,
    grammar: &Grammar,
    config: &ParseConfig,
) -> Result<(), GrammarError> {
    let width_limit = grammar.width_limit();

    let depth_val = match grammar.depth_limit() {
        crate::grammar::DepthLimit::Finite(n) => n as usize,
        crate::grammar::DepthLimit::Unlimited => usize::MAX,
    };

    if depth_val == 0 {
        return Ok(());
    }

    // Collect all symbol names that appear in any rule map
    let mut names_to_parse: HashSet<&str> = HashSet::new();
    for key in grammar.rule_map().keys() {
        names_to_parse.insert(key);
    }
    for key in grammar.hidden_rule_map().keys() {
        names_to_parse.insert(key);
    }
    for key in grammar.mseq_rule_map().keys() {
        names_to_parse.insert(key);
    }

    // Initialize worklist: all alive nodes whose name is parseable, sorted descending
    let mut sorted_nodes = graph.alive_nodes_sorted();
    sorted_nodes.reverse();
    let mut worklist: Vec<(NodeId, usize)> = sorted_nodes
        .into_iter()
        .filter(|(_, node)| names_to_parse.contains(node.name.as_str()))
        .map(|(id, _)| (id, 0usize))
        .collect();

    while let Some((node_id, depth)) = worklist.pop() {
        if !graph.is_alive(node_id) {
            continue;
        }

        // Clone name once, share across all three expand calls
        let node_name = graph.node(node_id).name.clone();

        let candidates_regular = expand_regular(
            node_id, graph, grammar, &node_name, width_limit, config.ignore_validators,
        )?;
        let candidates_hidden = expand_synthetic(
            node_id, graph, &node_name,
            grammar.hidden_rules(), grammar.hidden_rule_map(),
            width_limit, &NodeKind::Plus, NodeKind::Plus,
        );
        let candidates_mseq = expand_synthetic(
            node_id, graph, &node_name,
            grammar.mseq_rules(), grammar.mseq_rule_map(),
            width_limit, &NodeKind::MSeq, NodeKind::MSeq,
        );

        let all_candidates = candidates_regular
            .into_iter()
            .chain(candidates_hidden)
            .chain(candidates_mseq);

        for candidate in all_candidates {
            let candidate_name = &candidate.name;
            // Check rule_map and hidden_rule_map (not mseq — matches Python behavior)
            let should_push = depth < depth_val
                && (grammar.rule_map().contains_key(candidate_name)
                    || grammar.hidden_rule_map().contains_key(candidate_name));

            if let Some(new_id) = try_add_node(graph, candidate, config) {
                if should_push {
                    worklist.push((new_id, depth + 1));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consecutive::tag_result_to_graph;
    use crate::grammar::{DepthLimit, GrammarBuilder, Rule};
    use estnltk_core::{Annotation, AnnotationValue, MatchSpan, TagResult, TaggedSpan};

    fn make_text3_graph() -> ParseGraph {
        let result = TagResult {
            name: "layer_0".to_string(),
            attributes: vec!["attr_0".to_string()],
            ambiguous: false,
            spans: vec![
                TaggedSpan {
                    span: MatchSpan::new(0, 4),
                    annotations: vec![{
                        let mut a = Annotation::new();
                        a.insert("attr_0".to_string(), AnnotationValue::Str("A".to_string()));
                        a
                    }],
                },
                TaggedSpan {
                    span: MatchSpan::new(4, 5),
                    annotations: vec![{
                        let mut a = Annotation::new();
                        a.insert("attr_0".to_string(), AnnotationValue::Str("B".to_string()));
                        a
                    }],
                },
                TaggedSpan {
                    span: MatchSpan::new(6, 12),
                    annotations: vec![{
                        let mut a = Annotation::new();
                        a.insert("attr_0".to_string(), AnnotationValue::Str("C".to_string()));
                        a
                    }],
                },
                TaggedSpan {
                    span: MatchSpan::new(12, 13),
                    annotations: vec![{
                        let mut a = Annotation::new();
                        a.insert("attr_0".to_string(), AnnotationValue::Str("D".to_string()));
                        a
                    }],
                },
            ],
        };
        tag_result_to_graph(&result, "Tere, maailm!", "attr_0", None, None)
    }

    fn edge_names(graph: &ParseGraph) -> Vec<(String, String)> {
        graph
            .seq_edges_sorted()
            .iter()
            .map(|(a, b)| {
                (
                    graph.node(*a).name.clone(),
                    graph.node(*b).name.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn test_parse_graph_basic() {
        let mut builder = GrammarBuilder::new();
        builder.add_rule(Rule::new("E", "A B C D").unwrap());
        builder.add_rule(Rule::new("F", "A B").unwrap());
        builder.add_rule(Rule::new("G", "B C").unwrap());
        builder.add_rule(Rule::new("H", "C D").unwrap());
        builder.add_rule(Rule::new("I", "F G").unwrap());
        builder.add_rule(Rule::new("J", "F H").unwrap());
        let grammar = builder.build().unwrap();

        let mut graph = make_text3_graph();
        let config = ParseConfig {
            resolve_support_conflicts: false,
            resolve_start_end_conflicts: false,
            resolve_terminals_conflicts: false,
            ignore_validators: false,
        };
        parse_graph(&mut graph, &grammar, &config).unwrap();

        let edges = edge_names(&graph);
        assert!(edges.contains(&("A".into(), "B".into())));
        assert!(edges.contains(&("A".into(), "G".into())));
        assert!(edges.contains(&("F".into(), "C".into())));
        assert!(edges.contains(&("F".into(), "H".into())));
        assert!(edges.contains(&("B".into(), "C".into())));
        assert!(edges.contains(&("B".into(), "H".into())));
        assert!(edges.contains(&("G".into(), "D".into())));
        assert!(edges.contains(&("C".into(), "D".into())));
    }

    #[test]
    fn test_parse_graph_seq() {
        let mut builder = GrammarBuilder::new()
            .depth_limit(DepthLimit::Finite(10));
        builder.add_rule(Rule::new("E", "A").unwrap());
        builder.add_rule(Rule::new("F", "B").unwrap());
        builder.add_rule(Rule::new("F", "C").unwrap());
        builder.add_rule(Rule::new("G", "D").unwrap());
        builder.add_rule(Rule::new("H", "E SEQ(F) G").unwrap());
        let grammar = builder.build().unwrap();

        let mut graph = make_text3_graph();
        let config = ParseConfig {
            resolve_support_conflicts: false,
            resolve_start_end_conflicts: false,
            resolve_terminals_conflicts: false,
            ignore_validators: false,
        };
        parse_graph(&mut graph, &grammar, &config).unwrap();

        let edges = edge_names(&graph);
        assert!(edges.contains(&("A".into(), "B".into())));
        assert!(edges.contains(&("E".into(), "B".into())));
        assert!(edges.contains(&("C".into(), "D".into())));
        assert!(edges.contains(&("C".into(), "G".into())));
        assert!(edges.contains(&("F".into(), "D".into())));
        assert!(edges.contains(&("F".into(), "G".into())));

        let has_seq_f = graph.alive_nodes().any(|(_, n)| n.name == "SEQ(F)");
        assert!(has_seq_f);
    }

    #[test]
    fn test_parse_graph_support_conflicts() {
        let mut builder = GrammarBuilder::new();
        builder.add_rule(Rule::new("E", "A B").unwrap().with_priority(2).with_group(1));
        builder.add_rule(Rule::new("F", "B C").unwrap().with_priority(1).with_group(1));
        builder.add_rule(Rule::new("G", "C D").unwrap().with_priority(0).with_group(1));
        builder.add_rule(Rule::new("K", "A B").unwrap().with_priority(0).with_group(2));
        builder.add_rule(Rule::new("L", "B C").unwrap().with_priority(1).with_group(2));
        builder.add_rule(Rule::new("M", "C D").unwrap().with_priority(2).with_group(2));
        let grammar = builder.build().unwrap();

        let mut graph = make_text3_graph();
        let config = ParseConfig {
            resolve_support_conflicts: true,
            resolve_start_end_conflicts: false,
            resolve_terminals_conflicts: false,
            ignore_validators: false,
        };
        parse_graph(&mut graph, &grammar, &config).unwrap();

        let edges = edge_names(&graph);
        assert!(edges.contains(&("A".into(), "B".into())));
        assert!(edges.contains(&("K".into(), "C".into())));
        assert!(edges.contains(&("K".into(), "G".into())));
        assert!(edges.contains(&("B".into(), "C".into())));
        assert!(edges.contains(&("B".into(), "G".into())));
        assert!(edges.contains(&("C".into(), "D".into())));
        assert!(edges.contains(&("K".into(), "M".into())));
    }
}
