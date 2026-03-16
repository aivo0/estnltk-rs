pub mod regex_tagger;
pub mod substring_tagger;
pub mod span_tagger;
pub mod phrase_tagger;

pub use regex_tagger::{RegexTagger, ExtractionRule, make_rule};
pub use substring_tagger::{SubstringTagger, SubstringRule};
pub use span_tagger::{SpanTagger, SpanRule, SpanTaggerConfig};
pub use phrase_tagger::{PhraseTagger, PhraseRule, PhraseTaggerConfig, make_phrase_rule};
