pub fn strip_notion_noise(content: &str) -> String {
    let unspanned = strip_span_tags(content);
    strip_trailing_attrs(&unspanned)
}

fn strip_span_tags(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(open_at) = rest.find("<span") {
        out.push_str(&rest[..open_at]);
        let after_open_kw = &rest[open_at..];
        let Some(open_end) = after_open_kw.find('>') else {
            out.push_str(after_open_kw);
            return out;
        };
        let after_open = &after_open_kw[open_end + 1..];
        let Some(close_at) = after_open.find("</span>") else {
            out.push_str(&after_open_kw[..=open_end]);
            rest = after_open;
            continue;
        };
        let inner = &after_open[..close_at];
        out.push_str(inner);
        rest = &after_open[close_at + "</span>".len()..];
    }
    out.push_str(rest);
    out
}

fn strip_trailing_attrs(content: &str) -> String {
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| strip_trailing_attr_from_line(line).to_owned())
        .collect();
    if content.ends_with('\n') {
        lines.push(String::new());
    }
    lines.join("\n")
}

fn strip_trailing_attr_from_line(line: &str) -> &str {
    let trimmed = line.trim_end();
    if !trimmed.ends_with('}') {
        return line;
    }
    let Some(open_idx) = trimmed.rfind('{') else {
        return line;
    };
    let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
    if !is_attribute_block(inner) {
        return line;
    }
    trimmed[..open_idx].trim_end()
}

fn is_attribute_block(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.split_whitespace().all(|chunk| {
        let Some(eq_idx) = chunk.find('=') else {
            return false;
        };
        let key = &chunk[..eq_idx];
        let value = &chunk[eq_idx + 1..];
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            && value.len() >= 2
            && value.starts_with('"')
            && value.ends_with('"')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_simple_span() {
        assert_eq!(
            strip_notion_noise("<span underline=\"true\">Saturday</span>"),
            "Saturday"
        );
    }

    #[test]
    fn strips_trailing_attr_block() {
        assert_eq!(
            strip_notion_noise("### Heading {toggle=\"true\"}"),
            "### Heading"
        );
    }

    #[test]
    fn unwraps_span_and_strips_attr_on_same_line() {
        assert_eq!(
            strip_notion_noise(
                "### <span underline=\"true\">[Saturday](/url)</span> {toggle=\"true\"}"
            ),
            "### [Saturday](/url)"
        );
    }

    #[test]
    fn leaves_plain_markdown_unchanged() {
        let input = "# Title\n\n- bullet\n- another\n\n[link](url)";
        assert_eq!(strip_notion_noise(input), input);
    }

    #[test]
    fn does_not_strip_brace_block_with_non_attribute_content() {
        let input = "this has {free-form text} not attrs";
        assert_eq!(strip_notion_noise(input), input);
    }

    #[test]
    fn multiple_spans_on_same_line() {
        assert_eq!(
            strip_notion_noise("<span a=\"1\">one</span> and <span b=\"2\">two</span>"),
            "one and two"
        );
    }

    #[test]
    fn multiple_attrs_in_one_block_stripped() {
        assert_eq!(strip_notion_noise("Heading {a=\"1\" b=\"2\"}"), "Heading");
    }

    #[test]
    fn malformed_unclosed_span_leaves_content() {
        let input = "<span underline=\"true\">never closes";
        assert_eq!(strip_notion_noise(input), input);
    }

    #[test]
    fn malformed_unclosed_open_tag_passthrough() {
        let input = "<span underline=\"true broken";
        assert_eq!(strip_notion_noise(input), input);
    }

    #[test]
    fn preserves_trailing_newline() {
        assert_eq!(
            strip_notion_noise("Heading {toggle=\"true\"}\n"),
            "Heading\n"
        );
    }

    #[test]
    fn multiline_with_mixed_content() {
        let input = "## [Friday](/url)\n### <span underline=\"true\">[Sat](/url)</span> {toggle=\"true\"}\n- plain bullet";
        let expected = "## [Friday](/url)\n### [Sat](/url)\n- plain bullet";
        assert_eq!(strip_notion_noise(input), expected);
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(strip_notion_noise(""), "");
    }

    #[test]
    fn whitespace_before_attr_block_stripped() {
        assert_eq!(
            strip_notion_noise("Heading      {toggle=\"true\"}"),
            "Heading"
        );
    }

    #[test]
    fn span_inside_paragraph_text() {
        assert_eq!(
            strip_notion_noise("foo <span class=\"x\">bar</span> baz"),
            "foo bar baz"
        );
    }
}
