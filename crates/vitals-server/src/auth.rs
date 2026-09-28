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
//!
//! **Only a hash is kept.** A token set is persisted to disk, and a file that
//! holds live secrets turns any backup, sync folder or other local program
//! that can read the profile into a way to pair with the machine. The secret
//! exists in full exactly once — in the QR code — and afterwards the server
//! can recognise it but not reproduce it.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// How many leading characters of a secret are kept in the clear, to tell
/// two pairings apart in a list and to revoke one. Eight base64url
/// characters are 48 bits: plenty to identify, far too few to use.
pub const PREFIX_CHARS: usize = 8;

/// A single credential, as the server remembers it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "StoredToken")]
pub struct Token {
    /// SHA-256 of the secret, lowercase hex. A plain hash rather than a
    /// password KDF because the secret is 256 random bits: there is no
    /// dictionary to slow down, and a KDF would add latency to every request.
    pub hash: String,
    /// The first [`PREFIX_CHARS`] characters of the secret.
    pub prefix: String,
    pub scope: Scope,
    /// What the user called this pairing, so revoking the right one is
    /// possible ("Pixel 9", "Grafana").
    pub label: String,
    /// Unix seconds.
    pub created: i64,
}

impl Token {
    /// Remembers `secret` without keeping it.
    #[must_use]
    pub fn new(secret: &str, scope: Scope, label: impl Into<String>, created: i64) -> Self {
        Self {
            hash: hash_secret(secret),
            prefix: secret.chars().take(PREFIX_CHARS).collect(),
            scope,
            label: label.into(),
            created,
        }
    }
}

/// What `lan-tokens.json` may contain. Files written before hashing hold the
/// secret itself; they are read, hashed, and never written back that way.
#[derive(Deserialize)]
struct StoredToken {
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    scope: Scope,
    label: String,
    created: i64,
}

impl From<StoredToken> for Token {
    fn from(stored: StoredToken) -> Self {
        match stored.secret {
            Some(secret) => Self::new(&secret, stored.scope, stored.label, stored.created),
            None => Self {
                hash: stored.hash.unwrap_or_default(),
                prefix: stored.prefix.unwrap_or_default(),
                scope: stored.scope,
                label: stored.label,
                created: stored.created,
            },
        }
    }
}

/// SHA-256, lowercase hex.
#[must_use]
pub fn hash_secret(secret: &str) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// The hash is deliberately left out, which clippy reads as "missing". It is
// not secret, but it is 64 characters of noise in every log line.
#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Token {
    /// The prefix only: enough to correlate two log lines.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("prefix", &format_args!("{}…", self.prefix))
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
        // An empty token hashes like any other string; refusing it outright
        // keeps a stored entry with a lost hash from matching a bare header.
        if presented.is_empty() {
            return None;
        }
        let presented = hash_secret(presented);
        let mut found = None;
        for token in &self.tokens {
            let hit: bool = token
                .hash
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
        let hash = hash_secret(secret);
        self.tokens.retain(|t| t.hash != hash);
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
        Token::new(secret, scope, "test", 0)
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
        assert!(
            shown.contains("supersec"),
            "prefix is useful for correlation"
        );
    }

    #[test]
    fn a_saved_token_set_never_contains_the_secret() {
        let secret = generate_secret();
        let set = TokenSet {
            tokens: vec![Token::new(&secret, Scope::Control, "Pixel", 1)],
        };
        let json = serde_json::to_string(&set).unwrap_or_default();
        assert!(!json.contains(&secret), "{json}");
        assert!(!json.contains("\"secret\""), "{json}");

        let back: TokenSet = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(back.scope_for(&secret), Some(Scope::Control));
    }

    #[test]
    fn a_plaintext_file_from_an_older_version_still_pairs_and_is_rewritten_hashed() {
        let old = r#"{"tokens":[{"secret":"legacy-secret-value","scope":"read","label":"Old phone","created":5}]}"#;
        let set: TokenSet = serde_json::from_str(old).unwrap_or_default();
        assert_eq!(set.scope_for("legacy-secret-value"), Some(Scope::Read));
        assert_eq!(
            set.tokens.first().map(|t| t.prefix.as_str()),
            Some("legacy-s")
        );

        let rewritten = serde_json::to_string(&set).unwrap_or_default();
        assert!(!rewritten.contains("legacy-secret-value"), "{rewritten}");
    }

    #[test]
    fn an_empty_presented_token_never_matches_even_a_damaged_entry() {
        let set = TokenSet {
            tokens: vec![Token {
                hash: String::new(),
                prefix: String::new(),
                scope: Scope::Control,
                label: "damaged".into(),
                created: 0,
            }],
        };
        assert_eq!(set.scope_for(""), None);
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
