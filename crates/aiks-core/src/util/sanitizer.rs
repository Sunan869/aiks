use regex::Regex;
use std::sync::OnceLock;

/// Sanitizes secrets from text content before storing in knowledge base.
///
/// Replaces known secret patterns with "[REDACTED]".
/// Supported patterns by default:
/// - Bearer tokens
/// - API keys (OpenAI, Anthropic, Google, AWS, etc.)
/// - Authorization headers
/// - password/token/secret/api_key in key=value or JSON
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
        let mut patterns = Vec::new();

        // Bearer tokens: "Bearer <token>"
        if let Ok(re) = Regex::new(r"(?i)(Bearer\s+)[A-Za-z0-9\-._~+/]+=*") {
            patterns.push((re, "Bearer [REDACTED]"));
        }

        // Authorization header value: "Authorization: Bearer ..." or "Authorization: Basic ..."
        if let Ok(re) =
            Regex::new(r"(?i)(Authorization:\s*(?:Bearer|Basic|Token)\s+)[^\s\r\n,;]{8,}")
        {
            patterns.push((re, "Authorization: [REDACTED]"));
        }

        // OpenAI-style keys: sk-... (at least 20 chars)
        if let Ok(re) = Regex::new(r"\bsk-[A-Za-z0-9\-_]{20,}\b") {
            patterns.push((re, "[REDACTED_API_KEY]"));
        }

        // Anthropic keys: sk-ant-...
        if let Ok(re) = Regex::new(r"\bsk-ant-[A-Za-z0-9\-_]{20,}\b") {
            patterns.push((re, "[REDACTED_API_KEY]"));
        }

        // Google API keys: AIza...
        if let Ok(re) = Regex::new(r"\bAIza[0-9A-Za-z\-_]{35}\b") {
            patterns.push((re, "[REDACTED_API_KEY]"));
        }

        // AWS Access Key: AKIA...
        if let Ok(re) = Regex::new(r"\bAKIA[0-9A-Z]{16}\b") {
            patterns.push((re, "[REDACTED_AWS_KEY]"));
        }

        // JSON key-value: "api_key": "value", "token": "value", "secret": "value", "password": "value"
        if let Ok(re) = Regex::new(
            r#"(?i)"(?:api_key|token|secret|password|access_token|client_secret|private_key)"\s*:\s*"[^"]{4,}""#,
        ) {
            patterns.push((re, r#""[SECRET_KEY]": "[REDACTED]""#));
        }

        // key=value patterns: token=xxx, password=xxx, secret=xxx, api_key=xxx
        if let Ok(re) = Regex::new(
            r"(?i)\b(?:token|password|secret|api_key|access_key|private_key)=([A-Za-z0-9\-_+/=.]{8,})",
        ) {
            patterns.push((re, "[SECRET_KEY]=[REDACTED]"));
        }

        Self {
            patterns,
            custom_patterns: Vec::new(),
        }
    }

    /// Add a custom regex pattern. Matches will be replaced with "[REDACTED_CUSTOM]"
    pub fn add_pattern(&mut self, pattern: &str) -> anyhow::Result<()> {
        let re = Regex::new(pattern)?;
        self.custom_patterns.push(re);
        Ok(())
    }

    /// Sanitize a text string, replacing all known secret patterns.
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

    /// Returns true if the text contains any known secret patterns.
    pub fn has_secrets(&self, text: &str) -> bool {
        self.patterns.iter().any(|(re, _)| re.is_match(text))
            || self.custom_patterns.iter().any(|re| re.is_match(text))
    }
}

/// Global default sanitizer instance
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
        let text = "curl -H 'Authorization: Bearer eyJhbGciOiJSUzI1NiJ9.payload.signature' https://api.example.com";
        let result = s.sanitize(text);
        assert!(!result.contains("eyJhbGciOiJSUzI1NiJ9"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn sanitizes_openai_api_key() {
        let s = SecretSanitizer::new();
        let text = "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz1234567890ABCD";
        let result = s.sanitize(text);
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn sanitizes_json_password() {
        let s = SecretSanitizer::new();
        let text = r#"{"password": "mysecretpassword123", "user": "admin"}"#;
        let result = s.sanitize(text);
        assert!(!result.contains("mysecretpassword123"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn sanitizes_aws_access_key() {
        let s = SecretSanitizer::new();
        let text = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let result = s.sanitize(text);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result.contains("[REDACTED"));
    }

    #[test]
    fn preserves_non_secret_text() {
        let s = SecretSanitizer::new();
        let text = "Hello, this is normal text without any secrets.";
        let result = s.sanitize(text);
        assert_eq!(result, text);
    }

    #[test]
    fn has_secrets_detects_key() {
        let s = SecretSanitizer::new();
        assert!(s.has_secrets("Bearer eyJhbGciOiJSUzI1NiJ9.abc.def"));
        assert!(!s.has_secrets("Hello world"));
    }

    #[test]
    fn custom_pattern_works() {
        let mut s = SecretSanitizer::new();
        s.add_pattern(r"MY_SECRET_[A-Z0-9]+").unwrap();
        let text = "config: MY_SECRET_ABC123XYZ";
        let result = s.sanitize(text);
        assert!(!result.contains("MY_SECRET_ABC123XYZ"));
        assert!(result.contains("[REDACTED_CUSTOM]"));
    }
}
