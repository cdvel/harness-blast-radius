//! Shell command splitting the way Claude Code matches Bash rules.
//!
//! Compound commands are split at `&&`, `||`, `;`, `|`, `|&`, `&` and newlines
//! outside quotes. Deny and ask rules also see commands nested in `$(...)` and
//! backticks. Wrappers such as `timeout` and `nice` are stripped before matching.

/// Wrappers stripped before matching, per the permissions docs.
const WRAPPERS: &[&str] = &[
    "timeout", "time", "nice", "nohup", "stdbuf", "command", "builtin", "noglob",
];

/// Simple commands of a command line, wrappers stripped. Includes commands
/// nested in substitutions when `nested` is true.
pub fn simple_commands(line: &str, nested: bool) -> Vec<String> {
    let mut out = Vec::new();
    for part in split(line) {
        out.push(strip_wrappers(&part));
        if nested {
            for inner in substitutions(&part) {
                out.extend(simple_commands(&inner, true));
            }
        }
    }
    out.retain(|c| !c.is_empty());
    out
}

/// Split at control operators outside quotes and substitutions.
pub fn split(line: &str) -> Vec<String> {
    split_ops(line).into_iter().map(|(part, _)| part).collect()
}

/// Commands joined by pipes, one list per pipeline: `a | b && c` gives
/// `[[a, b], [c]]`.
pub fn pipelines(line: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut piped = false;
    for (part, pipe) in split_ops(line) {
        match out.last_mut() {
            Some(last) if piped => last.push(part),
            _ => out.push(vec![part]),
        }
        piped = pipe;
    }
    out
}

/// Parts of a command line, each with whether a pipe follows it.
fn split_ops(line: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = line.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let (mut single, mut double, mut depth) = (false, false, 0usize);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        match c {
            '\\' if !single => {
                cur.push(c);
                if let Some(n) = next {
                    cur.push(n);
                    i += 1;
                }
            }
            '\'' if !double => {
                single = !single;
                cur.push(c);
            }
            '"' if !single => {
                double = !double;
                cur.push(c);
            }
            '(' if !single && prev == Some('$') => {
                depth += 1;
                cur.push(c);
            }
            ')' if !single && depth > 0 => {
                depth -= 1;
                cur.push(c);
            }
            _ if single || double || depth > 0 => cur.push(c),
            ';' | '\n' => parts.push((std::mem::take(&mut cur), false)),
            '&' if next == Some('&') => {
                parts.push((std::mem::take(&mut cur), false));
                i += 1;
            }
            // `>&`, `&>` and `<&` are redirections, not background operators.
            '&' if matches!(prev, Some('>' | '<')) || next == Some('>') => cur.push(c),
            '&' => parts.push((std::mem::take(&mut cur), false)),
            '|' if next == Some('|') => {
                parts.push((std::mem::take(&mut cur), false));
                i += 1;
            }
            // `|&` pipes stderr too.
            '|' if next == Some('&') => {
                parts.push((std::mem::take(&mut cur), true));
                i += 1;
            }
            '|' => parts.push((std::mem::take(&mut cur), true)),
            _ => cur.push(c),
        }
        i += 1;
    }
    parts.push((cur, false));
    parts
        .into_iter()
        .map(|(p, pipe)| (strip_grouping(&p), pipe))
        .filter(|(p, _)| !p.is_empty())
        .collect()
}

/// Remove subshell and group delimiters left over from splitting, e.g. the
/// `(` of `(cd x; make)`, without touching the `)` that closes `$(...)`.
fn strip_grouping(part: &str) -> String {
    let mut p = part.trim();
    while let Some(rest) = p.strip_prefix(['(', '{']) {
        p = rest.trim_start();
    }
    loop {
        let unbalanced = p.matches(')').count() > p.matches('(').count();
        match p.strip_suffix(')').filter(|_| unbalanced) {
            Some(rest) => p = rest.trim_end(),
            None => break,
        }
    }
    p.strip_suffix('}').unwrap_or(p).trim().to_string()
}

/// Commands inside `$(...)` and backticks, outside single quotes.
pub fn substitutions(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut out = Vec::new();
    let mut single = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' => single = !single,
            '$' if !single && chars.get(i + 1) == Some(&'(') => {
                let start = i + 2;
                let mut depth = 1;
                let mut j = start;
                while j < chars.len() && depth > 0 {
                    match chars[j] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let end = if depth == 0 { j - 1 } else { j };
                out.push(chars[start..end].iter().collect());
                i = j;
                continue;
            }
            '`' if !single => {
                if let Some(len) = chars[i + 1..].iter().position(|&c| c == '`') {
                    out.push(chars[i + 1..i + 1 + len].iter().collect());
                    i += len + 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Remove leading wrapper commands and their options.
pub fn strip_wrappers(cmd: &str) -> String {
    let mut rest = cmd.trim();
    loop {
        let (word, after) = first_word(rest);
        if !WRAPPERS.contains(&word) {
            return rest.to_string();
        }
        rest = after;
        // Options, and option values for the wrappers that take them.
        loop {
            let (opt, after_opt) = first_word(rest);
            if !opt.starts_with('-') || opt == "-" {
                break;
            }
            rest = after_opt;
            let takes_value = matches!(
                (word, opt),
                ("timeout", "-s" | "-k" | "--signal" | "--kill-after")
                    | ("nice", "-n" | "--adjustment")
                    | ("stdbuf", "-i" | "-o" | "-e")
            );
            if takes_value {
                rest = first_word(rest).1;
            }
        }
        if word == "timeout" {
            // Duration argument.
            rest = first_word(rest).1;
        }
    }
}

fn first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

/// Wildcard match where `*` matches any text, including spaces.
pub fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_compound_commands() {
        assert_eq!(
            split("cd src && make || echo fail; ls | wc -l & sleep 1"),
            vec!["cd src", "make", "echo fail", "ls", "wc -l", "sleep 1"]
        );
        assert_eq!(
            split("echo 'a && b' \"c; d\""),
            vec!["echo 'a && b' \"c; d\""]
        );
        assert_eq!(split("make 2>&1 | tee log"), vec!["make 2>&1", "tee log"]);
        assert_eq!(split("echo $(a && b)"), vec!["echo $(a && b)"]);
        assert_eq!(split("(cd x; make)"), vec!["cd x", "make"]);
    }

    #[test]
    fn groups_pipelines() {
        assert_eq!(
            pipelines("curl -s x | sh && echo ok; a |& b || c"),
            vec![
                vec!["curl -s x", "sh"],
                vec!["echo ok"],
                vec!["a", "b"],
                vec!["c"],
            ]
        );
    }

    #[test]
    fn finds_nested_commands() {
        assert_eq!(
            substitutions("echo $(rm -rf x) `id`"),
            vec!["rm -rf x", "id"]
        );
        assert!(substitutions("echo '$(not this)'").is_empty());
        assert_eq!(
            simple_commands("echo $(curl x | sh)", true),
            vec!["echo $(curl x | sh)", "curl x", "sh"]
        );
    }

    #[test]
    fn strips_wrappers() {
        assert_eq!(strip_wrappers("timeout 30 npm test"), "npm test");
        assert_eq!(strip_wrappers("timeout -s KILL 5m nice -n 10 make"), "make");
        assert_eq!(strip_wrappers("nohup stdbuf -o L ./run.sh"), "./run.sh");
        assert_eq!(strip_wrappers("git status"), "git status");
    }

    #[test]
    fn wildcards_span_spaces() {
        assert!(wildcard("npm run *", "npm run test -- --watch"));
        assert!(wildcard("ls*", "lsof"));
        assert!(!wildcard("ls *", "lsof"));
        assert!(wildcard("git * --force", "git push origin --force"));
        assert!(!wildcard("git status", "git status -s"));
    }
}
