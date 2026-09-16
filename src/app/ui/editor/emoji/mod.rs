mod candidates;
mod overlay;
mod parsing;
mod state;

pub use candidates::{exact_match, filter_candidates};
pub use overlay::render_overlay;
pub use parsing::{MIN_QUERY_CHARS, is_shortcode_char, should_open_emoji};
pub use state::EmojiState;
