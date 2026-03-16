use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Parameters for the Punkt sentence tokenizer, loaded from a language model.
#[derive(Debug)]
pub struct PunktParameters {
    /// Abbreviation types (lowercased, without trailing period)
    pub abbrev_types: HashSet<String>,
    /// Sentence starters (lowercased)
    pub sent_starters: HashSet<String>,
    /// Collocations: (first_word, second_word) pairs
    pub collocations: HashSet<(String, String)>,
    /// Orthographic context: word -> flags
    pub ortho_context: HashMap<String, u32>,
}

// Orthographic context flag constants (from NLTK punkt.py)
const _ORTHO_BEG_UC: u32 = 1 << 1;  // beginning of sentence, uppercase
const _ORTHO_MID_UC: u32 = 1 << 2;  // middle of sentence, uppercase
const _ORTHO_UNK_UC: u32 = 1 << 3;  // unknown position, uppercase
const _ORTHO_BEG_LC: u32 = 1 << 4;  // beginning of sentence, lowercase
const _ORTHO_MID_LC: u32 = 1 << 5;  // middle of sentence, lowercase
const _ORTHO_UNK_LC: u32 = 1 << 6;  // unknown position, lowercase

pub const ORTHO_LC: u32 = _ORTHO_BEG_LC | _ORTHO_MID_LC | _ORTHO_UNK_LC;
pub const ORTHO_UC: u32 = _ORTHO_BEG_UC | _ORTHO_MID_UC | _ORTHO_UNK_UC;
pub const ORTHO_BEG_UC: u32 = _ORTHO_BEG_UC;
pub const ORTHO_MID_UC: u32 = _ORTHO_MID_UC;

// Embedded model data
const ABBREV_TYPES_DATA: &str = include_str!("../../data/punkt_estonian/abbrev_types.txt");
const SENT_STARTERS_DATA: &str = include_str!("../../data/punkt_estonian/sent_starters.txt");
const COLLOCATIONS_DATA: &str = include_str!("../../data/punkt_estonian/collocations.tab");
const ORTHO_CONTEXT_DATA: &str = include_str!("../../data/punkt_estonian/ortho_context.tab");

static ESTONIAN_PARAMS: OnceLock<PunktParameters> = OnceLock::new();

impl PunktParameters {
    /// Parse model from embedded data files.
    fn parse() -> Self {
        let abbrev_types: HashSet<String> = ABBREV_TYPES_DATA
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let sent_starters: HashSet<String> = SENT_STARTERS_DATA
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let collocations: HashSet<(String, String)> = COLLOCATIONS_DATA
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                let mut parts = line.splitn(2, '\t');
                let first = parts.next()?.to_string();
                let second = parts.next()?.to_string();
                Some((first, second))
            })
            .collect();

        let ortho_context: HashMap<String, u32> = ORTHO_CONTEXT_DATA
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                let mut parts = line.splitn(2, '\t');
                let word = parts.next()?.to_string();
                let flags: u32 = parts.next()?.parse().ok()?;
                Some((word, flags))
            })
            .collect();

        PunktParameters {
            abbrev_types,
            sent_starters,
            collocations,
            ortho_context,
        }
    }

    /// Get the Estonian Punkt parameters (lazily initialized, thread-safe).
    pub fn estonian() -> &'static PunktParameters {
        ESTONIAN_PARAMS.get_or_init(|| PunktParameters::parse())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_estonian_params() {
        let params = PunktParameters::estonian();
        assert!(!params.abbrev_types.is_empty());
        assert!(!params.sent_starters.is_empty());
        assert!(!params.collocations.is_empty());
        assert!(!params.ortho_context.is_empty());
    }

    #[test]
    fn test_abbrev_types() {
        let params = PunktParameters::estonian();
        // Some expected abbreviation types
        assert!(params.abbrev_types.contains("eos") || params.abbrev_types.len() > 40);
    }
}
