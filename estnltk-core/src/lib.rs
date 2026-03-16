pub mod types;
pub mod conflict;
pub mod byte_char;

// Re-export key items at crate root for convenience
pub use types::*;
pub use conflict::{resolve_conflicts, MatchEntry, RuleIndex, keep_maximal_matches,
                   keep_minimal_matches, conflict_priority_resolver};
pub use byte_char::{byte_to_char_map, char_to_byte_map};
