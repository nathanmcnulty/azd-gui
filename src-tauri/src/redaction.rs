use once_cell::sync::Lazy;
use regex::Regex;

static BEARER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(authorization\s*[:=]\s*bearer\s+)[^\s,;]+").unwrap());
static JWT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b").unwrap()
});
static SECRET_ASSIGNMENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|WEBHOOK|CLIENT_SECRET)[A-Z0-9_]*\s*[:=]\s*)("[^"]*"|'[^']*'|[^\s,;]+)"#).unwrap()
});
static ANSI_OSC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)").unwrap());
static ANSI_ESCAPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").unwrap());

pub fn redact(input: &str) -> String {
    let value = ANSI_OSC.replace_all(input, "");
    let value = ANSI_ESCAPE.replace_all(&value, "");
    let value = BEARER.replace_all(&value, "${1}[REDACTED]");
    let value = JWT.replace_all(&value, "[REDACTED_TOKEN]");
    SECRET_ASSIGNMENT
        .replace_all(&value, "${1}[REDACTED]")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn removes_tokens_and_secrets() {
        assert_eq!(
            redact("Authorization: Bearer abc123"),
            "Authorization: Bearer [REDACTED]"
        );
        assert_eq!(
            redact("CLIENT_SECRET=very-secret"),
            "CLIENT_SECRET=[REDACTED]"
        );
        assert_eq!(
            redact("ordinary deployment output"),
            "ordinary deployment output"
        );
    }

    #[test]
    fn removes_terminal_formatting_without_removing_text() {
        assert_eq!(
            redact("\u{1b}[33;1mWARNING: Sign-in required.\u{1b}[0m"),
            "WARNING: Sign-in required."
        );
        assert_eq!(
            redact("Open \u{1b}]8;;https://example.com\u{7}documentation\u{1b}]8;;\u{7}"),
            "Open documentation"
        );
    }
}
