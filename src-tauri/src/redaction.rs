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

pub fn redact(input: &str) -> String {
    let value = BEARER.replace_all(input, "${1}[REDACTED]");
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
}
