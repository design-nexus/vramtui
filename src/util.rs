pub fn bytes_to_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub fn human_mib(mib: u64) -> String {
    if mib >= 1024 {
        let gib = mib as f64 / 1024.0;
        if gib >= 10.0 {
            format!("{gib:.0}G")
        } else {
            format!("{gib:.1}G")
        }
    } else if mib == 0 {
        "0".into()
    } else {
        format!("{mib}M")
    }
}

pub fn human_bytes(bytes: u64) -> String {
    human_mib(bytes_to_mib(bytes))
}

pub fn is_unreachable(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("connect")
        || e.contains("connection")
        || e.contains("error sending request")
        || e.contains("timed out")
        || e.contains("timeout")
        || e.contains("refused")
        || e.contains("unreachable")
        || e.contains("dns error")
}

pub fn short_error(err: &str) -> String {
    if is_unreachable(err) {
        let e = err.to_ascii_lowercase();
        if e.contains("11434") || e.contains("ollama") {
            return "Ollama is not reachable".into();
        }
        if e.contains("1234") || e.contains("41343") || e.contains("lm studio") || e.contains("lms")
        {
            return "LM Studio is not reachable".into();
        }
        return "Server is not reachable".into();
    }
    let one = err.lines().next().unwrap_or(err);
    let one = one.split(" for url ").next().unwrap_or(one);
    truncate(one, 72)
}

pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

pub fn fuzzy_score(query: &str, name: &str) -> Option<u32> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let n = name.to_ascii_lowercase();
    if n == q {
        return Some(300);
    }
    if n.starts_with(&q) {
        return Some(200);
    }
    if n.contains(&q) {
        return Some(100);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mib_rounding() {
        assert_eq!(bytes_to_mib(7151067268), 6819);
        assert_eq!(human_mib(6819), "6.7G");
        assert_eq!(human_mib(16), "16M");
    }

    #[test]
    fn reqwest_url_errors_are_unreachable() {
        let err = "error sending request for url (http://127.0.0.1:11434/api/ps): timed out";
        assert!(is_unreachable(err));
        assert_eq!(short_error(err), "Ollama is not reachable");
    }
}
