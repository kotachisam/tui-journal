mod candidates;
mod overlay;
mod parsing;
mod state;
mod substitution;
mod tokens;

pub use candidates::{build_candidates, filter_candidates};
pub use overlay::render_overlay;
pub use parsing::{
    DocMention, is_break_char, parse_anchor_suffix_buffer, parse_mentions_in_doc,
    parse_mentions_in_line, should_open_mention,
};
pub use state::MentionState;
pub use substitution::{RenderedMention, render_mention_label, substitute_mentions};
pub use tokens::{find_anchor_line, format_mention_token};
