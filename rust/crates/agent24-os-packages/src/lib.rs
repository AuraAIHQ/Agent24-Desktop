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
    if let Some(over) = std::env::var_os("A24_OS_PACKAGES") {
        return std::path::PathBuf::from(over);
    }
    if ephemeral {
        // An ephemeral daemon must not read the real user's packages: it is used
        // by tests and by `agent24 chat` with no daemon running, and silently
        // mounting whatever the user happens to have installed would make those
        // runs depend on machine state they never asked about.
        return std::env::temp_dir().join(format!("agent24-ephemeral-pkgs-{}", std::process::id()));
    }
    state_dir.join("packages")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Both tests below read the DEFAULT path, so the override must not be set.
    /// Failing loudly beats skipping silently: a skipped test reports the same
    /// `ok` as a passing one, and this pair is the only thing holding the
    /// package root apart from the data root.
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
        let state = std::path::Path::new("/tmp/a24state");
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
        let state = std::path::Path::new("/tmp/a24state");
        let eph = packages_root(state, true);
        assert_ne!(eph, packages_root(state, false));
        assert!(!eph.starts_with(state), "{}", eph.display());
    }
}
