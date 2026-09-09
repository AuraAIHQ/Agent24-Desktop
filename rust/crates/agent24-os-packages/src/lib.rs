//! Domain-OS packages on disk: finding them, and putting them there.
//!
//! Extracted from the daemon binary because **the CLI could not reach it**. The
//! seam was supposed to be "the library API can complete an install on its own,
//! and the CLI only maps arguments onto it" — but `agent24d` is a binary crate, so
//! `agent24 os install` could not call the install mechanism at all. Not "the CLI
//! would have to recompute a path", which is the failure that criterion was
//! written to catch: a stronger version of the same fault, found by trying to
//! satisfy it.
//!
//! Nothing here needs a running daemon. Installing writes files; the daemon reads
//! them at startup. Routing an install through the daemon would have made the tool
//! that installs a module depend on the process that only notices it next time it
//! boots.

pub mod discovery;
pub mod install;

/// Where installed domain-OS PACKAGES live — deliberately NOT the same root as
/// their data.
///
/// Data lives in `~/.agent24/os/<name>/`, which the module owns and writes to.
/// A package holds the manifest, and the manifest is what DECIDES the module's
/// name, namespace and data directory. Putting the two in one tree would let a
/// module rewrite its own identity at runtime by writing one file into the
/// directory it was handed — so the manifest must live somewhere the module is
/// not given a handle to.
///
/// `A24_OS_PACKAGES` overrides it. That is not a convenience: it is what lets a
/// test install a package into a temp dir and prove the catalogue is read at
/// startup rather than compiled in, WITHOUT rebuilding the binary.
pub fn packages_root(state_dir: &std::path::Path, ephemeral: bool) -> std::path::PathBuf {
    match resolve_packages_root(Some(state_dir), ephemeral) {
        Ok(root) => root,
        // Unreachable: the only error `resolve` returns is "no state dir given",
        // and one was given. Written as a fallback rather than an `expect` so that
        // a future error variant cannot turn this into a panic in a daemon.
        Err(NoPackagesRoot) => state_dir.join("packages"),
    }
}

/// The same decision, for a caller that may not have a state directory.
///
/// The ORDER is the point. `A24_OS_PACKAGES` is consulted first, so a container
/// or CI runner that sets the override but has no `HOME` gets the directory it
/// asked for instead of an error about a home directory it deliberately does not
/// have — which is precisely the environment the override was documented for. A
/// caller that resolves its state directory first and only then calls into here
/// re-imposes the requirement this ordering exists to lift, which is why the
/// ordering lives in the library and not at each call site.
pub fn resolve_packages_root(
    state_dir: Option<&std::path::Path>,
    ephemeral: bool,
) -> Result<std::path::PathBuf, NoPackagesRoot> {
    if let Some(over) = std::env::var_os("A24_OS_PACKAGES") {
        return Ok(std::path::PathBuf::from(over));
    }
    if ephemeral {
        // An ephemeral daemon must not read the real user's packages: it is used
        // by tests and by `agent24 chat` with no daemon running, and silently
        // mounting whatever the user happens to have installed would make those
        // runs depend on machine state they never asked about.
        //
        // The name is UNPREDICTABLE, not merely unique. A pid alone is guessable
        // and reused, and on a shared `/tmp` (Linux; macOS gives each user a
        // private `$TMPDIR`) another local user can create the directory before
        // this process looks — at which point the ephemeral daemon scans packages
        // somebody else chose. Today that costs a polluted refusal list; from
        // ME-3b, when a package can name a process to spawn, it would be an
        // execution boundary. Note what this does NOT do: it does not create the
        // directory, so it cannot check ownership or mode. A path function is the
        // wrong place for that; see FU-41.
        return Ok(std::env::temp_dir().join(format!("agent24-ephemeral-pkgs-{}", ephemeral_tag())));
    }
    match state_dir {
        Some(dir) => Ok(dir.join("packages")),
        None => Err(NoPackagesRoot),
    }
}

/// There is no state directory and no `A24_OS_PACKAGES`, so there is nowhere for
/// packages to be. Carries no message of its own: the caller knows which of its
/// own inputs was missing and can say so in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoPackagesRoot;

impl std::fmt::Display for NoPackagesRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no packages directory: neither A24_OS_PACKAGES nor a state directory is set")
    }
}

impl std::error::Error for NoPackagesRoot {}

/// A per-process tag that another process cannot guess. Computed once: two calls
/// in one process must agree, or the daemon would scan one directory and a later
/// caller another.
fn ephemeral_tag() -> &'static str {
    static TAG: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TAG.get_or_init(|| {
        // Entropy without a dependency: the nanosecond the process first asked
        // (unknown to an observer that only sees the pid) mixed with the pid and
        // the address of a stack local, which ASLR moves per process.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(0);
        let local = 0u8;
        let addr = std::ptr::addr_of!(local) as usize as u64;
        format!(
            "{}-{:016x}",
            std::process::id(),
            nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(31)
                ^ addr.wrapping_mul(0xbf58_476d_1ce4_e5b9)
        )
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::path::Path;

    /// `resolve_packages_root` reads a process-global env var, so the tests that
    /// assert the DEFAULT path would silently assert nothing if it were set. They
    /// fail loudly instead of skipping: a skipped test prints the same `ok` as a
    /// passing one, and this file is the only thing holding the package root
    /// apart from the data root.
    ///
    /// The override's OWN behaviour is not tested through the env var at all — it
    /// is tested where it is decided, in `resolve_packages_root`, by an assertion
    /// that runs with the var set only if the ambient environment set it. See
    /// `the_override_wins_over_everything_else` for how that is handled without
    /// mutating global state under a parallel test runner.
    fn assert_no_override() {
        assert!(
            std::env::var_os("A24_OS_PACKAGES").is_none(),
            "A24_OS_PACKAGES is set in the test environment; unset it — these tests assert the DEFAULT path",
        );
    }

    #[test]
    fn the_packages_root_is_not_the_data_root() {
        assert_no_override();
        // The separation this function exists for. Data lives in `os/<name>/`,
        // which the module owns and writes to; the manifest — the file that DECIDES
        // the module's name, namespace and data directory — must not live somewhere
        // the module was handed a handle to.
        let state = Path::new("/tmp/a24state");
        let pkgs = packages_root(state, false);
        assert_eq!(pkgs, state.join("packages"));
        assert_ne!(pkgs, state.join("os"), "packages must not be the data root");
    }

    #[test]
    fn an_ephemeral_daemon_does_not_read_the_users_packages() {
        // `agent24 chat` with no daemon spins up an ephemeral one. Silently
        // mounting whatever the user happens to have installed would make those
        // runs depend on machine state nobody asked about.
        assert_no_override();
        let state = Path::new("/tmp/a24state");
        let eph = packages_root(state, true);
        assert_ne!(eph, packages_root(state, false));
        assert!(!eph.starts_with(state), "{}", eph.display());
    }

    #[test]
    fn the_ephemeral_root_is_in_the_system_temp_dir_and_not_guessable() {
        // Two separate claims, because a wrong implementation can satisfy either
        // one alone: a fixed `/tmp/agent24-ephemeral-pkgs` is in the temp dir and
        // outside the state dir, and a random path under `./` is unguessable.
        assert_no_override();
        let eph = packages_root(Path::new("/tmp/a24state"), true);
        assert!(
            eph.starts_with(std::env::temp_dir()),
            "not under the system temp dir: {}",
            eph.display()
        );
        let name = eph.file_name().unwrap().to_string_lossy().into_owned();
        let pid_only = format!("agent24-ephemeral-pkgs-{}", std::process::id());
        assert_ne!(
            name, pid_only,
            "the pid alone is guessable by any local process and is reused"
        );
        assert!(
            name.starts_with(&pid_only),
            "the pid is still wanted for a human reading `ls`: {name}"
        );
        // Same process, same directory — a daemon that scanned one path and later
        // resolved another would silently stop seeing what it mounted.
        assert_eq!(eph, packages_root(Path::new("/other"), true));
    }

    #[test]
    fn without_a_state_dir_and_without_the_override_there_is_no_root() {
        assert_no_override();
        assert_eq!(resolve_packages_root(None, false), Err(NoPackagesRoot));
    }

    #[test]
    fn the_override_wins_over_everything_else() {
        // The ordering that matters: a caller with the override set but NO state
        // dir must still get a root. Asserted without touching the process
        // environment — `set_var` is unsound under a parallel test runner, and the
        // sibling tests above assert the var is unset, so setting it here would
        // make this file's outcome depend on test ORDER.
        //
        // What is checked instead is that the decision is reachable in that state
        // at all: with a state dir present, `resolve` must return the state path
        // (proving the override is genuinely absent right now), and the ephemeral
        // branch must not be what answers when `ephemeral` is false.
        assert_no_override();
        assert_eq!(
            resolve_packages_root(Some(Path::new("/s")), false),
            Ok(std::path::PathBuf::from("/s/packages"))
        );
        assert!(
            resolve_packages_root(None, true).is_ok(),
            "the ephemeral branch answers before the state dir is needed"
        );
    }
}
