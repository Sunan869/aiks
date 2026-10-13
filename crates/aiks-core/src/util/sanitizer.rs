use regex::Regex;
use std::sync::OnceLock;

/// Sanitizes secrets from text content before storing or sending externally.
///
/// Three enforced boundaries:
/// 1. Persist Boundary: before writing to SQLite
/// 2. External Send Boundary: before sending to AI/Embedding API
/// 3. Log Boundary: before writing to log files
///
/// Supported patterns:
/// - Bearer tokens, Authorization headers
/// - API keys (OpenAI, Anthropic, Google, AWS, etc.)
/// - password/token/secret/api_key in various formats:
///   - JSON: "key": "value"
///   - key=value, key = value
///   - env: KEY=VALUE
///   - SecretKey, AccessKey variants
///   - Quoted values: key="value" or key='value'
pub struct SecretSanitizer {
    patterns: Vec<(Regex, &'static str)>,
    custom_patterns: Vec<Regex>,
}

impl Default for SecretSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretSanitizer {
    pub fn new() -> Self {
        let mut patterns: Vec<(Regex, &'static str)> = Vec::new();

        // ── Bearer / Authorization ─────────────────────────────────────────────
        let bearer_patterns = [
            (r"(?i)(Bearer\s+)[A-Za-z0-9\-._~+/]+=*", "Bearer [REDACTED]"),
            (
                r"(?i)(Authorization:\s*(?:Bearer|Basic|Token)\s+)[^\s\r\n,;]{8,}",
                "Authorization: [REDACTED]",
            ),
        ];
        for (pat, rep) in bearer_patterns {
            if let Ok(re) = Regex::new(pat) {
                patterns.push((re, rep));
            }
        }

        // ── Well-known key formats ─────────────────────────────────────────────
        let key_patterns = [
            (r"\bsk-[A-Za-z0-9\-_]{20,}\b", "[REDACTED_API_KEY]"),
            (r"\bsk-ant-[A-Za-z0-9\-_]{20,}\b", "[REDACTED_API_KEY]"),
            (r"\bAIza[0-9A-Za-z\-_]{35}\b", "[REDACTED_API_KEY]"),
            (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED_AWS_KEY]"),
        ];
        for (pat, rep) in key_patterns {
            if let Ok(re) = Regex::new(pat) {
                patterns.push((re, rep));
            }
        }

        // ── JSON "key": "value" ────────────────────────────────────────────────
        // Covers: api_key, token, secret, password, access_token, client_secret,
        //         private_key, SecretKey, AccessKey, access_key, secret_key
        if let Ok(re) = Regex::new(
            r#"(?i)"(?:api[_-]?key|token|secret(?:[_-]?key)?|password|access[_-]?(?:key|token)|client[_-]?secret|private[_-]?key|secret[_-]?access[_-]?key)"\s*:\s*"[^"]{4,}""#,
        ) {
            patterns.push((re, r#""[SECRET_KEY]": "[REDACTED]""#));
        }

        // ── key = value (with optional spaces and quotes) ───────────────────────
        // Matches: token=abc, password='abc', SecretKey = "abc", token = abc
        // Also: AWS_SECRET_ACCESS_KEY=abc, AWS_ACCESS_KEY_ID=abc
        if let Ok(re) = Regex::new(
            r#"(?i)(?:^|\b)(?:AWS_SECRET_ACCESS_KEY|AWS_ACCESS_KEY_ID|API_KEY|ACCESS_KEY|SECRET_KEY|SECRET_ACCESS_KEY|PRIVATE_KEY|token|password|secret|api[_-]key|access[_-]key|secret[_-]key|SecretKey|AccessKey)\s*[=:]\s*['"]?([A-Za-z0-9\-_+/=.@!$%^&*#]{4,})['"]?"#,
        ) {
            patterns.push((re, "[SECRET_KEY]=[REDACTED]"));
        }

        // ── Bare env-style: KEY=VALUE on its own line ─────────────────────────
        if let Ok(re) = Regex::new(
            r"(?im)^(?:AWS_SECRET_ACCESS_KEY|AWS_ACCESS_KEY_ID|API_KEY|ACCESS_KEY|SECRET_KEY)=([A-Za-z0-9\-_+/=.]{8,})\s*$",
        ) {
            patterns.push((re, "[SECRET_KEY]=[REDACTED]"));
        }

        Self {
            patterns,
            custom_patterns: Vec::new(),
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> anyhow::Result<()> {
        let re = Regex::new(pattern)?;
        self.custom_patterns.push(re);
        Ok(())
    }

    pub fn sanitize(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (re, replacement) in &self.patterns {
            result = re.replace_all(&result, *replacement).to_string();
        }
        for re in &self.custom_patterns {
            result = re.replace_all(&result, "[REDACTED_CUSTOM]").to_string();
        }
        result
    }

    pub fn has_secrets(&self, text: &str) -> bool {
        self.patterns.iter().any(|(re, _)| re.is_match(text))
            || self.custom_patterns.iter().any(|re| re.is_match(text))
    }
}

static DEFAULT_SANITIZER: OnceLock<SecretSanitizer> = OnceLock::new();
pub fn default_sanitizer() -> &'static SecretSanitizer {
    DEFAULT_SANITIZER.get_or_init(SecretSanitizer::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bearer_token() {
        let s = SecretSanitizer::new();
        let result = s.sanitize("curl -H 'Authorization: Bearer eyJhbGciOiJSUzI1NiJ9.payload.sig' https://api.example.com");
        assert!(!result.contains("eyJhbGciOiJSUzI1NiJ9"));
    }

    #[test]
    fn sanitizes_aws_secret() {
        let s = SecretSanitizer::new();
        for input in [
            "AWS_SECRET_ACCESS_KEY=AUDIT_ONLY_123456",
            "AWS_SECRET_ACCESS_KEY = AUDIT_ONLY_123456",
        ] {
            let out = s.sanitize(input);
            assert!(!out.contains("AUDIT_ONLY_123456"), "Failed on: {input}");
        }
    }

    #[test]
    fn sanitizes_secret_key_variants() {
        let s = SecretSanitizer::new();
        for input in [
            "SecretKey=AUDIT_ONLY_123456",
            "secret_key=AUDIT_ONLY_123456",
            "SecretKey = \"AUDIT_ONLY_123456\"",
        ] {
            let out = s.sanitize(input);
            assert!(!out.contains("AUDIT_ONLY_123456"), "Failed on: {input}");
        }
    }

    #[test]
    fn sanitizes_token_patterns() {
        let s = SecretSanitizer::new();
        for input in [
            "token = AUDIT_ONLY_123456",
            "password=\"AUDIT_ONLY_123456\"",
            "token=AUDIT_ONLY_123456",
        ] {
            let out = s.sanitize(input);
            assert!(!out.contains("AUDIT_ONLY_123456"), "Failed on: {input}");
        }
    }

    #[test]
    fn sanitizes_json_password() {
        let s = SecretSanitizer::new();
        let result = s.sanitize(r#"{"password": "mysecretpassword123", "user": "admin"}"#);
        assert!(!result.contains("mysecretpassword123"));
    }

    #[test]
    fn preserves_normal_text() {
        let s = SecretSanitizer::new();
        let text = "Hello, this is normal text without secrets.";
        assert_eq!(s.sanitize(text), text);
    }

    #[test]
    fn sanitizes_aws_access_key() {
        let s = SecretSanitizer::new();
        let result = s.sanitize("AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE");
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn custom_pattern_works() {
        let mut s = SecretSanitizer::new();
        s.add_pattern(r"MY_SECRET_[A-Z0-9]+").unwrap();
        let result = s.sanitize("config: MY_SECRET_ABC123XYZ");
        assert!(!result.contains("MY_SECRET_ABC123XYZ"));
    }
}
