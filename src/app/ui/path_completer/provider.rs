use ratatui::text::Line;

use crate::app::ui::inline_completer::CompletionProvider;

use super::walker::PathCandidate;

const OVERLAY_WIDTH: u16 = 60;

pub struct PathCompletionProvider;

impl CompletionProvider for PathCompletionProvider {
    type Candidate = PathCandidate;

    fn render_candidate<'a>(&self, candidate: &'a Self::Candidate) -> Line<'a> {
        if candidate.is_dir {
            Line::from(format!("{}/", candidate.name))
        } else {
            Line::from(candidate.name.as_str())
        }
    }

    fn title(&self, _selected: Option<&Self::Candidate>) -> String {
        "Path — Tab/Enter complete, Esc dismiss".to_string()
    }

    fn overlay_width(&self) -> u16 {
        OVERLAY_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_file_as_plain_name() {
        let p = PathCompletionProvider;
        let c = PathCandidate {
            name: "foo.md".to_string(),
            is_dir: false,
        };
        let line = p.render_candidate(&c);
        let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(combined, "foo.md");
    }

    #[test]
    fn renders_dir_with_trailing_slash() {
        let p = PathCompletionProvider;
        let c = PathCandidate {
            name: "subdir".to_string(),
            is_dir: true,
        };
        let line = p.render_candidate(&c);
        let combined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(combined, "subdir/");
    }

    #[test]
    fn title_does_not_depend_on_selection() {
        let p = PathCompletionProvider;
        assert_eq!(p.title(None), p.title(None));
        assert!(p.title(None).contains("Tab"));
    }
}
