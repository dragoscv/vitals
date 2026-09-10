//! Bearer tokens and what they are allowed to do.
//!
//! The threat model is honest about what this is: a plaintext HTTP service on
//! a home or office LAN. It is not trying to survive an attacker who can read
//! your traffic — TLS with a self-signed certificate would make the phone
//! show a warning and break the install-to-home-screen flow, buying a warning
//! dialog instead of security. What it *is* trying to prevent:
//!
//! - a device that never scanned the QR code reading your process list;
//! - a read-only pairing being able to end a process;
//! - a leaked token being permanent (they are revocable, and can be rotated).
//!
//! The token travels in the URL fragment of the QR code, which browsers do
//! not send to the server, so it never appears in a request line or a log.
//! The phone reads it from `location.hash` and puts it in `Authorization`.

use std::fmt;

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

/// What a token may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// See everything. The default for a phone pairing.
    Read,
    /// See everything, and act: end, suspend, resume, set priority.
    ///
    /// Never the default. A phone left on a desk with a `control` token is a
    /// remote kill switch for the machine, so granting it is a deliberate act
    /// in Settings.
    Control,
}

impl Scope {
    #[must_use]
    pub const fn allows_control(self) -> bool {
        matches!(self, Self::Control)
    }
}

/// A single credential.
#[derive(Clone, Serialize, Deserialize)]
pub struct Token {
    /// The secret. 32 bytes, base64url, no padding.
    pub secret: String,
    pub scope: Scope,
    /// What the user called this pairing, so revoking the right one is
    /// possible ("Pixel 9", "Grafana").
    pub label: String,
    /// Unix seconds.
    pub created: i64,
}

// The `secret` field is deliberately redacted, which clippy reads as
// "missing". That is the point.
#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Token {
    /// Never prints the secret.
    ///
    /// Terminal output and tracing spans are persisted; a token that leaks
    /// into a log is a token that has to be rotated. Only the prefix, which
    /// is enough to correlate two log lines.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field(
                "secret",
                &format_args!("{}…", &self.secret[..self.secret.len().min(6)]),
            )
            .field("scope", &self.scope)
            .field("label", &self.label)
            .finish()
    }
}

/// Every currently-valid token.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenSet {
    pub tokens: Vec<Token>,
}

impl TokenSet {
    /// Resolves a presented secret to its scope.
    ///
    /// Compares every token in constant time and does not stop at the first
    /// match: an early return leaks, through timing, how far down the list a
    /// guess got. On a LAN that is a thin channel, but the cost of closing it
    /// is one boolean.
    #[must_use]
    pub fn scope_for(&self, presented: &str) -> Option<Scope> {
        let mut found = None;
        for token in &self.tokens {
            let hit: bool = token
                .secret
                .as_bytes()
                .ct_eq(presented.as_bytes())
                .unwrap_u8()
                == 1;
            if hit {
                found = Some(token.scope);
            }
        }
        found
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn revoke(&mut self, secret: &str) {
        self.tokens.retain(|t| t.secret != secret);
    }

    pub fn revoke_all(&mut self) {
        self.tokens.clear();
    }
}

/// Generates a token secret.
///
/// 256 bits from the OS CSPRNG, base64url without padding so it survives a URL
/// fragment and a QR code without escaping.
///
/// The OS source rather than a userspace PRNG, because this value is the only
/// thing standing between the LAN and your process list. A failure here
/// **panics** rather than falling back to something weaker: a predictable
/// token is worse than no server at all.
#[must_use]
// Deliberate: see the doc comment. A weak fallback would be the bug.
#[allow(clippy::expect_used)]
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS randomness source must work");
    base64url(&bytes)
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(secret: &str, scope: Scope) -> Token {
        Token {
            secret: secret.to_owned(),
            scope,
            label: "test".into(),
            created: 0,
        }
    }

    #[test]
    fn an_unknown_secret_has_no_scope() {
        let set = TokenSet {
            tokens: vec![token("aaa", Scope::Read)],
        };
        assert_eq!(set.scope_for("bbb"), None);
        assert_eq!(set.scope_for(""), None);
    }

    #[test]
    fn a_known_secret_resolves_to_its_scope() {
        let set = TokenSet {
            tokens: vec![token("aaa", Scope::Read), token("bbb", Scope::Control)],
        };
        assert_eq!(set.scope_for("aaa"), Some(Scope::Read));
        assert_eq!(set.scope_for("bbb"), Some(Scope::Control));
    }

    #[test]
    fn a_prefix_is_not_a_match() {
        // Constant-time comparison must still be a comparison.
        let set = TokenSet {
            tokens: vec![token("secretvalue", Scope::Read)],
        };
        assert_eq!(set.scope_for("secret"), None);
        assert_eq!(set.scope_for("secretvalue1"), None);
    }

    #[test]
    fn revoking_removes_exactly_one() {
        let mut set = TokenSet {
            tokens: vec![token("aaa", Scope::Read), token("bbb", Scope::Read)],
        };
        set.revoke("aaa");
        assert_eq!(set.scope_for("aaa"), None);
        assert_eq!(set.scope_for("bbb"), Some(Scope::Read));
    }

    #[test]
    fn read_scope_cannot_control() {
        assert!(!Scope::Read.allows_control());
        assert!(Scope::Control.allows_control());
    }

    #[test]
    fn secrets_are_long_url_safe_and_not_repeated() {
        let a = generate_secret();
        let b = generate_secret();
        assert_ne!(a, b, "two secrets in a row must not collide");
        assert_eq!(a.len(), 43, "32 bytes base64url unpadded");
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "must survive a URL fragment unescaped: {a}"
        );
    }

    #[test]
    fn debug_never_prints_the_whole_secret() {
        // Terminal output is persisted in session logs; a secret that reaches
        // one has to be rotated.
        let t = token("supersecretvalue-do-not-log", Scope::Read);
        let shown = format!("{t:?}");
        assert!(!shown.contains("supersecretvalue"), "{shown}");
        assert!(shown.contains("supers"), "prefix is useful for correlation");
    }

    #[test]
    fn base64url_matches_known_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(&[251, 255, 190]), "-_--");
    }
}
