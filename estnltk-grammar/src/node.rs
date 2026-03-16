use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use estnltk_core::AnnotationValue;

/// Stable identifier for a node in the [`ParseGraph`](crate::graph::ParseGraph).
pub type NodeId = u32;

/// Compute a group hash from a name and a hashable value.
/// Used for default group IDs on rules and nodes.
pub fn compute_group_hash(name: &str, items: &impl Hash) -> i64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (name, items).hash(&mut h);
    h.finish() as i64
}

/// Discriminates node kinds in the parse graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A leaf node representing an input-layer span.
    Terminal {
        /// Index into the input `TagResult.spans`.
        span_index: usize,
    },
    /// Derived from a regular grammar rule.
    NonTerminal,
    /// Derived from a `SEQ(X)` expansion (one-or-more repetitions).
    Plus,
    /// Derived from an `MSEQ(X)` expansion (zero-or-more repetitions).
    MSeq,
}

/// A node in the parse graph.
///
/// Mirrors the Python `GrammarNode` hierarchy (TerminalNode, NonTerminalNode,
/// PlusNode, MSeqNode) as a single struct with a [`NodeKind`] discriminant.
#[derive(Debug, Clone)]
pub struct GrammarNode {
    pub id: NodeId,
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub kind: NodeKind,
    /// IDs of support (child) nodes. Empty for terminals.
    pub support: Vec<NodeId>,
    /// Flattened terminal node IDs, sorted by position.
    pub terminals: Vec<NodeId>,
    pub group: i64,
    pub priority: i32,
    pub score: f64,
    /// User-defined attributes from rule decorator.
    pub attributes: HashMap<String, AnnotationValue>,
    /// The text of the span (only set for Terminal nodes).
    pub text: Option<String>,
}

impl GrammarNode {
    /// Compute the identity key used for deduplication.
    ///
    /// Mirrors Python semantics:
    /// - `TerminalNode.__eq__`: `(name, span.base_span)`
    /// - `NonTerminalNode.__eq__`: `(name, support_tuple)`
    pub fn identity_key(&self) -> NodeKey {
        match &self.kind {
            NodeKind::Terminal { .. } => NodeKey::Terminal {
                name: self.name.clone(),
                span_start: self.start,
                span_end: self.end,
            },
            _ => NodeKey::NonTerminal {
                name: self.name.clone(),
                support: self.support.clone(),
            },
        }
    }
}

/// Implement PartialEq/Hash inline to avoid allocating a NodeKey on every comparison.
impl PartialEq for GrammarNode {
    fn eq(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (NodeKind::Terminal { .. }, NodeKind::Terminal { .. }) => {
                self.name == other.name && self.start == other.start && self.end == other.end
            }
            (NodeKind::Terminal { .. }, _) | (_, NodeKind::Terminal { .. }) => false,
            _ => self.name == other.name && self.support == other.support,
        }
    }
}

impl Eq for GrammarNode {}

impl Hash for GrammarNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.kind {
            NodeKind::Terminal { .. } => {
                0u8.hash(state); // discriminant
                self.name.hash(state);
                self.start.hash(state);
                self.end.hash(state);
            }
            _ => {
                1u8.hash(state);
                self.name.hash(state);
                self.support.hash(state);
            }
        }
    }
}

impl PartialOrd for GrammarNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GrammarNode {
    /// Order by (start, end, name) — mirrors Python `Node.__lt__`.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.start, self.end, &self.name).cmp(&(other.start, other.end, &other.name))
    }
}

/// Identity key for node deduplication.
///
/// Two different key variants are needed because Terminal and NonTerminal nodes
/// use different equality semantics in Python.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum NodeKey {
    Terminal {
        name: String,
        span_start: usize,
        span_end: usize,
    },
    NonTerminal {
        name: String,
        support: Vec<NodeId>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_terminal(id: NodeId, name: &str, start: usize, end: usize) -> GrammarNode {
        GrammarNode {
            id,
            name: name.to_string(),
            start,
            end,
            kind: NodeKind::Terminal { span_index: 0 },
            support: vec![],
            terminals: vec![id],
            group: 0,
            priority: 0,
            score: 0.0,
            attributes: HashMap::new(),
            text: None,
        }
    }

    #[test]
    fn terminal_identity() {
        let a = make_terminal(0, "A", 0, 4);
        let b = make_terminal(1, "A", 0, 4);
        assert_eq!(a.identity_key(), b.identity_key());
        assert_eq!(a, b); // also test inline eq
    }

    #[test]
    fn terminal_ordering() {
        let a = make_terminal(0, "A", 0, 4);
        let b = make_terminal(1, "B", 4, 5);
        assert!(a < b);
    }

    #[test]
    fn nonterminal_identity() {
        let key1 = NodeKey::NonTerminal {
            name: "E".to_string(),
            support: vec![0, 1],
        };
        let key2 = NodeKey::NonTerminal {
            name: "E".to_string(),
            support: vec![0, 1],
        };
        let key3 = NodeKey::NonTerminal {
            name: "E".to_string(),
            support: vec![0, 2],
        };
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}
