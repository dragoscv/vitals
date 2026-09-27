//! Who a process runs as.
//!
//! The per-process part is one `OpenProcess` + `OpenProcessToken` +
//! `GetTokenInformation`; the expensive part is `LookupAccountSidW`, an LSA
//! round trip. A machine has a handful of accounts and hundreds of processes,
//! so names are cached per SID and owners per process identity — a process
//! never changes the user in its primary token. Steady state costs one hash
//! lookup per row.
//!
//! This used to be deferred to the detail query "because of the LSA cost",
//! which in practice meant the User column was an em dash on every row of a
//! table where it is visible by default.

use std::collections::{HashMap, HashSet};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::System::Threading::{OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

use vitals_core::ids::ProcessKey;

use crate::actions::process::ProcessHandle;

/// Resolves and remembers process owners.
///
/// Without elevation, `OpenProcess` is denied for processes of another
/// account — SYSTEM, the service accounts, another user's session. Those rows
/// keep an honest em dash rather than a guessed "SYSTEM"; a guess that is
/// wrong for a service running as NETWORK SERVICE is worse than no answer.
/// The elevated helper (ADR-0012) is where the rest will come from.
#[derive(Debug, Default)]
pub struct OwnerCache {
    /// Resolved owners, including `None` for a process whose token is denied,
    /// so a protected process is asked once rather than every tick.
    by_process: HashMap<ProcessKey, Option<String>>,
    /// Account name per raw SID bytes.
    by_sid: HashMap<Vec<u8>, Option<String>>,
}

impl OwnerCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_process: HashMap::with_capacity(512),
            by_sid: HashMap::with_capacity(16),
        }
    }

    /// The account a process runs as, as `DOMAIN\user` minus the domain for
    /// local and well-known accounts.
    pub fn owner(&mut self, key: ProcessKey) -> Option<String> {
        if let Some(known) = self.by_process.get(&key) {
            return known.clone();
        }
        let resolved = token_sid(key).and_then(|sid| {
            self.by_sid
                .entry(sid)
                .or_insert_with_key(|sid| account_name(sid))
                .clone()
        });
        self.by_process.insert(key, resolved.clone());
        resolved
    }

    /// Forgets processes that are gone, so the cache is bounded by the live
    /// process count rather than by uptime.
    pub fn retain_live(&mut self, live: &HashSet<ProcessKey>) {
        self.by_process.retain(|key, _| live.contains(key));
    }
}

/// Reads the user SID out of a process's primary token.
fn token_sid(key: ProcessKey) -> Option<Vec<u8>> {
    let process = ProcessHandle::open(key.pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the process handle is live; `token` is written only on success.
    if unsafe { OpenProcessToken(process.raw(), TOKEN_QUERY, &raw mut token) } == FALSE {
        return None;
    }
    let token = TokenGuard(token);

    // TOKEN_USER is a pointer plus attributes, followed in the same buffer by
    // the SID it points to. 128 bytes holds the largest SID Windows issues.
    let mut buffer = [0_u64; 16];
    let mut needed = 0_u32;
    // SAFETY: `buffer` is 128 bytes, 8-aligned (TOKEN_USER holds a pointer),
    // and that exact size is passed.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            u32::try_from(size_of_val(&buffer)).unwrap_or(0),
            &raw mut needed,
        )
    };
    if ok == FALSE {
        return None;
    }

    // SAFETY: on success the buffer begins with an initialised TOKEN_USER
    // whose SID pointer targets memory inside `buffer`.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let sid = user.User.Sid;
    // SAFETY: `sid` is a valid SID written by GetTokenInformation.
    let length = unsafe { GetLengthSid(sid) } as usize;
    // SAFETY: GetLengthSid reports the exact byte length of that SID.
    Some(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}

/// Resolves SID bytes to an account name.
fn account_name(sid: &[u8]) -> Option<String> {
    let mut name = [0_u16; 256];
    let mut domain = [0_u16; 256];
    let mut name_len = u32::try_from(name.len()).unwrap_or(0);
    let mut domain_len = u32::try_from(domain.len()).unwrap_or(0);
    let mut use_kind: SID_NAME_USE = 0;
    // SAFETY: `sid` holds a complete SID copied from the token; both buffers
    // and their lengths match; a null system name means the local machine.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid.as_ptr().cast_mut().cast(),
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    if ok == FALSE || name_len == 0 {
        return None;
    }
    // The bare user name, not DOMAIN\user: the column is narrow, the domain is
    // the same machine name on nearly every row, and "SYSTEM" reads better
    // than "NT AUTHORITY\SYSTEM" for the audience this is written for.
    Some(String::from_utf16_lossy(&name[..name_len as usize]))
}

struct TokenGuard(HANDLE);

impl Drop for TokenGuard {
    fn drop(&mut self) {
        // SAFETY: the token came from OpenProcessToken and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessEnumerator;

    #[test]
    fn our_own_process_resolves_to_the_signed_in_user() {
        let me = std::env::var("USERNAME").expect("USERNAME is set on Windows");
        let processes = ProcessEnumerator::new().enumerate().expect("enumerate");
        let own = processes
            .iter()
            .find(|p| p.key.pid.get() == std::process::id())
            .expect("the test process is in the list");
        let owner = OwnerCache::new().owner(own.key);
        assert_eq!(
            owner.as_deref().map(str::to_lowercase),
            Some(me.to_lowercase())
        );
    }

    #[test]
    fn most_processes_have_a_resolvable_owner() {
        // An unelevated caller is denied protected processes, but the bulk of
        // a desktop's processes belong to the user and must resolve; if they
        // do not, the column is still a wall of em dashes.
        let processes = ProcessEnumerator::new().enumerate().expect("enumerate");
        let mut cache = OwnerCache::new();
        let resolved = processes
            .iter()
            .filter(|p| cache.owner(p.key).is_some())
            .count();
        assert!(
            resolved * 2 > processes.len(),
            "only {resolved} of {} processes resolved an owner",
            processes.len()
        );
    }

    #[test]
    fn a_denied_process_is_remembered_rather_than_asked_every_tick() {
        let mut cache = OwnerCache::new();
        let system = ProcessEnumerator::new()
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == 4)
            .expect("the System process exists");
        let first = cache.owner(system.key);
        assert!(cache.by_process.contains_key(&system.key));
        assert_eq!(cache.owner(system.key), first);
    }
}
