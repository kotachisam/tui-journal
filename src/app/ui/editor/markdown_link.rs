#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedLink {
    pub char_range: std::ops::Range<usize>,
    pub text: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocLink {
    pub line_idx: usize,
    pub char_range: std::ops::Range<usize>,
    pub text: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedLink {
    pub line_idx: usize,
    pub text_char_range: std::ops::Range<usize>,
    pub text: String,
    pub url: String,
}

pub fn parse_markdown_links_in_line(line: &str) -> Vec<ParsedLink> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        let Some((text_end, text)) = consume_bracketed(&chars, i + 1, ']') else {
            i += 1;
            continue;
        };
        if text_end + 1 >= chars.len() || chars[text_end + 1] != '(' {
            i += 1;
            continue;
        }
        let Some((url_end, url)) = consume_bracketed(&chars, text_end + 2, ')') else {
            i += 1;
            continue;
        };
        if text.is_empty() || url.is_empty() {
            i += 1;
            continue;
        }
        out.push(ParsedLink {
            char_range: i..url_end + 1,
            text,
            url,
        });
        i = url_end + 1;
    }
    out
}

fn consume_bracketed(chars: &[char], start: usize, close: char) -> Option<(usize, String)> {
    let mut depth: usize = 0;
    let mut buf = String::new();
    let mut k = start;
    while k < chars.len() {
        let c = chars[k];
        if c == '\n' {
            return None;
        }
        if c == close && depth == 0 {
            return Some((k, buf));
        }
        match c {
            '[' if close == ']' => depth += 1,
            ']' if close == ']' && depth > 0 => depth -= 1,
            '(' if close == ')' => depth += 1,
            ')' if close == ')' && depth > 0 => depth -= 1,
            _ => {}
        }
        buf.push(c);
        k += 1;
    }
    None
}

pub fn parse_markdown_links_in_doc(content: &str) -> Vec<DocLink> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (line_idx, line) in content.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for parsed in parse_markdown_links_in_line(line) {
            out.push(DocLink {
                line_idx,
                char_range: parsed.char_range,
                text: parsed.text,
                url: parsed.url,
            });
        }
    }
    out
}

pub fn substitute_markdown_links(content: &str) -> (String, Vec<RenderedLink>) {
    let doc_links = parse_markdown_links_in_doc(content);
    if doc_links.is_empty() {
        return (content.to_string(), Vec::new());
    }

    let mut by_line: std::collections::BTreeMap<usize, Vec<&DocLink>> =
        std::collections::BTreeMap::new();
    for link in &doc_links {
        by_line.entry(link.line_idx).or_default().push(link);
    }

    let mut rendered_lines: Vec<String> = Vec::new();
    let mut rendered_links: Vec<RenderedLink> = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let Some(links) = by_line.get(&line_idx) else {
            rendered_lines.push(line.to_string());
            continue;
        };
        let chars: Vec<char> = line.chars().collect();
        let mut new_line = String::new();
        let mut cursor: usize = 0;
        let mut new_char_pos: usize = 0;
        for link in links {
            let pre: String = chars[cursor..link.char_range.start].iter().collect();
            new_line.push_str(&pre);
            new_char_pos += link.char_range.start - cursor;

            let text_char_count = link.text.chars().count();
            let text_start = new_char_pos;
            new_line.push_str(&link.text);
            new_char_pos += text_char_count;

            rendered_links.push(RenderedLink {
                line_idx,
                text_char_range: text_start..text_start + text_char_count,
                text: link.text.clone(),
                url: link.url.clone(),
            });
            cursor = link.char_range.end;
        }
        let tail: String = chars[cursor..].iter().collect();
        new_line.push_str(&tail);
        rendered_lines.push(new_line);
    }

    let mut rendered_content = rendered_lines.join("\n");
    if content.ends_with('\n') {
        rendered_content.push('\n');
    }
    (rendered_content, rendered_links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_link() {
        let result = parse_markdown_links_in_line("see [Saturday](/url) here");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Saturday");
        assert_eq!(result[0].url, "/url");
        assert_eq!(result[0].char_range, 4..20);
    }

    #[test]
    fn parses_two_links_in_line() {
        let result = parse_markdown_links_in_line("[a](/x) and [b](/y)");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "a");
        assert_eq!(result[1].text, "b");
    }

    #[test]
    fn skips_brackets_without_url() {
        let result = parse_markdown_links_in_line("[just brackets] and stuff");
        assert!(result.is_empty());
    }

    #[test]
    fn skips_empty_text() {
        let result = parse_markdown_links_in_line("[](url)");
        assert!(result.is_empty());
    }

    #[test]
    fn skips_empty_url() {
        let result = parse_markdown_links_in_line("[text]()");
        assert!(result.is_empty());
    }

    #[test]
    fn allows_nested_parens_in_url() {
        let result = parse_markdown_links_in_line("[t](http://ex.com/(p)/page)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "http://ex.com/(p)/page");
    }

    #[test]
    fn handles_link_with_long_text_and_url() {
        let line = "    - [Eventually got to OSS about 2230 but left at 2345 to go pick her up from the station.](/20765ee7b1598081bd3cf111caff8013?pvs=25#20765ee7b15980a4afccd71f32de144c)";
        let result = parse_markdown_links_in_line(line);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].text,
            "Eventually got to OSS about 2230 but left at 2345 to go pick her up from the station."
        );
    }

    #[test]
    fn substitute_replaces_link_with_text_only() {
        let (out, links) = substitute_markdown_links("see [Saturday](/url) here");
        assert_eq!(out, "see Saturday here");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text_char_range, 4..12);
        assert_eq!(links[0].url, "/url");
    }

    #[test]
    fn substitute_handles_multiple_links_per_line() {
        let (out, links) = substitute_markdown_links("[a](/x) and [bee](/y)");
        assert_eq!(out, "a and bee");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].text_char_range, 0..1);
        assert_eq!(links[1].text_char_range, 6..9);
    }

    #[test]
    fn substitute_preserves_non_link_lines() {
        let input = "no link here\n[link](/url)\nplain again";
        let (out, _) = substitute_markdown_links(input);
        assert_eq!(out, "no link here\nlink\nplain again");
    }

    #[test]
    fn substitute_skips_links_in_code_fences() {
        let input = "outside [a](/x)\n```\n[b](/y)\n```\nafter [c](/z)";
        let (out, links) = substitute_markdown_links(input);
        assert_eq!(out, "outside a\n```\n[b](/y)\n```\nafter c");
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn substitute_preserves_trailing_newline() {
        let (out, _) = substitute_markdown_links("[link](/url)\n");
        assert_eq!(out, "link\n");
    }

    #[test]
    fn substitute_no_links_returns_input_verbatim() {
        let (out, links) = substitute_markdown_links("plain text only");
        assert_eq!(out, "plain text only");
        assert!(links.is_empty());
    }
}
