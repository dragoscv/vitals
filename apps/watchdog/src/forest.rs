//! Which process to blame — and, as importantly, which not to.
//!
//! Pure and platform-free, so every rule below has a test that runs on the
//! Linux CI leg as well.
//!
//! ## The shape of the problem
//!
//! On 2026-09-28 the machine lagged at 95 % CPU. The load was a Gradle
//! `java.exe` four levels below a `pwsh` chain inside VS Code, a `tsc` nine
//! levels below `turbo`, and VS Code's own GPU process. Naming "the busiest
//! process" is not enough: the busiest *tree* may be a `cargo` whose sixteen
//! `rustc` children each look harmless, and the busiest *root* is `explorer`
//! or VS Code, which must never be offered for ending just because the real
//! culprit was started from them.
//!
//! So the load is summed per subtree, and the search walks **down** from each
//! root to the smallest subtree that still carries most of the load. It never
//! walks up. Shells, terminals, IDEs and the service manager are *hosts*:
//! their children are unrelated jobs, so each child is judged on its own and
//! the host itself is only ever blamed for its own CPU, never its children's.

use std::collections::HashMap;

use vitals_core::ids::{Pid, ProcessKey};

/// One process, with the two loads a target can be chosen by.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub key: ProcessKey,
    pub parent: Option<Pid>,
    pub name: String,
    /// Smoothed CPU, in logical cores (1.0 = one core fully busy).
    pub cpu: f64,
    /// Private bytes.
    pub memory: u64,
}

/// What the load is measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Cpu,
    Memory,
}

impl Metric {
    fn of(self, p: &Proc) -> f64 {
        match self {
            Self::Cpu => p.cpu,
            Self::Memory => p.memory as f64,
        }
    }
}

/// What ending a target means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The process and everything below it.
    Tree,
    /// The process alone. Used for a host that is itself busy.
    Alone,
}

/// A process worth proposing, and what may be offered for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub index: usize,
    /// The load this target accounts for, in the metric it was chosen by.
    pub load: f64,
    pub scope: Scope,
    /// Processes that ending it would also end.
    pub descendants: usize,
    /// False for a busy host that has children: ending it would take down
    /// jobs that are not the problem, so only lowering its priority is
    /// offered.
    pub can_end: bool,
}

/// Processes whose children are independent jobs, not parts of them.
///
/// Matched case-insensitively against the image name. An IDE is here because
/// "the build you started from a VS Code terminal is slow" must never turn
/// into "end VS Code".
const HOSTS: &[&str] = &[
    "explorer.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "svchost.exe",
    "userinit.exe",
    "sihost.exe",
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "pwsh.exe",
    "powershell.exe",
    "bash.exe",
    "sh.exe",
    "wsl.exe",
    "wslhost.exe",
    "code.exe",
    "code - insiders.exe",
    "cursor.exe",
    "devenv.exe",
    "idea64.exe",
    "studio64.exe",
    "code-tunnel.exe",
];

/// Hosts that are never offered for ending, even with no children.
///
/// A childless VS Code GPU process burning two cores can be ended alone —
/// VS Code restarts it. A childless `svchost.exe` is a group of Windows
/// services, and `explorer.exe` is the desktop itself: seen live on
/// 2026-09-28 as `svchost.exe 0.99 cores, end:true`, before this list.
const NEVER_END: &[&str] = &[
    "explorer.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "svchost.exe",
    "userinit.exe",
    "sihost.exe",
    "code-tunnel.exe",
];

/// Whether `name` hosts unrelated children.
#[must_use]
pub fn is_host(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    HOSTS.contains(&lower.as_str())
}

/// Whether a host may be ended when it has no children of its own.
fn host_endable_alone(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !NEVER_END.contains(&lower.as_str())
}

/// The process tree, with parent links that survive PID reuse.
#[derive(Debug)]
pub struct Forest {
    procs: Vec<Proc>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

impl Forest {
    /// Links each process to its parent.
    ///
    /// A parent PID is only believed when that process started strictly
    /// earlier. Windows never clears `InheritedFromUniqueProcessId`, so an
    /// orphan whose parent exited points at whatever later inherited that
    /// number — and without this check a fresh `notepad` could be charged
    /// with a three-day-old build. Strict ordering also makes cycles
    /// impossible, which is what lets every walk below be a plain loop.
    #[must_use]
    pub fn new(procs: Vec<Proc>) -> Self {
        let by_pid: HashMap<u32, usize> = procs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.key.pid.get(), i))
            .collect();

        let parent: Vec<Option<usize>> = procs
            .iter()
            .map(|p| {
                let j = *by_pid.get(&p.parent?.get())?;
                let candidate = procs.get(j)?;
                (candidate.key.start_time < p.key.start_time).then_some(j)
            })
            .collect();

        let mut children = vec![Vec::new(); procs.len()];
        let mut roots = Vec::new();
        for (i, link) in parent.iter().enumerate() {
            match link {
                Some(j) => {
                    if let Some(list) = children.get_mut(*j) {
                        list.push(i);
                    }
                }
                None => roots.push(i),
            }
        }

        Self {
            procs,
            parent,
            children,
            roots,
        }
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Proc> {
        self.procs.get(index)
    }

    #[must_use]
    pub fn find(&self, key: ProcessKey) -> Option<usize> {
        self.procs.iter().position(|p| p.key == key)
    }

    fn kids(&self, index: usize) -> &[usize] {
        self.children.get(index).map_or(&[], Vec::as_slice)
    }

    /// Every process below `index`, nearest first. Excludes `index`.
    #[must_use]
    pub fn descendants(&self, index: usize) -> Vec<usize> {
        let mut out: Vec<usize> = self.kids(index).to_vec();
        let mut at = 0;
        while let Some(&next) = out.get(at) {
            out.extend_from_slice(self.kids(next));
            at += 1;
        }
        out
    }

    /// The parents of `index`, nearest first.
    #[must_use]
    pub fn ancestors(&self, index: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = index;
        while let Some(Some(up)) = self.parent.get(at) {
            out.push(*up);
            at = *up;
        }
        out
    }

    /// Load of every subtree, in `metric`.
    fn subtree_loads(&self, metric: Metric) -> Vec<f64> {
        let mut load: Vec<f64> = self.procs.iter().map(|p| metric.of(p)).collect();
        // Children always start after their parent (enforced in `new`), so
        // visiting latest-first adds every child before its parent is read.
        let mut order: Vec<usize> = (0..self.procs.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.procs.get(i).map(|p| p.key.start_time)));
        for i in order {
            if let (Some(Some(up)), Some(&own)) = (self.parent.get(i), load.get(i))
                && let Some(slot) = load.get_mut(*up)
            {
                *slot += own;
            }
        }
        load
    }

    /// Candidates, heaviest first.
    ///
    /// `share` is how much of a subtree one child must carry for the search
    /// to move down into it: at 0.7 a `cargo` with sixteen equal `rustc`
    /// children stays at `cargo`, while a `java` whose one child does all the
    /// work moves to that child.
    #[must_use]
    pub fn targets(&self, metric: Metric, share: f64) -> Vec<Target> {
        let loads = self.subtree_loads(metric);
        let load = |i: usize| loads.get(i).copied().unwrap_or(0.0);
        let own = |i: usize| self.procs.get(i).map_or(0.0, |p| metric.of(p));
        let host = |i: usize| self.procs.get(i).is_some_and(|p| is_host(&p.name));
        let alone_ok = |i: usize| {
            self.kids(i).is_empty()
                && self
                    .procs
                    .get(i)
                    .is_some_and(|p| host_endable_alone(&p.name))
        };

        let mut out = Vec::new();
        let mut pending: Vec<usize> = self.roots.clone();

        while let Some(n) = pending.pop() {
            if host(n) {
                // Transparent: each child is judged as its own root, and the
                // host is only charged with what it burns itself.
                pending.extend_from_slice(self.kids(n));
                if own(n) > 0.0 {
                    out.push(Target {
                        index: n,
                        load: own(n),
                        scope: Scope::Alone,
                        descendants: 0,
                        can_end: alone_ok(n),
                    });
                }
                continue;
            }

            let mut at = n;
            loop {
                let heaviest = self
                    .kids(at)
                    .iter()
                    .copied()
                    .max_by(|&a, &b| load(a).total_cmp(&load(b)));
                match heaviest {
                    Some(c) if load(c) >= share * load(at) && load(c) > 0.0 => at = c,
                    _ => break,
                }
            }

            if host(at) {
                // The path ran into a host (a build tool that shells out).
                // The host's children are separate jobs again.
                pending.extend_from_slice(self.kids(at));
                if own(at) > 0.0 {
                    out.push(Target {
                        index: at,
                        load: own(at),
                        scope: Scope::Alone,
                        descendants: 0,
                        can_end: alone_ok(at),
                    });
                }
            } else {
                out.push(Target {
                    index: at,
                    load: load(at),
                    scope: Scope::Tree,
                    descendants: self.descendants(at).len(),
                    can_end: true,
                });
            }
        }

        out.sort_by(|a, b| b.load.total_cmp(&a.load));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, start: u64, name: &str, cpu: f64) -> Proc {
        Proc {
            key: ProcessKey::new(Pid(pid), start),
            parent: (parent != 0).then_some(Pid(parent)),
            name: name.to_owned(),
            cpu,
            memory: 0,
        }
    }

    fn name_of(forest: &Forest, t: &Target) -> String {
        forest
            .get(t.index)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    /// The 2026-09-28 incident, trimmed: explorer → VS Code → pwsh chain →
    /// cmd → gradle wrapper java → the Kotlin daemon doing the work.
    fn incident() -> Forest {
        Forest::new(vec![
            p(10, 0, 100, "explorer.exe", 0.1),
            p(20, 10, 200, "Code - Insiders.exe", 0.3),
            p(21, 20, 210, "Code - Insiders.exe", 1.8), // GPU process, a leaf
            p(30, 20, 300, "pwsh.exe", 0.0),
            p(31, 30, 310, "pwsh.exe", 0.0),
            p(40, 31, 400, "cmd.exe", 0.0),
            p(50, 40, 500, "java.exe", 0.1), // gradlew wrapper
            p(51, 50, 510, "java.exe", 3.8), // the daemon
        ])
    }

    #[test]
    fn the_culprit_is_the_busy_child_never_the_shell_or_the_ide_it_was_started_from() {
        let forest = incident();
        let top = forest.targets(Metric::Cpu, 0.7);
        let first = top.first().expect("a target");
        assert_eq!(forest.get(first.index).map(|p| p.key.pid), Some(Pid(51)));
        assert_eq!(first.scope, Scope::Tree);
        assert!(first.can_end);
        for t in &top {
            let name = name_of(&forest, t).to_ascii_lowercase();
            if name == "explorer.exe" || (name.starts_with("code") && t.index == 1) {
                assert!(!t.can_end, "{name} was offered for ending");
            }
        }
    }

    #[test]
    fn a_busy_ide_process_with_no_children_can_be_ended_alone() {
        let forest = incident();
        let gpu = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .find(|t| forest.get(t.index).map(|p| p.key.pid) == Some(Pid(21)))
            .expect("the GPU process is a candidate");
        assert_eq!(gpu.scope, Scope::Alone);
        assert!(gpu.can_end);
    }

    #[test]
    fn a_busy_host_with_children_is_never_offered_for_ending() {
        let forest = Forest::new(vec![
            p(1, 0, 1, "Code - Insiders.exe", 4.0),
            p(2, 1, 2, "Code - Insiders.exe", 0.1),
        ]);
        let main = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .find(|t| t.index == 0)
            .expect("the main process is a candidate");
        assert!(!main.can_end);
    }

    #[test]
    fn a_build_spread_over_many_workers_blames_the_tool_that_started_them() {
        let mut procs = vec![p(1, 0, 1, "cmd.exe", 0.0), p(2, 1, 2, "cargo.exe", 0.05)];
        for i in 0..16 {
            procs.push(p(100 + i, 2, 10 + u64::from(i), "rustc.exe", 0.4));
        }
        let forest = Forest::new(procs);
        let first = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .next()
            .expect("target");
        assert_eq!(name_of(&forest, &first), "cargo.exe");
        assert_eq!(first.descendants, 16);
        assert!((first.load - 6.45).abs() < 1e-9);
    }

    #[test]
    fn the_search_passes_through_shells_a_build_tool_spawned() {
        // turbo → pnpm → cmd → node → cmd → tsc: the tsc is the one to end.
        let forest = Forest::new(vec![
            p(1, 0, 1, "turbo.exe", 0.0),
            p(2, 1, 2, "pnpm.exe", 0.0),
            p(3, 2, 3, "cmd.exe", 0.0),
            p(4, 3, 4, "node.exe", 0.1),
            p(5, 4, 5, "cmd.exe", 0.0),
            p(6, 5, 6, "tsc.exe", 3.9),
        ]);
        let first = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .next()
            .expect("target");
        assert_eq!(name_of(&forest, &first), "tsc.exe");
    }

    #[test]
    fn a_recycled_parent_pid_does_not_make_an_old_process_a_child_of_a_new_one() {
        // PID 7 exited and was reused by a notepad that started AFTER the
        // build: the build must stay a root, not become notepad's child.
        let forest = Forest::new(vec![
            p(7, 0, 900, "notepad.exe", 0.0),
            p(8, 7, 500, "java.exe", 3.0),
        ]);
        assert!(forest.ancestors(1).is_empty());
        assert!(forest.descendants(0).is_empty());
        let first = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .next()
            .expect("target");
        assert_eq!(name_of(&forest, &first), "java.exe");
        assert_eq!(first.descendants, 0);
    }

    #[test]
    fn an_app_with_load_spread_over_its_own_children_is_blamed_as_a_whole() {
        let mut procs = vec![p(1, 0, 1, "chrome.exe", 0.2)];
        for i in 0..10 {
            procs.push(p(10 + i, 1, 2 + u64::from(i), "chrome.exe", 0.3));
        }
        let forest = Forest::new(procs);
        let first = forest
            .targets(Metric::Cpu, 0.7)
            .into_iter()
            .next()
            .expect("target");
        assert_eq!(first.index, 0);
        assert_eq!(first.descendants, 10);
    }

    #[test]
    fn memory_targets_are_chosen_by_private_bytes() {
        let mut a = p(1, 0, 1, "leaky.exe", 0.0);
        a.memory = 40 << 30;
        let mut b = p(2, 0, 2, "busy.exe", 8.0);
        b.memory = 1 << 30;
        let forest = Forest::new(vec![a, b]);
        let first = forest
            .targets(Metric::Memory, 0.7)
            .into_iter()
            .next()
            .expect("target");
        assert_eq!(name_of(&forest, &first), "leaky.exe");
    }

    #[test]
    fn ancestry_is_nearest_first_and_descendants_include_grandchildren() {
        let forest = incident();
        let daemon = forest.find(ProcessKey::new(Pid(51), 510)).expect("daemon");
        let chain: Vec<u32> = forest
            .ancestors(daemon)
            .iter()
            .filter_map(|&i| forest.get(i).map(|p| p.key.pid.get()))
            .collect();
        assert_eq!(chain, vec![50, 40, 31, 30, 20, 10]);
        assert_eq!(forest.descendants(0).len(), 7);
    }

    #[test]
    fn a_busy_service_host_or_desktop_is_never_offered_for_ending_even_alone() {
        let forest = Forest::new(vec![
            p(1, 0, 1, "services.exe", 0.0),
            p(2, 1, 2, "svchost.exe", 3.0),
            p(3, 0, 3, "explorer.exe", 2.0),
        ]);
        let targets = forest.targets(Metric::Cpu, 0.7);
        assert!(!targets.is_empty());
        for t in targets {
            assert!(
                !t.can_end,
                "{} was offered for ending",
                name_of(&forest, &t)
            );
        }
    }

    #[test]
    fn host_matching_ignores_case() {
        assert!(is_host("PWSH.EXE"));
        assert!(!is_host("java.exe"));
    }
}
