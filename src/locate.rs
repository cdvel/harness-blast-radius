//! Line numbers for findings, for SARIF and JSON output.
//!
//! `serde_json` drops positions, so this parses the file again with a small
//! JSON reader that records the line each value starts on.

use std::path::Path;

enum Node {
    Object(Vec<(String, Located)>),
    Array(Vec<Located>),
    String(String),
    Other,
}

struct Located {
    line: usize,
    node: Node,
}

/// Line (1-based) of `key` in the JSON file at `path`. `key` is a dotted
/// settings path such as `permissions.allow` or `projects.<path>.mcpServers.x`,
/// where `<path>` matches any key. Inside that value, a string the finding's
/// `detail` contains narrows it to that element's line.
pub fn line(path: &Path, key: &str, detail: &str) -> Option<usize> {
    let text = std::fs::read_to_string(path).ok()?;
    let root = Parser::new(&text).parse()?;
    if key.is_empty() {
        return None;
    }
    let found = walk(&root, key)?;
    let narrowed = best_string(found, detail).unwrap_or(found.line);
    Some(narrowed)
}

fn walk<'a>(at: &'a Located, key: &str) -> Option<&'a Located> {
    if key.is_empty() {
        return Some(at);
    }
    let Node::Object(entries) = &at.node else {
        return None;
    };
    if let Some(rest) = key.strip_prefix("<path>") {
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        return entries.iter().find_map(|(_, v)| walk(v, rest));
    }
    // Keys can contain dots (server names), so match whole keys, longest first.
    let mut matches: Vec<&(String, Located)> = entries
        .iter()
        .filter(|(k, _)| key == k || key.starts_with(&format!("{k}.")))
        .collect();
    matches.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    matches.into_iter().find_map(|(k, v)| {
        let rest = key[k.len()..].strip_prefix('.').unwrap_or("");
        walk(v, rest)
    })
}

/// Line of the longest string value under `at` that `detail` contains.
fn best_string(at: &Located, detail: &str) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    let mut stack = vec![at];
    while let Some(n) = stack.pop() {
        match &n.node {
            Node::String(s) if s.len() >= 3 && detail.contains(s.as_str()) => {
                if best.is_none_or(|(len, _)| s.len() > len) {
                    best = Some((s.len(), n.line));
                }
            }
            Node::Object(entries) => stack.extend(entries.iter().rev().map(|(_, v)| v)),
            Node::Array(items) => stack.extend(items.iter().rev()),
            _ => {}
        }
    }
    best.map(|(_, line)| line)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
            line: 1,
        }
    }

    fn parse(mut self) -> Option<Located> {
        let v = self.value()?;
        self.ws();
        (self.pos == self.bytes.len()).then_some(v)
    }

    fn ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'\n' => self.line += 1,
                b' ' | b'\t' | b'\r' => {}
                _ => break,
            }
            self.pos += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        self.ws();
        if self.bytes.get(self.pos) == Some(&b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Located> {
        self.ws();
        let line = self.line;
        let node = match *self.bytes.get(self.pos)? {
            b'{' => {
                self.pos += 1;
                let mut entries = Vec::new();
                if !self.eat(b'}') {
                    loop {
                        self.ws();
                        let key = self.string()?;
                        if !self.eat(b':') {
                            return None;
                        }
                        entries.push((key, self.value()?));
                        if self.eat(b'}') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                Node::Object(entries)
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                if !self.eat(b']') {
                    loop {
                        items.push(self.value()?);
                        if self.eat(b']') {
                            break;
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                Node::Array(items)
            }
            b'"' => Node::String(self.string()?),
            _ => {
                // Numbers, true, false, null: positions are all that matter.
                let start = self.pos;
                while let Some(&b) = self.bytes.get(self.pos) {
                    if matches!(b, b',' | b'}' | b']') || b.is_ascii_whitespace() {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    return None;
                }
                Node::Other
            }
        };
        Some(Located { line, node })
    }

    /// A JSON string at the current position, decoded with serde_json.
    fn string(&mut self) -> Option<String> {
        if self.bytes.get(self.pos) != Some(&b'"') {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        loop {
            match *self.bytes.get(self.pos)? {
                b'\\' => self.pos += 2,
                b'"' => break,
                b'\n' => return None,
                _ => self.pos += 1,
            }
        }
        self.pos += 1;
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        serde_json::from_str(raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_in(text: &str, key: &str, detail: &str) -> Option<usize> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, text).unwrap();
        line(&path, key, detail)
    }

    const SETTINGS: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(npm run *)",
      "Bash(python3 *)"
    ]
  },
  "mcpServers": {
    "a.b": { "url": "http://x" }
  },
  "projects": {
    "/home/u/p": {
      "mcpServers": { "tracker": {} }
    }
  }
}"#;

    #[test]
    fn finds_the_matching_array_element() {
        let detail = "Bash(python3 *) (applies after workspace trust)";
        assert_eq!(line_in(SETTINGS, "permissions.allow", detail), Some(5));
        assert_eq!(line_in(SETTINGS, "permissions.allow", "none"), Some(3));
    }

    #[test]
    fn dotted_names_and_wildcards() {
        assert_eq!(line_in(SETTINGS, "mcpServers.a.b", ""), Some(9));
        assert_eq!(
            line_in(SETTINGS, "projects.<path>.mcpServers.tracker", ""),
            Some(13)
        );
        assert_eq!(line_in(SETTINGS, "missing.key", ""), None);
    }

    #[test]
    fn invalid_json_has_no_lines() {
        assert_eq!(line_in("{\"a\": [1,],}", "a", ""), None);
    }
}
