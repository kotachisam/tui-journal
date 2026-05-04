use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathCandidate {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PathContext {
    pub parent_dir: PathBuf,
    pub query: String,
}

pub fn parse_path_context(input: &str, home_dir: Option<&Path>, cwd: &Path) -> Option<PathContext> {
    if input.is_empty() {
        return None;
    }

    let last_sep = input.rfind(['/', '\\']);

    let (parent_text, query) = match last_sep {
        Some(idx) => (&input[..=idx], &input[idx + 1..]),
        None => ("", input),
    };

    let parent_path = resolve_parent(parent_text, home_dir, cwd);

    Some(PathContext {
        parent_dir: parent_path,
        query: query.to_string(),
    })
}

fn resolve_parent(parent_text: &str, home_dir: Option<&Path>, cwd: &Path) -> PathBuf {
    if parent_text.is_empty() {
        return cwd.to_path_buf();
    }
    if let Some(home) = home_dir {
        if parent_text == "~/" || parent_text == "~" {
            return home.to_path_buf();
        }
        if let Some(rest) = parent_text.strip_prefix("~/") {
            return home.join(rest);
        }
    }
    PathBuf::from(parent_text)
}

pub fn list_directory(parent: &Path, query: &str, max: usize) -> Vec<PathCandidate> {
    let show_hidden = query.starts_with('.');

    let entries = match std::fs::read_dir(parent) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut candidates: Vec<PathCandidate> = entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            if !name.starts_with(query) {
                return None;
            }
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            Some(PathCandidate { name, is_dir })
        })
        .collect();

    candidates.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    candidates.truncate(max);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_dir_with(entries: &[(&str, bool)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, is_dir) in entries {
            let p = dir.path().join(name);
            if *is_dir {
                fs::create_dir(&p).unwrap();
            } else {
                fs::write(&p, "").unwrap();
            }
        }
        dir
    }

    #[test]
    fn lists_files_and_dirs() {
        let dir = make_dir_with(&[("file.txt", false), ("subdir", true)]);
        let mut result = list_directory(dir.path(), "", 50);
        result.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn dirs_sorted_before_files() {
        let dir = make_dir_with(&[
            ("zfile.txt", false),
            ("adir", true),
            ("bfile.md", false),
            ("cdir", true),
        ]);
        let result = list_directory(dir.path(), "", 50);
        assert_eq!(result[0].name, "adir");
        assert_eq!(result[1].name, "cdir");
        assert!(result[0].is_dir);
        assert!(result[1].is_dir);
        assert_eq!(result[2].name, "bfile.md");
        assert_eq!(result[3].name, "zfile.txt");
    }

    #[test]
    fn alphabetical_within_groups() {
        let dir = make_dir_with(&[("zebra.txt", false), ("apple.txt", false), ("mango.txt", false)]);
        let result = list_directory(dir.path(), "", 50);
        assert_eq!(result[0].name, "apple.txt");
        assert_eq!(result[1].name, "mango.txt");
        assert_eq!(result[2].name, "zebra.txt");
    }

    #[test]
    fn prefix_filtering_matches_only_starting() {
        let dir = make_dir_with(&[("foo.md", false), ("bar.md", false), ("foobar.md", false)]);
        let result = list_directory(dir.path(), "foo", 50);
        let names: Vec<&str> = result.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["foo.md", "foobar.md"]);
    }

    #[test]
    fn hides_dotfiles_by_default() {
        let dir = make_dir_with(&[(".hidden", false), ("visible.md", false)]);
        let result = list_directory(dir.path(), "", 50);
        let names: Vec<&str> = result.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["visible.md"]);
    }

    #[test]
    fn shows_dotfiles_when_query_starts_with_dot() {
        let dir = make_dir_with(&[(".hidden", false), (".config", true), ("visible.md", false)]);
        let result = list_directory(dir.path(), ".", 50);
        let names: Vec<&str> = result.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec![".config", ".hidden"]);
    }

    #[test]
    fn caps_at_max_candidates() {
        let entries: Vec<(String, bool)> = (0..100).map(|i| (format!("file_{i:03}.md"), false)).collect();
        let entries_ref: Vec<(&str, bool)> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let dir = make_dir_with(&entries_ref);
        let result = list_directory(dir.path(), "", 10);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn nonexistent_parent_returns_empty() {
        let result = list_directory(Path::new("/this/should/not/exist/anywhere/420"), "", 50);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_context_relative_no_slash() {
        let cwd = Path::new("/tmp/work");
        let ctx = parse_path_context("foo", None, cwd).unwrap();
        assert_eq!(ctx.parent_dir, PathBuf::from("/tmp/work"));
        assert_eq!(ctx.query, "foo");
    }

    #[test]
    fn parse_context_absolute_with_query() {
        let cwd = Path::new("/tmp/work");
        let ctx = parse_path_context("/Users/sam_r/Doc", None, cwd).unwrap();
        assert_eq!(ctx.parent_dir, PathBuf::from("/Users/sam_r/"));
        assert_eq!(ctx.query, "Doc");
    }

    #[test]
    fn parse_context_trailing_slash_empty_query() {
        let cwd = Path::new("/tmp/work");
        let ctx = parse_path_context("/Users/sam_r/", None, cwd).unwrap();
        assert_eq!(ctx.parent_dir, PathBuf::from("/Users/sam_r/"));
        assert_eq!(ctx.query, "");
    }

    #[test]
    fn parse_context_tilde_expansion() {
        let cwd = Path::new("/tmp/work");
        let home = Path::new("/Users/sam_r");
        let ctx = parse_path_context("~/Doc", Some(home), cwd).unwrap();
        assert_eq!(ctx.parent_dir, PathBuf::from("/Users/sam_r"));
        assert_eq!(ctx.query, "Doc");
    }

    #[test]
    fn parse_context_tilde_only_with_slash() {
        let cwd = Path::new("/tmp/work");
        let home = Path::new("/Users/sam_r");
        let ctx = parse_path_context("~/", Some(home), cwd).unwrap();
        assert_eq!(ctx.parent_dir, PathBuf::from("/Users/sam_r"));
        assert_eq!(ctx.query, "");
    }

    #[test]
    fn parse_context_empty_input_returns_none() {
        let cwd = Path::new("/tmp");
        assert!(parse_path_context("", None, cwd).is_none());
    }
}
