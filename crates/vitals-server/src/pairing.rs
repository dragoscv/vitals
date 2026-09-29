//! Short-code pairing, for devices that cannot scan a QR code.
//!
//! A Google TV or a Samsung TV has no camera, and typing a 43-character token
//! with a remote is miserable. So the desktop shows six digits for five
//! minutes, the TV finds the PC over mDNS, and `POST /api/v1/pair` exchanges
//! the digits for an ordinary bearer token. After that the TV is a paired
//! device like any phone: the code is gone and only the token remains.
//!
//! Six digits are a million possibilities, which is nothing against an
//! unlimited guesser. What makes it safe is the budget around it:
//!
//! - **One code at a time.** Creating a new one replaces the old, so there is
//!   never more than one target on the network.
//! - **Five wrong guesses in total, from anyone,** and the code is burnt —
//!   even the right code is refused afterwards. A per-caller limit would be
//!   no limit at all on a LAN where addresses are free.
//! - **Five minutes.** An unused code expires on its own.
//! - **Every refusal looks the same.** No code, an expired one, a wrong one
//!   and a burnt one all return byte-identical bodies; telling them apart is
//!   a free oracle for whoever is guessing.
//!
//! That bounds a guesser to five chances in a million per code the user
//! deliberately showed. Burning the code is a denial of service against the
//! pairing, never against the machine: the user presses the button again.
//!
//! Neither the code nor the token it buys is ever logged.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use subtle::ConstantTimeEq;

use crate::auth::{Scope, Token, TokenSet};
use crate::state::ServerLock;

/// How long a code is valid after it is shown.
pub const CODE_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// Wrong guesses allowed across every caller before the code is burnt.
pub const MAX_ATTEMPTS: u8 = 5;

/// Digits in a code.
pub const CODE_DIGITS: usize = 6;

/// Longest label kept for a paired device, in characters. A TV sends what
/// its owner typed; the token list must not become a place to store essays.
pub const MAX_LABEL_CHARS: usize = 64;

/// The label a device gets when it sends none.
pub const DEFAULT_LABEL: &str = "TV";

/// Called with the whole token set after a redemption adds to it.
///
/// The host owns persistence — the desktop writes `lan-tokens.json`, the
/// headless CLI keeps nothing — so the desk only says when.
pub type PersistTokens = Arc<dyn Fn(&TokenSet) + Send + Sync>;

/// Unix milliseconds now. Injectable so expiry can be tested without waiting
/// five minutes.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// A code as it is handed to the person who will type it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedCode {
    /// Exactly [`CODE_DIGITS`] ASCII digits, leading zeros kept: `004213` is
    /// a valid code and must be shown as such.
    pub code: String,
    pub expires_at_ms: u64,
}

/// Whether a code is waiting to be used. What the desktop polls to learn
/// that the TV has paired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStatus {
    pub active: bool,
    /// `None` when nothing is active.
    pub expires_at_ms: Option<u64>,
}

/// Why a redemption failed. Deliberately a single case: the reasons are
/// known here, and must not leave this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

/// A redeemed code: the new secret, shown to the TV once, and its scope.
#[derive(Clone, PartialEq, Eq)]
pub struct Redeemed {
    pub secret: String,
    pub scope: Scope,
}

impl std::fmt::Debug for Redeemed {
    /// The secret is never printed; see the module doc.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redeemed")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

struct ActiveCode {
    digits: [u8; CODE_DIGITS],
    scope: Scope,
    expires_at_ms: u64,
    attempts_left: u8,
}

/// Holds the one active pairing code, if any.
pub struct PairingDesk {
    // A plain mutex: every operation is a few comparisons, and redeem must
    // decide and consume atomically or two racing correct guesses could both
    // be granted a token.
    active: Mutex<Option<ActiveCode>>,
    clock: Clock,
    persist: PersistTokens,
}

impl std::fmt::Debug for PairingDesk {
    /// Whether a code is active, never the code.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingDesk")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl PairingDesk {
    /// A desk on the system clock.
    #[must_use]
    pub fn new(persist: PersistTokens) -> Self {
        Self::with_clock(persist, Arc::new(system_now_ms))
    }

    /// A desk on the given clock.
    #[must_use]
    pub fn with_clock(persist: PersistTokens, clock: Clock) -> Self {
        Self {
            active: Mutex::new(None),
            clock,
            persist,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<ActiveCode>> {
        // Poisoning is ignored for the same reason as `ServerLock`: the
        // contents are a small value with no invariant a panic could break
        // half-way, and a poisoned lock would disable pairing for the session.
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Creates a code that redeems for a token of `scope`, replacing any code
    /// already active — there is never more than one target to guess at.
    #[must_use]
    pub fn create(&self, scope: Scope) -> IssuedCode {
        let digits = random_digits();
        let expires_at_ms = (self.clock)().saturating_add(lifetime_ms());
        *self.lock() = Some(ActiveCode {
            digits,
            scope,
            expires_at_ms,
            attempts_left: MAX_ATTEMPTS,
        });
        IssuedCode {
            code: digits.iter().map(|d| char::from(b'0' + d)).collect(),
            expires_at_ms,
        }
    }

    /// Withdraws the active code, if any.
    pub fn cancel(&self) {
        *self.lock() = None;
    }

    /// Whether a code can still be redeemed. An expired code reads as
    /// inactive even before anyone tries it.
    #[must_use]
    pub fn status(&self) -> PairingStatus {
        let now = (self.clock)();
        let guard = self.lock();
        match guard.as_ref() {
            Some(active) if now < active.expires_at_ms => PairingStatus {
                active: true,
                expires_at_ms: Some(active.expires_at_ms),
            },
            _ => PairingStatus {
                active: false,
                expires_at_ms: None,
            },
        }
    }

    /// Exchanges `code` for a new token, added to `tokens` and persisted.
    ///
    /// `code` must already be [`CODE_DIGITS`] ASCII digits; the router rejects
    /// anything else as a malformed request **before** calling this, so a
    /// typo in the body does not spend one of the five attempts.
    ///
    /// # Errors
    ///
    /// [`Refused`] for every failure — none active, expired, wrong, burnt —
    /// so the caller cannot tell them apart even by accident.
    pub fn redeem(
        &self,
        code: &str,
        label: Option<&str>,
        tokens: &ServerLock<TokenSet>,
    ) -> Result<Redeemed, Refused> {
        let now = (self.clock)();
        let mut guard = self.lock();
        let Some(active) = guard.as_mut() else {
            return Err(Refused);
        };
        if now >= active.expires_at_ms {
            *guard = None;
            return Err(Refused);
        }

        let presented = parse_digits(code);
        let matches = presented.is_some_and(|digits| bool::from(digits.ct_eq(&active.digits)));
        if !matches {
            active.attempts_left = active.attempts_left.saturating_sub(1);
            if active.attempts_left == 0 {
                *guard = None;
            }
            return Err(Refused);
        }

        let scope = active.scope;
        // Consumed before the token exists, under the same lock: a second
        // correct guess racing this one finds nothing.
        *guard = None;
        drop(guard);

        let secret = crate::auth::generate_secret();
        let created = i64::try_from(now / 1000).unwrap_or(i64::MAX);
        let token = Token::new(&secret, scope, clean_label(label), created);
        {
            let mut set = tokens.write();
            set.tokens.push(token);
            (self.persist)(&set);
        }
        Ok(Redeemed { secret, scope })
    }
}

/// Whether `code` is the shape a code must have. Checked by the router so a
/// malformed body is a 400 and costs no attempt.
#[must_use]
pub fn is_well_formed(code: &str) -> bool {
    parse_digits(code).is_some()
}

fn parse_digits(code: &str) -> Option<[u8; CODE_DIGITS]> {
    let bytes = code.as_bytes();
    if bytes.len() != CODE_DIGITS {
        return None;
    }
    let mut digits = [0u8; CODE_DIGITS];
    for (slot, byte) in digits.iter_mut().zip(bytes) {
        if !byte.is_ascii_digit() {
            return None;
        }
        *slot = byte - b'0';
    }
    Some(digits)
}

/// Trimmed, capped at [`MAX_LABEL_CHARS`] characters (not bytes, so a
/// Romanian or emoji name is never cut mid-character), [`DEFAULT_LABEL`]
/// when blank.
fn clean_label(label: Option<&str>) -> String {
    let trimmed = label.map(str::trim).unwrap_or_default();
    if trimmed.is_empty() {
        return DEFAULT_LABEL.to_owned();
    }
    let capped: String = trimmed.chars().take(MAX_LABEL_CHARS).collect();
    // Cutting can expose trailing whitespace from the middle of the input.
    capped.trim_end().to_owned()
}

/// Six digits, uniform over `000000..=999999`.
///
/// Rejection sampling rather than `% 1_000_000`: 2³² is not a multiple of a
/// million, so a plain modulo makes the low 967,296 codes slightly likelier
/// than the rest — a bias a guesser could spend its five attempts on.
///
/// The OS source, and a panic if it fails, for the same reason as
/// [`crate::auth::generate_secret`]: a predictable code is worse than none.
// Deliberate: see the doc comment. A weak fallback would be the bug.
#[allow(clippy::expect_used)]
fn random_digits() -> [u8; CODE_DIGITS] {
    const SPACE: u32 = 1_000_000;
    // The largest multiple of SPACE that fits in a u32; draws at or above it
    // are thrown away. Fewer than one draw in four thousand is rejected.
    const ZONE: u32 = u32::MAX - (u32::MAX % SPACE);
    let value = loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("the OS randomness source must work");
        let draw = u32::from_le_bytes(bytes);
        if draw < ZONE {
            break draw % SPACE;
        }
    };
    let mut digits = [0u8; CODE_DIGITS];
    let mut rest = value;
    for slot in digits.iter_mut().rev() {
        // `rest % 10` is below ten, so the cast cannot truncate.
        *slot = (rest % 10) as u8;
        rest /= 10;
    }
    digits
}

fn lifetime_ms() -> u64 {
    u64::try_from(CODE_LIFETIME.as_millis()).unwrap_or(u64::MAX)
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use super::*;

    const START_MS: u64 = 1_700_000_000_000;

    struct Rig {
        desk: PairingDesk,
        now: Arc<AtomicU64>,
        saves: Arc<AtomicUsize>,
        tokens: ServerLock<TokenSet>,
    }

    fn rig() -> Rig {
        let now = Arc::new(AtomicU64::new(START_MS));
        let saves = Arc::new(AtomicUsize::new(0));
        let clock_now = Arc::clone(&now);
        let counted = Arc::clone(&saves);
        let desk = PairingDesk::with_clock(
            Arc::new(move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
            }),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        Rig {
            desk,
            now,
            saves,
            tokens: ServerLock::new(TokenSet::default()),
        }
    }

    /// A well-formed code that is certainly not `code`.
    fn wrong(code: &str) -> String {
        let first = code.as_bytes()[0];
        let other = if first == b'9' {
            '0'
        } else {
            char::from(first + 1)
        };
        format!("{other}{}", &code[1..])
    }

    #[test]
    fn five_wrong_guesses_burn_the_code_so_even_the_right_one_is_refused_afterwards() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        for _ in 0..MAX_ATTEMPTS {
            assert_eq!(
                r.desk.redeem(&wrong(&issued.code), None, &r.tokens),
                Err(Refused)
            );
        }
        assert_eq!(r.desk.redeem(&issued.code, None, &r.tokens), Err(Refused));
        assert!(!r.desk.status().active);
        assert!(r.tokens.read().is_empty(), "no token may have been minted");
    }

    #[test]
    fn four_wrong_guesses_still_leave_the_right_code_usable() {
        // The other half of the budget: the limit is five, not four.
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        for _ in 0..MAX_ATTEMPTS - 1 {
            let _ = r.desk.redeem(&wrong(&issued.code), None, &r.tokens);
        }
        assert!(r.desk.redeem(&issued.code, None, &r.tokens).is_ok());
    }

    #[test]
    fn an_expired_code_is_refused_and_reads_as_inactive() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        r.now.store(issued.expires_at_ms, Ordering::SeqCst);
        assert!(!r.desk.status().active);
        assert_eq!(r.desk.redeem(&issued.code, None, &r.tokens), Err(Refused));
    }

    #[test]
    fn a_code_is_valid_for_exactly_five_minutes() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        assert_eq!(issued.expires_at_ms, START_MS + 300_000);
        r.now.store(issued.expires_at_ms - 1, Ordering::SeqCst);
        assert!(r.desk.redeem(&issued.code, None, &r.tokens).is_ok());
    }

    #[test]
    fn a_code_is_single_use() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        assert!(r.desk.redeem(&issued.code, None, &r.tokens).is_ok());
        assert_eq!(r.desk.redeem(&issued.code, None, &r.tokens), Err(Refused));
        assert_eq!(r.tokens.read().tokens.len(), 1);
    }

    #[test]
    fn a_new_code_replaces_the_old_one_so_only_one_is_ever_guessable() {
        let r = rig();
        let first = r.desk.create(Scope::Read);
        let second = r.desk.create(Scope::Read);
        if first.code != second.code {
            assert_eq!(r.desk.redeem(&first.code, None, &r.tokens), Err(Refused));
        }
        assert!(r.desk.redeem(&second.code, None, &r.tokens).is_ok());
    }

    #[test]
    fn a_new_code_gets_a_fresh_attempt_budget() {
        let r = rig();
        let old = r.desk.create(Scope::Read);
        for _ in 0..MAX_ATTEMPTS - 1 {
            let _ = r.desk.redeem(&wrong(&old.code), None, &r.tokens);
        }
        let new = r.desk.create(Scope::Read);
        for _ in 0..MAX_ATTEMPTS - 1 {
            let _ = r.desk.redeem(&wrong(&new.code), None, &r.tokens);
        }
        assert!(r.desk.redeem(&new.code, None, &r.tokens).is_ok());
    }

    #[test]
    fn a_cancelled_code_is_refused() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        r.desk.cancel();
        assert!(!r.desk.status().active);
        assert_eq!(r.desk.redeem(&issued.code, None, &r.tokens), Err(Refused));
    }

    #[test]
    fn redeeming_mints_a_token_of_the_codes_scope_and_persists_the_set() {
        let r = rig();
        let issued = r.desk.create(Scope::Control);
        let redeemed = r
            .desk
            .redeem(&issued.code, Some("  Living room TV  "), &r.tokens)
            .unwrap();
        assert_eq!(redeemed.scope, Scope::Control);
        assert_eq!(redeemed.secret.len(), 43);

        let set = r.tokens.read();
        assert_eq!(set.scope_for(&redeemed.secret), Some(Scope::Control));
        let token = set.tokens.first().unwrap();
        assert_eq!(token.label, "Living room TV");
        assert_eq!(token.created, i64::try_from(START_MS / 1000).unwrap());
        assert_eq!(r.saves.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_refused_redemption_persists_nothing() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        let _ = r.desk.redeem(&wrong(&issued.code), None, &r.tokens);
        assert_eq!(r.saves.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_blank_or_missing_label_becomes_tv_and_a_long_one_is_capped_by_characters() {
        assert_eq!(clean_label(None), "TV");
        assert_eq!(clean_label(Some("   ")), "TV");
        let long = "ș".repeat(100);
        assert_eq!(clean_label(Some(&long)).chars().count(), MAX_LABEL_CHARS);
    }

    #[test]
    fn generated_codes_are_always_six_ascii_digits() {
        let r = rig();
        for _ in 0..2_000 {
            let issued = r.desk.create(Scope::Read);
            assert_eq!(issued.code.len(), CODE_DIGITS, "{}", issued.code.len());
            assert!(issued.code.bytes().all(|b| b.is_ascii_digit()));
            assert!(is_well_formed(&issued.code));
        }
    }

    #[test]
    fn generated_codes_cover_the_whole_range_rather_than_a_fixed_few() {
        // Not a proof of uniformity, but it fails for a stuck or truncated
        // generator: 2,000 draws from a million repeat only a couple of times
        // (birthday bound ≈ 2), so more than ten repeats means the space is
        // far smaller than it claims.
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..2_000 {
            seen.insert(random_digits());
        }
        assert!(seen.len() > 1_990, "{} distinct of 2000", seen.len());
    }

    #[test]
    fn shape_checks_reject_everything_that_is_not_six_ascii_digits() {
        for bad in [
            "",
            "12345",
            "1234567",
            "12345a",
            " 12345",
            "１２３４５６",
            "-12345",
        ] {
            assert!(!is_well_formed(bad), "{bad:?}");
        }
        assert!(is_well_formed("004213"));
    }

    #[test]
    fn debug_output_never_contains_the_code() {
        let r = rig();
        let issued = r.desk.create(Scope::Read);
        let shown = format!("{:?}", r.desk);
        assert!(!shown.contains(&issued.code), "{shown}");
    }
}
