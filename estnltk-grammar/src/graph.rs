use std::collections::{HashMap, HashSet, VecDeque};

use smallvec::SmallVec;

use crate::node::{GrammarNode, NodeId, NodeKey, NodeKind};

/// A dual directed graph used during grammar parsing.
///
/// Contains two intertwined graphs:
/// 1. **Sequence graph**: edges represent consecutive ordering of spans/nodes.
///    Used by the parser to find matching sequences for rule RHS.
/// 2. **Parse tree**: edges track derivation (parent → support children).
///    Used for cascading removal when conflict resolution removes a node.
///
/// Replaces Python's NetworkX-based `LayerGraph` + `parse_trees`.
pub struct ParseGraph {
    /// Arena storage: all nodes, indexed by NodeId. Append-only.
    nodes: Vec<GrammarNode>,
    /// Whether each node is alive. `false` = removed by conflict resolution.
    alive: Vec<bool>,
    /// Number of alive nodes (maintained for O(1) len()).
    alive_count: usize,

    // -- Sequence graph (consecutive ordering) --
    seq_succ: Vec<SmallVec<[NodeId; 4]>>,
    seq_pred: Vec<SmallVec<[NodeId; 4]>>,

    // -- Parse tree (derivation) --
    /// child → parent nodes (nodes whose support contains this child)
    tree_parents: Vec<SmallVec<[NodeId; 4]>>,
    /// parent → children (the support of this node)
    tree_children: Vec<SmallVec<[NodeId; 4]>>,

    // -- Indices --
    /// (start, end) → list of alive NodeIds at that position
    span_index: HashMap<(usize, usize), SmallVec<[NodeId; 8]>>,
    /// Deduplication set: prevents adding nodes with the same identity
    identity_set: HashSet<NodeKey>,
}

impl ParseGraph {
    pub fn new() -> Self {
        ParseGraph {
            nodes: Vec::new(),
            alive: Vec::new(),
            alive_count: 0,
            seq_succ: Vec::new(),
            seq_pred: Vec::new(),
            tree_parents: Vec::new(),
            tree_children: Vec::new(),
            span_index: HashMap::new(),
            identity_set: HashSet::new(),
        }
    }

    /// Number of alive nodes (O(1)).
    pub fn len(&self) -> usize {
        self.alive_count
    }

    pub fn is_empty(&self) -> bool {
        self.alive_count == 0
    }

    /// Total nodes ever added (including removed).
    pub fn total_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Check if a node with this identity already exists (alive or dead).
    pub fn contains_key(&self, key: &NodeKey) -> bool {
        self.identity_set.contains(key)
    }

    /// Check if a node is alive.
    pub fn is_alive(&self, id: NodeId) -> bool {
        let idx = id as usize;
        idx < self.alive.len() && self.alive[idx]
    }

    /// Get a node by ID (may be dead).
    pub fn node(&self, id: NodeId) -> &GrammarNode {
        &self.nodes[id as usize]
    }

    /// Get a mutable node by ID.
    pub fn node_mut(&mut self, id: NodeId) -> &mut GrammarNode {
        &mut self.nodes[id as usize]
    }

    /// Add a node to the graph. Returns its NodeId.
    ///
    /// If a node with the same identity already exists, this is a no-op
    /// and returns `None`.
    pub fn add_node(&mut self, node: GrammarNode) -> Option<NodeId> {
        let key = node.identity_key();
        self.add_node_with_key(node, key)
    }

    /// Add a node with a pre-computed identity key, avoiding redundant allocation.
    pub fn add_node_with_key(&mut self, mut node: GrammarNode, key: NodeKey) -> Option<NodeId> {
        if self.identity_set.contains(&key) {
            return None;
        }

        let id = self.nodes.len() as NodeId;
        node.id = id;
        self.nodes.push(node);
        self.alive.push(true);
        self.alive_count += 1;
        self.seq_succ.push(SmallVec::new());
        self.seq_pred.push(SmallVec::new());
        self.tree_parents.push(SmallVec::new());
        self.tree_children.push(SmallVec::new());

        let node = &self.nodes[id as usize];

        // Update span index
        self.span_index
            .entry((node.start, node.end))
            .or_default()
            .push(id);

        // Update parse tree: record this node as parent of its support children
        for i in 0..node.support.len() {
            let child_id = self.nodes[id as usize].support[i];
            self.tree_parents[child_id as usize].push(id);
            self.tree_children[id as usize].push(child_id);
        }

        self.identity_set.insert(key);
        Some(id)
    }

    /// Add a directed edge in the sequence graph (from → to).
    pub fn add_seq_edge(&mut self, from: NodeId, to: NodeId) {
        self.seq_succ[from as usize].push(to);
        self.seq_pred[to as usize].push(from);
    }

    /// Get alive sequence successors.
    pub fn seq_succ(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.seq_succ[id as usize]
            .iter()
            .copied()
            .filter(|&nid| self.is_alive(nid))
    }

    /// Get alive sequence predecessors.
    pub fn seq_pred(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.seq_pred[id as usize]
            .iter()
            .copied()
            .filter(|&nid| self.is_alive(nid))
    }

    /// Get alive parse-tree parents (nodes whose support contains this node).
    pub fn tree_parents(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.tree_parents[id as usize]
            .iter()
            .copied()
            .filter(|&nid| self.is_alive(nid))
    }

    /// Get all alive nodes at a given (start, end) position.
    pub fn nodes_at(&self, start: usize, end: usize) -> impl Iterator<Item = NodeId> + '_ {
        self.span_index
            .get(&(start, end))
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
            .filter(|&id| self.is_alive(id))
    }

    /// Iterate over all alive nodes, yielding (NodeId, &GrammarNode).
    pub fn alive_nodes(&self) -> impl Iterator<Item = (NodeId, &GrammarNode)> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| self.alive[*i])
            .map(|(i, n)| (i as NodeId, n))
    }

    /// Iterate over all alive nodes, sorted by (start, end, name).
    pub fn alive_nodes_sorted(&self) -> Vec<(NodeId, &GrammarNode)> {
        let mut result: Vec<_> = self.alive_nodes().collect();
        result.sort_by(|(_, a), (_, b)| a.cmp(b));
        result
    }

    /// Remove nodes and all their transitive parse-tree ancestors.
    ///
    /// Only NonTerminal/Plus/MSeq nodes can be removed (terminals are permanent).
    pub fn remove_with_ancestors(&mut self, nodes: &[NodeId]) {
        let mut to_remove: HashSet<NodeId> = HashSet::new();

        // Collect all ancestors via BFS upward in the parse tree
        let mut queue: VecDeque<NodeId> = nodes.iter().copied().collect();
        while let Some(nid) = queue.pop_front() {
            if !to_remove.insert(nid) {
                continue;
            }
            // Add all alive parse-tree parents
            for &parent in &self.tree_parents[nid as usize] {
                if self.is_alive(parent) && !to_remove.contains(&parent) {
                    queue.push_back(parent);
                }
            }
        }

        // Mark all collected nodes as dead
        for &nid in &to_remove {
            let node = &self.nodes[nid as usize];
            debug_assert!(
                !matches!(node.kind, NodeKind::Terminal { .. }),
                "Attempt to remove terminal node"
            );
            self.alive[nid as usize] = false;
            self.alive_count -= 1;

            // Remove from span index
            if let Some(ids) = self.span_index.get_mut(&(node.start, node.end)) {
                ids.retain(|id| *id != nid);
            }
        }
    }

    /// Check if a sequence edge exists between two alive nodes.
    pub fn has_seq_edge(&self, from: NodeId, to: NodeId) -> bool {
        self.is_alive(from)
            && self.is_alive(to)
            && self.seq_succ[from as usize].contains(&to)
    }

    /// Get all alive edges in the sequence graph, sorted. Test-only.
    #[cfg(test)]
    pub fn seq_edges_sorted(&self) -> Vec<(NodeId, NodeId)> {
        let mut edges = Vec::new();
        for (from_idx, succs) in self.seq_succ.iter().enumerate() {
            let from = from_idx as NodeId;
            if !self.is_alive(from) {
                continue;
            }
            for &to in succs {
                if self.is_alive(to) {
                    edges.push((from, to));
                }
            }
        }
        edges.sort_by(|(a_from, a_to), (b_from, b_to)| {
            let a = (&self.nodes[*a_from as usize], &self.nodes[*a_to as usize]);
            let b = (&self.nodes[*b_from as usize], &self.nodes[*b_to as usize]);
            a.cmp(&b)
        });
        edges
    }
}

impl Default for ParseGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn make_terminal(name: &str, start: usize, end: usize, span_index: usize) -> GrammarNode {
        GrammarNode {
            id: 0, // will be set by add_node
            name: name.to_string(),
            start,
            end,
            kind: NodeKind::Terminal { span_index },
            support: vec![],
            terminals: vec![], // will be patched after add
            group: 0,
            priority: 0,
            score: 0.0,
            attributes: HashMap::new(),
            text: None,
        }
    }

    fn make_nonterminal(
        name: &str,
        support: Vec<NodeId>,
        start: usize,
        end: usize,
        terminals: Vec<NodeId>,
    ) -> GrammarNode {
        let group = crate::node::compute_group_hash(name, &support);
        GrammarNode {
            id: 0,
            name: name.to_string(),
            start,
            end,
            kind: NodeKind::NonTerminal,
            support,
            terminals,
            group,
            priority: 0,
            score: 0.0,
            attributes: HashMap::new(),
            text: None,
        }
    }

    #[test]
    fn test_add_and_alive() {
        let mut g = ParseGraph::new();
        let mut t = make_terminal("A", 0, 4, 0);
        t.terminals = vec![0]; // self-referencing (will be id 0)
        let id = g.add_node(t).unwrap();
        assert_eq!(id, 0);
        assert!(g.is_alive(0));
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn test_dedup() {
        let mut g = ParseGraph::new();
        let mut t1 = make_terminal("A", 0, 4, 0);
        t1.terminals = vec![0];
        let mut t2 = make_terminal("A", 0, 4, 0);
        t2.terminals = vec![1]; // same identity, different details
        assert!(g.add_node(t1).is_some());
        assert!(g.add_node(t2).is_none()); // duplicate
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn test_seq_edges() {
        let mut g = ParseGraph::new();
        let mut ta = make_terminal("A", 0, 4, 0);
        ta.terminals = vec![0];
        let mut tb = make_terminal("B", 4, 5, 1);
        tb.terminals = vec![1];

        let id_a = g.add_node(ta).unwrap();
        let id_b = g.add_node(tb).unwrap();
        g.add_seq_edge(id_a, id_b);

        let succs: Vec<_> = g.seq_succ(id_a).collect();
        assert_eq!(succs, vec![id_b]);

        let preds: Vec<_> = g.seq_pred(id_b).collect();
        assert_eq!(preds, vec![id_a]);

        assert!(g.has_seq_edge(id_a, id_b));
    }

    #[test]
    fn test_span_index() {
        let mut g = ParseGraph::new();
        let mut ta = make_terminal("A", 0, 4, 0);
        ta.terminals = vec![0];
        g.add_node(ta).unwrap();

        let at_0_4: Vec<_> = g.nodes_at(0, 4).collect();
        assert_eq!(at_0_4, vec![0]);

        let at_5_6: Vec<_> = g.nodes_at(5, 6).collect();
        assert!(at_5_6.is_empty());
    }

    #[test]
    fn test_remove_with_ancestors() {
        let mut g = ParseGraph::new();

        // Build: A(0,4) -> B(4,5) ; E(A,B) is nonterminal
        let mut ta = make_terminal("A", 0, 4, 0);
        ta.terminals = vec![0];
        let id_a = g.add_node(ta).unwrap();

        let mut tb = make_terminal("B", 4, 5, 1);
        tb.terminals = vec![1];
        let id_b = g.add_node(tb).unwrap();

        g.add_seq_edge(id_a, id_b);

        // E depends on A and B
        let e = make_nonterminal("E", vec![id_a, id_b], 0, 5, vec![id_a, id_b]);
        let id_e = g.add_node(e).unwrap();

        assert!(g.is_alive(id_e));

        // Remove E
        g.remove_with_ancestors(&[id_e]);
        assert!(!g.is_alive(id_e));
        // Terminals survive
        assert!(g.is_alive(id_a));
        assert!(g.is_alive(id_b));
    }

    #[test]
    fn test_cascade_removal() {
        let mut g = ParseGraph::new();

        let mut ta = make_terminal("A", 0, 4, 0);
        ta.terminals = vec![0];
        let id_a = g.add_node(ta).unwrap();

        let mut tb = make_terminal("B", 4, 5, 1);
        tb.terminals = vec![1];
        let id_b = g.add_node(tb).unwrap();

        g.add_seq_edge(id_a, id_b);

        // F depends on A, B
        let f = make_nonterminal("F", vec![id_a, id_b], 0, 5, vec![id_a, id_b]);
        let id_f = g.add_node(f).unwrap();

        // G depends on F (grandchild of A, B)
        let g_node = make_nonterminal("G", vec![id_f], 0, 5, vec![id_a, id_b]);
        let id_g = g.add_node(g_node).unwrap();

        assert!(g.is_alive(id_f));
        assert!(g.is_alive(id_g));

        // Remove F — should cascade to G (ancestor in parse tree)
        g.remove_with_ancestors(&[id_f]);
        assert!(!g.is_alive(id_f));
        assert!(!g.is_alive(id_g)); // cascade removed
    }
}
