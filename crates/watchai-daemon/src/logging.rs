use std::io::{self, Write};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

/// Check if a text segment contains markers indicative of sensitive information.
pub fn is_sensitive(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token=")
        || lower.contains("token:")
        || lower.contains("prompt=")
        || lower.contains("prompt:")
        || lower.contains("diff --git")
}

/// Redact sensitive data from a given text line or string.
/// Protects against accidental leakage of API keys, tokens, credentials, prompts, and code diffs.
pub fn redact_sensitive_text(input: &str) -> String {
    if !is_sensitive(input) {
        return input.to_string();
    }

    let mut result = input.to_string();

    // 1. Redact API keys: sk-ant-... or sk-...
    result = redact_api_keys(&result);

    // 2. Redact Bearer tokens: Bearer <token>
    result = redact_bearer_tokens(&result);

    // 3. Redact key-value secrets: token=..., password=..., secret=..., api_key=...
    result = redact_key_value_secrets(&result);

    // 4. Redact prompt payloads: prompt=..., prompt: ...
    result = redact_prompts(&result);

    // 5. Redact code diff lines: diff --git ...
    result = redact_code_diffs(&result);

    result
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn redact_api_keys(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.find("sk-") {
        out.push_str(&remaining[..pos]);
        let token_slice = &remaining[pos..];
        // Scan until end of token
        let token_len = token_slice
            .char_indices()
            .take_while(|(_, c)| is_token_char(*c))
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(3);

        // Only redact if token is of reasonable key length (> 8 chars)
        if token_len >= 8 {
            out.push_str("[REDACTED_API_KEY]");
        } else {
            out.push_str(&token_slice[..token_len]);
        }
        remaining = &token_slice[token_len..];
    }
    out.push_str(remaining);
    out
}

fn redact_bearer_tokens(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.to_lowercase().find("bearer ") {
        out.push_str(&remaining[..pos]);
        out.push_str("Bearer ");
        let token_slice = &remaining[pos + 7..];
        let token_len = token_slice
            .char_indices()
            .take_while(|(_, c)| is_token_char(*c))
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);

        out.push_str("[REDACTED_TOKEN]");
        remaining = &token_slice[token_len..];
    }
    out.push_str(remaining);
    out
}

fn redact_key_value(input: &str, keys: &[&str], replacement: &str) -> String {
    let mut result = input.to_string();

    for &key in keys {
        let mut new_result = String::with_capacity(result.len());
        let mut remaining = result.as_str();

        while let Some(idx) = remaining.to_lowercase().find(key) {
            let before = &remaining[..idx];
            let after_key = &remaining[idx + key.len()..];

            // Account for opening quote before key if present
            let (prefix, matched_key_with_quote) =
                if before.ends_with('"') || before.ends_with('\'') {
                    (&before[..before.len() - 1], true)
                } else {
                    (before, false)
                };

            // Check if key is preceded by word char (avoid mid-word false match)
            let prev_char = prefix.chars().last();
            if let Some(c) = prev_char {
                if c.is_alphanumeric() {
                    new_result.push_str(&remaining[..idx + key.len()]);
                    remaining = after_key;
                    continue;
                }
            }

            // Check after_key for optional closing quote, whitespace, separator (= or :)
            let mut s = after_key;
            if matched_key_with_quote {
                if let Some(stripped) = s.strip_prefix('"').or_else(|| s.strip_prefix('\'')) {
                    s = stripped;
                } else {
                    new_result.push_str(&remaining[..idx + key.len()]);
                    remaining = after_key;
                    continue;
                }
            }

            s = s.trim_start();

            if !s.starts_with('=') && !s.starts_with(':') {
                new_result.push_str(&remaining[..idx + key.len()]);
                remaining = after_key;
                continue;
            }

            let sep = &s[..1];
            let after_sep = &s[1..];
            let ws_len = after_sep.len() - after_sep.trim_start().len();
            let ws = &after_sep[..ws_len];
            s = &after_sep[ws_len..];

            new_result.push_str(prefix);
            if matched_key_with_quote {
                new_result.push('"');
            }
            new_result.push_str(&remaining[idx..idx + key.len()]);
            if matched_key_with_quote {
                new_result.push('"');
            }
            new_result.push_str(sep);
            new_result.push_str(ws);

            // Parse value
            let (quote, val_slice) = if let Some(stripped) = s.strip_prefix('"') {
                (Some('"'), stripped)
            } else if let Some(stripped) = s.strip_prefix('\'') {
                (Some('\''), stripped)
            } else {
                (None, s)
            };

            let val_len = if let Some(q) = quote {
                val_slice.find(q).unwrap_or(val_slice.len())
            } else {
                val_slice
                    .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '}')
                    .unwrap_or(val_slice.len())
            };

            new_result.push_str(replacement);

            if quote.is_some() && val_slice.len() > val_len {
                remaining = &val_slice[val_len + 1..];
            } else {
                remaining = &val_slice[val_len..];
            }
        }

        new_result.push_str(remaining);
        result = new_result;
    }

    result
}

fn redact_key_value_secrets(input: &str) -> String {
    const SECRET_KEYS: &[&str] = &["password", "secret", "api_key", "apikey", "token"];
    redact_key_value(input, SECRET_KEYS, "[REDACTED]")
}

fn redact_prompts(input: &str) -> String {
    const PROMPT_KEYS: &[&str] = &["prompt"];
    redact_key_value(input, PROMPT_KEYS, "[REDACTED_PROMPT]")
}

fn redact_code_diffs(input: &str) -> String {
    let mut lines = Vec::new();
    let mut in_diff = false;

    for line in input.lines() {
        if line.starts_with("diff --git ")
            || line.starts_with("--- a/")
            || line.starts_with("+++ b/")
            || line.starts_with("@@ ")
        {
            in_diff = true;
            lines.push("[REDACTED_CODE_DIFF]");
            continue;
        }

        if in_diff {
            if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
                continue;
            } else {
                in_diff = false;
            }
        }

        lines.push(line);
    }

    if input.ends_with('\n') {
        let mut joined = lines.join("\n");
        joined.push('\n');
        joined
    } else {
        lines.join("\n")
    }
}

/// A writer wrapper that automatically sanitizes and redacts all output passing through it.
pub struct RedactingWriter<W> {
    inner: W,
}

impl<W> RedactingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let redacted = redact_sensitive_text(&text);
        self.inner.write_all(redacted.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Factory constructing RedactingWriter instances for tracing-subscriber.
#[derive(Clone, Default)]
pub struct MakeRedactingWriter;

impl<'a> MakeWriter<'a> for MakeRedactingWriter {
    type Writer = RedactingWriter<io::Stdout>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter::new(io::stdout())
    }
}

/// Initialize global tracing subscriber with automated privacy redaction filters.
pub fn init_logging() {
    init_logging_with_filter("info,watchai_daemon=debug,watchai_adapters=debug");
}

/// Initialize tracing subscriber with explicit default filter string.
pub fn init_logging_with_filter(default_filter: &str) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(MakeRedactingWriter)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_api_keys() {
        let raw = "Config loaded: sk-ant-api03-abcdef1234567890_test for user";
        let cleaned = redact_sensitive_text(raw);
        assert!(!cleaned.contains("sk-ant-api03-abcdef1234567890_test"));
        assert!(cleaned.contains("[REDACTED_API_KEY]"));

        let raw_openai = "Connecting with sk-proj-1234567890abcdefghijklmnop";
        let cleaned_openai = redact_sensitive_text(raw_openai);
        assert!(!cleaned_openai.contains("sk-proj-1234567890abcdefghijklmnop"));
        assert!(cleaned_openai.contains("[REDACTED_API_KEY]"));
    }

    #[test]
    fn test_redact_bearer_tokens() {
        let raw = "HTTP Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let cleaned = redact_sensitive_text(raw);
        assert!(!cleaned.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(cleaned.contains("Bearer [REDACTED_TOKEN]"));
    }

    #[test]
    fn test_redact_key_value_secrets() {
        let raw1 = "Database config: password=supersecretpass and token=my-secret-token";
        let cleaned1 = redact_sensitive_text(raw1);
        assert!(!cleaned1.contains("supersecretpass"));
        assert!(!cleaned1.contains("my-secret-token"));
        assert!(cleaned1.contains("password=[REDACTED]"));
        assert!(cleaned1.contains("token=[REDACTED]"));

        let raw2 = "Auth payload: {\"api_key\": \"secret12345\"}";
        let cleaned2 = redact_sensitive_text(raw2);
        assert!(!cleaned2.contains("secret12345"));
        assert!(cleaned2.contains("api_key\": [REDACTED]"));
    }

    #[test]
    fn test_redact_prompts() {
        let raw = "User instruction: prompt=\"Write an exploit for the Linux kernel\"";
        let cleaned = redact_sensitive_text(raw);
        assert!(!cleaned.contains("Write an exploit for the Linux kernel"));
        assert!(cleaned.contains("prompt=[REDACTED_PROMPT]"));
    }

    #[test]
    fn test_redact_code_diffs() {
        let raw = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n-let x = 1;\n+let x = 2;\n";
        let cleaned = redact_sensitive_text(raw);
        assert!(!cleaned.contains("let x = 1"));
        assert!(!cleaned.contains("let x = 2"));
        assert!(cleaned.contains("[REDACTED_CODE_DIFF]"));
    }

    #[test]
    fn test_non_sensitive_operational_logs_preserved() {
        let raw = "Discovered surviving session on startup: provider=claude-code, id=proc-12345678, state=IDLE, active=1";
        let cleaned = redact_sensitive_text(raw);
        assert_eq!(raw, cleaned, "Operational metadata must not be corrupted");
    }

    #[test]
    fn test_redacting_writer_output() {
        let mut buf = Vec::new();
        let mut writer = RedactingWriter::new(&mut buf);
        let log_line = b"User logged in with token=abcde12345 secret=xyz9876\n";
        writer.write_all(log_line).unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(!output.contains("abcde12345"));
        assert!(!output.contains("xyz9876"));
        assert!(output.contains("token=[REDACTED]"));
        assert!(output.contains("secret=[REDACTED]"));
    }
}
