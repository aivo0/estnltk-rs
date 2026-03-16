use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use estnltk_core::AnnotationValue;

use crate::node::{GrammarNode, compute_group_hash};

/// Error type for grammar construction and parsing.
#[derive(Debug, thiserror::Error)]
pub enum GrammarError {
    #[error("Repetitive rules: {lhs} -> {rhs}")]
    RepetitiveRule { lhs: String, rhs: String },
    #[error("Infinite grammar without depth limit")]
    InfiniteGrammar,
    #[error("Parenthesis not allowed in symbol: {0}")]
    InvalidSymbol(String),
    #[error("Parenthesis only allowed with SEQ or MSEQ: {0}")]
    InvalidRhs(String),
    #[error("Illegal attributes in decorator output: {0:?}")]
    IllegalAttributes(Vec<String>),
    #[error("{0}")]
    Config(String),
}

// ---------------------------------------------------------------------------
// Callback types
// ---------------------------------------------------------------------------

/// Decorator: given support nodes, return annotation attributes.
pub type DecoratorFn =
    Arc<dyn Fn(&[&GrammarNode]) -> HashMap<String, AnnotationValue> + Send + Sync>;

/// Validator: given support nodes, return whether the rule application is valid.
pub type ValidatorFn = Arc<dyn Fn(&[&GrammarNode]) -> bool + Send + Sync>;

/// Scoring: given support nodes, return a score (higher = preferred).
pub type ScoringFn = Arc<dyn Fn(&[&GrammarNode]) -> f64 + Send + Sync>;

/// Gap validator: given the text between two spans, return whether they should
/// be considered consecutive.
pub type GapValidatorFn = Arc<dyn Fn(&str) -> bool + Send + Sync>;

// ---------------------------------------------------------------------------
// SEQ / MSEQ pattern helpers
// ---------------------------------------------------------------------------

/// If `s` matches `SEQ(X)`, return `Some("X")`.
pub fn match_seq_pattern(s: &str) -> Option<&str> {
    s.strip_prefix("SEQ(").and_then(|rest| rest.strip_suffix(')'))
}

/// If `s` matches `MSEQ(X)`, return `Some("X")`.
pub fn match_mseq_pattern(s: &str) -> Option<&str> {
    s.strip_prefix("MSEQ(").and_then(|rest| rest.strip_suffix(')'))
}

fn contains_parenthesis(s: &str) -> bool {
    s.contains('(') || s.contains(')')
}

// ---------------------------------------------------------------------------
// Depth / Width limits
// ---------------------------------------------------------------------------

/// Depth limit for grammar parsing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepthLimit {
    Finite(u32),
    Unlimited,
}

impl DepthLimit {
    pub fn is_unlimited(&self) -> bool {
        matches!(self, DepthLimit::Unlimited)
    }
}

/// Width limit for grammar parsing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidthLimit {
    Finite(u32),
    Unlimited,
}

impl WidthLimit {
    pub fn exceeds(&self, count: usize) -> bool {
        match self {
            WidthLimit::Finite(n) => count > *n as usize,
            WidthLimit::Unlimited => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Rule
// ---------------------------------------------------------------------------

/// A production rule in the grammar.
///
/// Mirrors Python's `Rule(lhs, rhs, priority, group, decorator, validator, scoring)`.
pub struct Rule {
    pub lhs: String,
    pub rhs: Vec<String>,
    pub priority: i32,
    pub group: i64,
    pub decorator: Option<DecoratorFn>,
    pub validator: Option<ValidatorFn>,
    pub scoring: Option<ScoringFn>,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rule({} -> {} : pri={}, grp={})",
            self.lhs,
            self.rhs.join(" "),
            self.priority,
            self.group
        )
    }
}

impl Rule {
    /// Create a new rule. `rhs` can be a single space-separated string or a `Vec<String>`.
    pub fn new(lhs: impl Into<String>, rhs: impl Into<RhsSpec>) -> Result<Self, GrammarError> {
        let lhs = lhs.into();
        let rhs_vec = rhs.into().into_vec();

        // Validate lhs
        if contains_parenthesis(&lhs)
            && match_seq_pattern(&lhs).is_none()
            && match_mseq_pattern(&lhs).is_none()
        {
            return Err(GrammarError::InvalidSymbol(lhs));
        }

        // Validate rhs symbols
        for r in &rhs_vec {
            if contains_parenthesis(r)
                && match_seq_pattern(r).is_none()
                && match_mseq_pattern(r).is_none()
            {
                return Err(GrammarError::InvalidRhs(format!("{:?}", rhs_vec)));
            }
        }

        let group = compute_group_hash(&lhs, &rhs_vec);

        Ok(Rule {
            lhs,
            rhs: rhs_vec,
            priority: 0,
            group,
            decorator: None,
            validator: None,
            scoring: None,
        })
    }

    /// Builder: set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Builder: set group.
    pub fn with_group(mut self, group: i64) -> Self {
        self.group = group;
        self
    }

    /// Builder: set decorator callback.
    pub fn with_decorator(mut self, f: DecoratorFn) -> Self {
        self.decorator = Some(f);
        self
    }

    /// Builder: set validator callback.
    pub fn with_validator(mut self, f: ValidatorFn) -> Self {
        self.validator = Some(f);
        self
    }

    /// Builder: set scoring callback.
    pub fn with_scoring(mut self, f: ScoringFn) -> Self {
        self.scoring = Some(f);
        self
    }

    /// Call the decorator, returning an empty map if none is set.
    pub fn decorate(&self, support: &[&GrammarNode]) -> HashMap<String, AnnotationValue> {
        match &self.decorator {
            Some(f) => f(support),
            None => HashMap::new(),
        }
    }

    /// Call the validator, returning true if none is set.
    pub fn validate(&self, support: &[&GrammarNode]) -> bool {
        match &self.validator {
            Some(f) => f(support),
            None => true,
        }
    }

    /// Call the scoring function, returning 0.0 if none is set.
    pub fn score(&self, support: &[&GrammarNode]) -> f64 {
        match &self.scoring {
            Some(f) => f(support),
            None => 0.0,
        }
    }
}

/// Specifies the RHS of a rule — either a space-separated string or a vec.
pub enum RhsSpec {
    Str(String),
    Vec(Vec<String>),
}

impl RhsSpec {
    fn into_vec(self) -> Vec<String> {
        match self {
            RhsSpec::Str(s) => s.split_whitespace().map(|w| w.to_string()).collect(),
            RhsSpec::Vec(v) => v,
        }
    }
}

impl From<&str> for RhsSpec {
    fn from(s: &str) -> Self {
        RhsSpec::Str(s.to_string())
    }
}

impl From<String> for RhsSpec {
    fn from(s: String) -> Self {
        RhsSpec::Str(s)
    }
}

impl From<Vec<String>> for RhsSpec {
    fn from(v: Vec<String>) -> Self {
        RhsSpec::Vec(v)
    }
}

impl From<&[&str]> for RhsSpec {
    fn from(v: &[&str]) -> Self {
        RhsSpec::Vec(v.iter().map(|s| s.to_string()).collect())
    }
}

// ---------------------------------------------------------------------------
// Synthetic rule (for SEQ/MSEQ expansions, no callbacks)
// ---------------------------------------------------------------------------

/// A lightweight synthetic rule for SEQ/MSEQ expansion.
/// No decorator/validator/scoring — always uses defaults.
#[derive(Debug, Clone)]
pub struct SyntheticRule {
    pub lhs: String,
    pub rhs: Vec<String>,
    pub priority: i32,
    pub group: i64,
}

impl SyntheticRule {
    pub fn new(lhs: &str, rhs: Vec<&str>) -> Self {
        let lhs_str = lhs.to_string();
        let rhs_vec: Vec<String> = rhs.into_iter().map(|s| s.to_string()).collect();
        let group = compute_group_hash(&lhs_str, &rhs_vec);
        SyntheticRule {
            lhs: lhs_str,
            rhs: rhs_vec,
            priority: 0,
            group,
        }
    }
}

// ---------------------------------------------------------------------------
// GrammarBuilder → Grammar
// ---------------------------------------------------------------------------

/// Mutable builder for constructing a [`Grammar`].
pub struct GrammarBuilder {
    rules: Vec<Rule>,
    start_symbols: Vec<String>,
    depth_limit: DepthLimit,
    width_limit: WidthLimit,
    legal_attributes: HashSet<String>,
}

impl Default for GrammarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GrammarBuilder {
    pub fn new() -> Self {
        GrammarBuilder {
            rules: Vec::new(),
            start_symbols: Vec::new(),
            depth_limit: DepthLimit::Unlimited,
            width_limit: WidthLimit::Unlimited,
            legal_attributes: HashSet::new(),
        }
    }

    pub fn start_symbols(mut self, symbols: Vec<impl Into<String>>) -> Self {
        self.start_symbols = symbols.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn depth_limit(mut self, limit: DepthLimit) -> Self {
        self.depth_limit = limit;
        self
    }

    pub fn width_limit(mut self, limit: WidthLimit) -> Self {
        self.width_limit = limit;
        self
    }

    pub fn legal_attributes(mut self, attrs: HashSet<String>) -> Self {
        self.legal_attributes = attrs;
        self
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Convenience: construct and add a rule in one call.
    pub fn add(
        &mut self,
        lhs: impl Into<String>,
        rhs: impl Into<RhsSpec>,
    ) -> Result<(), GrammarError> {
        self.rules.push(Rule::new(lhs, rhs)?);
        Ok(())
    }

    /// Build an immutable, validated [`Grammar`].
    pub fn build(self) -> Result<Grammar, GrammarError> {
        Grammar::compile(
            self.rules,
            self.start_symbols,
            self.depth_limit,
            self.width_limit,
            self.legal_attributes,
        )
    }
}

/// An immutable, compiled grammar ready for parsing.
///
/// Created via [`GrammarBuilder::build()`]. All rule maps and metadata are
/// precomputed. Can be shared across threads with `Arc<Grammar>`.
pub struct Grammar {
    rules: Vec<Rule>,
    start_symbols: Vec<String>,
    depth_limit: DepthLimit,
    width_limit: WidthLimit,
    legal_attributes: HashSet<String>,
    terminals: HashSet<String>,
    nonterminals: HashSet<String>,
    /// Maps a symbol name → list of `(rule_index, position_in_rhs)`.
    rule_map: HashMap<String, Vec<(usize, usize)>>,
    /// Synthetic rules for `SEQ(X)` expansion.
    hidden_rules: Vec<SyntheticRule>,
    /// Maps a symbol name → list of `(hidden_rule_index, position_in_rhs)`.
    hidden_rule_map: HashMap<String, Vec<(usize, usize)>>,
    /// Synthetic rules for `MSEQ(X)` expansion.
    mseq_rules: Vec<SyntheticRule>,
    /// Maps a symbol name → list of `(mseq_rule_index, position_in_rhs)`.
    mseq_rule_map: HashMap<String, Vec<(usize, usize)>>,
    /// SEQ symbol pairs: (SEQ(X), X).
    plus_symbols: Vec<(String, String)>,
    /// MSEQ symbol pairs: (MSEQ(X), X).
    mseq_symbols: Vec<(String, String)>,
}

impl std::fmt::Debug for Grammar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grammar")
            .field("start_symbols", &self.start_symbols)
            .field("terminals", &self.terminals)
            .field("nonterminals", &self.nonterminals)
            .field("rules", &self.rules)
            .field("depth_limit", &self.depth_limit)
            .field("width_limit", &self.width_limit)
            .finish()
    }
}

impl Grammar {
    fn compile(
        rules: Vec<Rule>,
        start_symbols: Vec<String>,
        depth_limit: DepthLimit,
        width_limit: WidthLimit,
        legal_attributes: HashSet<String>,
    ) -> Result<Self, GrammarError> {
        // Check for duplicate (lhs, rhs) pairs
        let mut seen: HashSet<(&str, &[String])> = HashSet::new();
        for rule in &rules {
            if !seen.insert((&rule.lhs, &rule.rhs)) {
                return Err(GrammarError::RepetitiveRule {
                    lhs: rule.lhs.clone(),
                    rhs: rule.rhs.join(" "),
                });
            }
        }

        // Build rule_map and collect SEQ/MSEQ symbols
        let mut rule_map: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let mut plus_symbols: HashSet<(String, String)> = HashSet::new();
        let mut mseq_symbols: HashSet<(String, String)> = HashSet::new();

        for (rule_idx, rule) in rules.iter().enumerate() {
            for (pos, rhs_sym) in rule.rhs.iter().enumerate() {
                rule_map
                    .entry(rhs_sym.clone())
                    .or_default()
                    .push((rule_idx, pos));
                if let Some(inner) = match_seq_pattern(rhs_sym) {
                    plus_symbols.insert((rhs_sym.clone(), inner.to_string()));
                } else if let Some(inner) = match_mseq_pattern(rhs_sym) {
                    mseq_symbols.insert((rhs_sym.clone(), inner.to_string()));
                }
            }
        }

        // Build hidden_rule_map for SEQ expansions
        let mut hidden_rules = Vec::new();
        let mut hidden_rule_map: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        for (ps, s) in &plus_symbols {
            // SEQ(X) -> SEQ(X) SEQ(X)  (position 0 and 1)
            let rule_self = SyntheticRule::new(ps, vec![ps, ps]);
            let idx = hidden_rules.len();
            hidden_rules.push(rule_self);
            hidden_rule_map
                .entry(ps.clone())
                .or_default()
                .push((idx, 0));
            hidden_rule_map
                .entry(ps.clone())
                .or_default()
                .push((idx, 1));

            // SEQ(X) -> X  (position 0)
            let rule_base = SyntheticRule::new(ps, vec![s]);
            let idx = hidden_rules.len();
            hidden_rules.push(rule_base);
            hidden_rule_map
                .entry(s.clone())
                .or_default()
                .push((idx, 0));
        }

        // Build mseq_rule_map for MSEQ expansions
        let mut mseq_rules = Vec::new();
        let mut mseq_rule_map: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

        for (ps, s) in &mseq_symbols {
            // MSEQ(X) -> MSEQ(X) MSEQ(X)  (position 0 and 1)
            let rule_self = SyntheticRule::new(ps, vec![ps, ps]);
            let idx = mseq_rules.len();
            mseq_rules.push(rule_self);
            mseq_rule_map
                .entry(ps.clone())
                .or_default()
                .push((idx, 0));
            mseq_rule_map
                .entry(ps.clone())
                .or_default()
                .push((idx, 1));

            // MSEQ(X) -> X  (position 0)
            let rule_base = SyntheticRule::new(ps, vec![s]);
            let idx = mseq_rules.len();
            mseq_rules.push(rule_base);
            mseq_rule_map
                .entry(s.clone())
                .or_default()
                .push((idx, 0));
        }

        // Compute terminals and nonterminals
        let mut nonterminals: HashSet<String> = rules.iter().map(|r| r.lhs.clone()).collect();
        let mut terminals: HashSet<String> = HashSet::new();
        for rule in &rules {
            for sym in &rule.rhs {
                if let Some(inner) = match_seq_pattern(sym) {
                    nonterminals.insert(sym.clone());
                    terminals.insert(inner.to_string());
                } else if let Some(inner) = match_mseq_pattern(sym) {
                    nonterminals.insert(sym.clone());
                    terminals.insert(inner.to_string());
                } else {
                    terminals.insert(sym.clone());
                }
            }
        }
        // Terminals = all symbols that appear only on RHS (not on LHS)
        terminals.retain(|s| !nonterminals.contains(s));

        let plus_symbols: Vec<(String, String)> = plus_symbols.into_iter().collect();
        let mseq_symbols: Vec<(String, String)> = mseq_symbols.into_iter().collect();

        let grammar = Grammar {
            rules,
            start_symbols,
            depth_limit,
            width_limit,
            legal_attributes,
            terminals,
            nonterminals,
            rule_map,
            hidden_rules,
            hidden_rule_map,
            mseq_rules,
            mseq_rule_map,
            plus_symbols,
            mseq_symbols,
        };

        // Check for infinite grammar
        if depth_limit.is_unlimited() && !grammar.has_finite_max_depth() {
            return Err(GrammarError::InfiniteGrammar);
        }

        Ok(grammar)
    }

    // -- Accessors --

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn start_symbols(&self) -> &[String] {
        &self.start_symbols
    }

    pub fn depth_limit(&self) -> DepthLimit {
        self.depth_limit
    }

    pub fn width_limit(&self) -> WidthLimit {
        self.width_limit
    }

    pub fn legal_attributes(&self) -> &HashSet<String> {
        &self.legal_attributes
    }

    pub fn terminals(&self) -> &HashSet<String> {
        &self.terminals
    }

    pub fn nonterminals(&self) -> &HashSet<String> {
        &self.nonterminals
    }

    pub fn rule_map(&self) -> &HashMap<String, Vec<(usize, usize)>> {
        &self.rule_map
    }

    pub fn hidden_rules(&self) -> &[SyntheticRule] {
        &self.hidden_rules
    }

    pub fn hidden_rule_map(&self) -> &HashMap<String, Vec<(usize, usize)>> {
        &self.hidden_rule_map
    }

    pub fn mseq_rules(&self) -> &[SyntheticRule] {
        &self.mseq_rules
    }

    pub fn mseq_rule_map(&self) -> &HashMap<String, Vec<(usize, usize)>> {
        &self.mseq_rule_map
    }

    /// Check if the grammar has finite max depth (is acyclic).
    ///
    /// Builds a directed graph of rule dependencies and checks for cycles
    /// using 3-color DFS (white/gray/black).
    pub fn has_finite_max_depth(&self) -> bool {
        // Build adjacency list: symbol → set of symbols it depends on
        let mut adj: HashMap<&str, HashSet<&str>> = HashMap::new();
        for rule in &self.rules {
            for sym in &rule.rhs {
                adj.entry(&rule.lhs).or_default().insert(sym);
            }
        }
        for (ps, s) in &self.plus_symbols {
            adj.entry(ps).or_default().insert(s);
        }
        for (ps, s) in &self.mseq_symbols {
            adj.entry(ps).or_default().insert(s);
        }

        // DFS cycle detection: 0=white, 1=gray, 2=black
        let all_nodes: HashSet<&str> = adj
            .keys()
            .copied()
            .chain(adj.values().flat_map(|v| v.iter().copied()))
            .collect();
        let mut color: HashMap<&str, u8> = all_nodes.iter().map(|&n| (n, 0u8)).collect();

        fn has_cycle<'a>(
            node: &'a str,
            adj: &HashMap<&str, HashSet<&'a str>>,
            color: &mut HashMap<&'a str, u8>,
        ) -> bool {
            color.insert(node, 1); // gray
            if let Some(neighbors) = adj.get(node) {
                for &neighbor in neighbors {
                    match color.get(neighbor) {
                        Some(1) => return true,  // back edge → cycle
                        Some(0) | None => {
                            if has_cycle(neighbor, adj, color) {
                                return true;
                            }
                        }
                        _ => {} // black, already processed
                    }
                }
            }
            color.insert(node, 2); // black
            false
        }

        for &node in &all_nodes {
            if color.get(node) == Some(&0) {
                if has_cycle(node, &adj, &mut color) {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_seq_pattern() {
        assert_eq!(match_seq_pattern("SEQ(bla)"), Some("bla"));
        assert_eq!(match_seq_pattern("MSEQ()"), None);
        assert_eq!(match_seq_pattern("SEQ(A B)"), Some("A B"));
        assert_eq!(match_seq_pattern("notSEQ(X)"), None);
    }

    #[test]
    fn test_match_mseq_pattern() {
        assert_eq!(match_mseq_pattern("MSEQ(X)"), Some("X"));
        assert_eq!(match_mseq_pattern("SEQ(X)"), None);
    }

    #[test]
    fn test_rule_defaults() {
        let rule = Rule::new("A", "B SEQ(C)").unwrap();
        assert_eq!(rule.lhs, "A");
        assert_eq!(rule.rhs, vec!["B", "SEQ(C)"]);
        assert_eq!(rule.priority, 0);
        assert!(rule.decorator.is_none());
        assert!(rule.validator.is_none());
        assert!(rule.scoring.is_none());
    }

    #[test]
    fn test_rule_with_methods() {
        let rule = Rule::new("A", "B C")
            .unwrap()
            .with_priority(10)
            .with_group(42);
        assert_eq!(rule.priority, 10);
        assert_eq!(rule.group, 42);
    }

    #[test]
    fn test_rule_invalid_lhs() {
        let result = Rule::new("(bad", "B");
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_invalid_rhs() {
        let result = Rule::new("A", ")bad");
        assert!(result.is_err());
    }

    #[test]
    fn test_grammar_basic() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["S"])
            .depth_limit(DepthLimit::Finite(5))
            .width_limit(WidthLimit::Finite(20))
            .legal_attributes(HashSet::from([
                "attr_1".to_string(),
                "attr_2".to_string(),
            ]));
        builder.add_rule(Rule::new("S", "A").unwrap());
        builder.add_rule(Rule::new("S", "B").unwrap());
        builder.add_rule(Rule::new("A", "B F").unwrap());
        builder.add_rule(Rule::new("B", "G").unwrap());

        let grammar = builder.build().unwrap();
        assert_eq!(grammar.start_symbols(), &["S"]);
        assert_eq!(grammar.rules().len(), 4);
        assert_eq!(grammar.terminals(), &HashSet::from(["F".into(), "G".into()]));
        assert_eq!(
            grammar.nonterminals(),
            &HashSet::from(["A".into(), "B".into(), "S".into()])
        );
        assert!(grammar.hidden_rule_map().is_empty());
        assert!(grammar.has_finite_max_depth());
    }

    #[test]
    fn test_grammar_with_seq() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["S"])
            .depth_limit(DepthLimit::Finite(5));
        builder.add_rule(Rule::new("S", "A").unwrap());
        builder.add_rule(Rule::new("S", "B").unwrap());
        builder.add_rule(Rule::new("A", "B F").unwrap());
        builder.add_rule(Rule::new("B", "G").unwrap());
        builder.add_rule(Rule::new("B", "SEQ(D)").unwrap());

        let grammar = builder.build().unwrap();
        assert_eq!(grammar.rules().len(), 5);
        assert!(!grammar.hidden_rule_map().is_empty());
        assert!(grammar.has_finite_max_depth());
    }

    #[test]
    fn test_grammar_cyclic_needs_depth_limit() {
        let mut builder = GrammarBuilder::new().start_symbols(vec!["A"]);
        builder.add_rule(Rule::new("A", "A B").unwrap());
        // Without depth limit, should fail
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_grammar_seq_self_cyclic() {
        let mut builder = GrammarBuilder::new().start_symbols(vec!["A"]);
        builder.add_rule(Rule::new("A", "SEQ(A)").unwrap());
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_grammar_cyclic_with_depth_limit_ok() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["A"])
            .depth_limit(DepthLimit::Finite(5));
        builder.add_rule(Rule::new("A", "A B").unwrap());
        let grammar = builder.build().unwrap();
        assert!(!grammar.has_finite_max_depth());
    }

    #[test]
    fn test_grammar_seq_terminals() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["A"]);
        builder.add_rule(Rule::new("A", "SEQ(B)").unwrap());
        let grammar = builder.build().unwrap();
        assert_eq!(grammar.terminals(), &HashSet::from(["B".into()]));
        assert!(grammar.nonterminals().contains("A"));
        assert!(grammar.nonterminals().contains("SEQ(B)"));
    }

    #[test]
    fn test_grammar_repetitive_rules() {
        let mut builder = GrammarBuilder::new().start_symbols(vec!["S"]);
        builder.add_rule(Rule::new("S", "A").unwrap());
        builder.add_rule(Rule::new("S", "A").unwrap());
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_grammar_empty() {
        let builder = GrammarBuilder::new();
        let grammar = builder.build().unwrap();
        assert!(grammar.rules().is_empty());
        assert!(grammar.terminals().is_empty());
    }
}
