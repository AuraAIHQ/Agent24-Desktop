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

/// The name of the environment variable that overrides the packages root.
///
/// Exported because the two functions below take the override as an ARGUMENT
/// rather than reading it: a caller needs to know which variable to read.
pub const PACKAGES_ROOT_ENV: &str = "A24_OS_PACKAGES";

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
/// Reads [`PACKAGES_ROOT_ENV`] for the caller. That override is not a
/// convenience: it is what lets a test install a package into a temp dir and
/// prove the catalogue is read at startup rather than compiled in, WITHOUT
/// rebuilding the binary.
pub fn packages_root(state_dir: &std::path::Path, ephemeral: bool) -> std::path::PathBuf {
    match resolve_packages_root(env_override().as_deref(), Some(state_dir), ephemeral) {
        Ok(root) => root,
        // Unreachable: the only error `resolve` returns is "no state dir and no
        // override", and a state dir was given. Written as a fallback rather than
        // an `expect` so a future error variant cannot turn this into a panic
        // inside a daemon.
        Err(NoPackagesRoot) => state_dir.join("packages"),
    }
}

/// Read [`PACKAGES_ROOT_ENV`]. The only place in this crate that touches the
/// environment, so every rule about WHICH directory wins is decided by a pure
/// function that a test can drive.
pub fn env_override() -> Option<std::ffi::OsString> {
    std::env::var_os(PACKAGES_ROOT_ENV)
}

/// Decide the packages root from inputs, reading nothing.
///
/// The ORDER is the point, and it is the reason this takes `over` as an argument
/// instead of reading the environment itself. The override is consulted FIRST, so
/// a container or CI runner that sets it but has no `HOME` gets the directory it
/// asked for instead of an error about a home directory it deliberately does not
/// have — which is exactly the environment the override is documented for.
///
/// An earlier version read the variable in here. The ordering was then correct
/// but UNGUARDED: moving the override check after the state dir — reinstating the
/// bug verbatim — turned no test red, because a test cannot set a process-global
/// variable safely under a parallel runner and so no test exercised the branch at
/// all. A rule worth writing down is a rule worth being able to break in a test.
pub fn resolve_packages_root(
    over: Option<&std::ffi::OsStr>,
    state_dir: Option<&std::path::Path>,
    ephemeral: bool,
) -> Result<std::path::PathBuf, NoPackagesRoot> {
    if let Some(over) = over {
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
        // ME-3b, when a package can name a process to spawn, it is an execution
        // boundary. Note what this does NOT do: it does not create the directory,
        // so it cannot check ownership or mode. A path function is the wrong place
        // for that; see FU-41.
        return Ok(std::env::temp_dir().join(format!("agent24-ephemeral-pkgs-{}", ephemeral_tag())));
    }
    match state_dir {
        Some(dir) => Ok(dir.join("packages")),
        None => Err(NoPackagesRoot),
    }
}

/// There is no override and no state directory, so there is nowhere for packages
/// to be. Carries no message of its own: the caller knows which of its own inputs
/// was missing and can say so in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoPackagesRoot;

impl std::fmt::Display for NoPackagesRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no packages directory: neither {PACKAGES_ROOT_ENV} nor a state directory is set"
        )
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
            .map(|d| u64::from(d.subsec_nanos()) ^ d.as_secs())
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
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    /// The precedence, as a table. Every row is reachable because nothing here
    /// reads the environment — which is the whole reason `over` is a parameter.
    /// Moving the override check below the state dir (the bug this ordering fixes)
    /// turns the first row red; when the variable was read inside the function,
    /// that same change turned nothing red anywhere in the workspace.
    #[test]
    fn the_override_wins_and_it_wins_first() {
        let state = Path::new("/s");
        let over = OsStr::new("/o");
        /// One row: the override, the state dir, whether the caller is ephemeral,
        /// and the root that must come out.
        type Case<'a> = (
            Option<&'a OsStr>,
            Option<&'a Path>,
            bool,
            Result<&'a str, NoPackagesRoot>,
        );
        let cases: [Case<'_>; 5] = [
            // The row that matters: override set, NO state dir — a container or CI
            // runner with no HOME. This must not be an error.
            (Some(over), None, false, Ok("/o")),
            // Override beats a state dir that is also present…
            (Some(over), Some(state), false, Ok("/o")),
            // …and beats the ephemeral branch too, so an ephemeral daemon under an
            // explicit override still reads what the operator pointed it at.
            (Some(over), Some(state), true, Ok("/o")),
            // Controls. Without the override the state dir answers…
            (None, Some(state), false, Ok("/s/packages")),
            // …and with neither there is nowhere, rather than a silent default.
            (None, None, false, Err(NoPackagesRoot)),
        ];
        for (over, state_dir, ephemeral, want) in cases {
            let got = resolve_packages_root(over, state_dir, ephemeral);
            match want {
                Ok(path) => assert_eq!(
                    got,
                    Ok(PathBuf::from(path)),
                    "over={over:?} state={state_dir:?} ephemeral={ephemeral}"
                ),
                Err(e) => assert_eq!(
                    got,
                    Err(e),
                    "over={over:?} state={state_dir:?} ephemeral={ephemeral}"
                ),
            }
        }
    }

    #[test]
    fn the_packages_root_is_not_the_data_root() {
        // The separation this module exists for. Data lives in `os/<name>/`, which
        // the module owns and writes to; the manifest — the file that DECIDES the
        // module's name, namespace and data directory — must not live somewhere the
        // module was handed a handle to.
        let state = Path::new("/tmp/a24state");
        let pkgs = resolve_packages_root(None, Some(state), false).unwrap();
        assert_eq!(pkgs, state.join("packages"));
        assert_ne!(pkgs, state.join("os"), "packages must not be the data root");
    }

    #[test]
    fn an_ephemeral_daemon_does_not_read_the_users_packages() {
        // `agent24 chat` with no daemon spins up an ephemeral one. Silently
        // mounting whatever the user happens to have installed would make those
        // runs depend on machine state nobody asked about.
        let state = Path::new("/tmp/a24state");
        let eph = resolve_packages_root(None, Some(state), true).unwrap();
        assert_ne!(
            eph,
            resolve_packages_root(None, Some(state), false).unwrap()
        );
        assert!(!eph.starts_with(state), "{}", eph.display());
    }

    #[test]
    fn the_ephemeral_root_is_in_the_system_temp_dir_and_not_guessable() {
        // Two claims, because a wrong implementation satisfies either one alone: a
        // fixed `/tmp/agent24-ephemeral-pkgs` is in the temp dir and outside the
        // state dir, and a random path under `./` is unguessable.
        let eph = resolve_packages_root(None, Some(Path::new("/tmp/a24state")), true).unwrap();
        assert!(
            eph.starts_with(std::env::temp_dir()),
            "not under the system temp dir: {}",
            eph.display()
        );
        let name = eph.file_name().unwrap().to_string_lossy().into_owned();
        let pid_only = format!("agent24-ephemeral-pkgs-{}", std::process::id());
        assert_ne!(
            name, pid_only,
            "the pid alone is guessable by any local process, and it is reused"
        );
        assert!(
            name.starts_with(&pid_only),
            "the pid is still wanted for a human reading `ls`: {name}"
        );
        // Same process, same directory — a daemon that scanned one path and later
        // resolved another would silently stop seeing what it mounted.
        assert_eq!(
            eph,
            resolve_packages_root(None, Some(Path::new("/other")), true).unwrap()
        );
    }

    /// `packages_root` is the thin wrapper that reads the environment. Its own
    /// behaviour worth asserting is the one thing it does beyond delegating: with
    /// a state dir supplied it can never fail, so it never panics.
    #[test]
    fn the_env_reading_wrapper_always_answers_when_given_a_state_dir() {
        let got = packages_root(Path::new("/s"), false);
        match env_override() {
            Some(over) => assert_eq!(got, PathBuf::from(over)),
            None => assert_eq!(got, PathBuf::from("/s/packages")),
        }
    }
}
