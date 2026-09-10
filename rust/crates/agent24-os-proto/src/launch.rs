//! ME-3b-3 — turning a manifest's `spawn` command into a running child.
//!
//! Three things happen here and they are separable on purpose:
//!
//! 1. [`resolve`] — decide WHICH program, and refuse anything outside the
//!    package. This is where the load-bearing check lives; the manifest's own
//!    `SpawnCommand::validate` is purely lexical and says so.
//! 2. [`mint_token`] — a fresh secret for this one handshake.
//! 3. [`spawn`] — start it, in its own process group, with pipes.
//!
//! Supervision (backoff, circuit breaker, startup timeout) is the next slice and
//! deliberately not here: what "ready" means depends on the handshake, and a
//! restart policy written before there is anything to restart is a guess.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use agent24_domain::SpawnCommand;

/// Why a module could not be started.
#[derive(Debug)]
pub enum LaunchError {
    /// The command could not be resolved to a program.
    Unresolved(String),
    /// It resolved to something outside the package directory.
    EscapesPackage { resolved: PathBuf, package: PathBuf },
    /// The OS refused to start it.
    Spawn(std::io::Error),
    /// A fresh token could not be produced. **Not** recoverable by reusing an
    /// old one — see [`mint_token`].
    NoEntropy(std::io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unresolved(s) => write!(f, "{s}"),
            Self::EscapesPackage { resolved, package } => write!(
                f,
                "the spawn command resolves to {}, which is outside the package at {}",
                resolved.display(),
                package.display()
            ),
            Self::Spawn(e) => write!(f, "could not start the module process: {e}"),
            Self::NoEntropy(e) => write!(f, "could not mint a handshake token: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Decide which program a `spawn` command names.
///
/// # The check the manifest's own validation cannot do
///
/// `SpawnCommand::validate` is **lexical**: it refuses `/bin/sh` and `..`, and
/// its docs say plainly that it establishes only "the manifest contains no
/// spelled-out path escape". Two things defeat reading it as more than that: a
/// bare name resolves through `PATH` (deliberate — it is what lets `node` work),
/// and **a symlink inside the package is invisible to a lexical check**.
///
/// So the load-bearing check is here, where the path is real: a relative command
/// is canonicalised and must still lie under the canonicalised package
/// directory. `bin/node` pointing at `/bin/sh` is refused at this point, and
/// only at this point.
///
/// A bare name (no separator) is looked up on `PATH` and is NOT subject to that
/// rule — by design. It is how a Node or Python module names its interpreter,
/// and pretending otherwise would push every such module into a wrapper script,
/// which is the indirection this whole field exists to avoid.
///
/// # Errors
///
/// [`LaunchError::Unresolved`] or [`LaunchError::EscapesPackage`].
pub fn resolve(spawn: &SpawnCommand, package_dir: &Path) -> Result<PathBuf, LaunchError> {
    let command = Path::new(&spawn.command);
    // A bare name has no separator. `Path::components` would normalise away a
    // leading `./`, so the test is on the raw string.
    if !spawn.command.contains(std::path::MAIN_SEPARATOR) {
        return which(&spawn.command).ok_or_else(|| {
            LaunchError::Unresolved(format!(
                "spawn.command {:?} was not found on PATH",
                spawn.command
            ))
        });
    }

    let package = package_dir.canonicalize().map_err(|e| {
        LaunchError::Unresolved(format!(
            "the package directory {} could not be resolved: {e}",
            package_dir.display()
        ))
    })?;
    let resolved = package.join(command).canonicalize().map_err(|e| {
        LaunchError::Unresolved(format!(
            "spawn.command {:?} could not be resolved inside the package: {e}",
            spawn.command
        ))
    })?;
    if !resolved.starts_with(&package) {
        return Err(LaunchError::EscapesPackage { resolved, package });
    }
    Ok(resolved)
}

/// Find `name` on `PATH`, the way `execvp` would.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// How many bytes of entropy a handshake token carries.
pub const TOKEN_BYTES: usize = 32;

/// Mint a token for ONE handshake.
///
/// # Why this returns an error instead of a fallback
///
/// The token is what tells the kernel that the process talking to it is the one
/// it started. What makes the naive `!=` comparison in the handshake acceptable
/// is not that the token is "one-shot" as a description — it is that **each
/// secret can be measured exactly once** (a byte-at-a-time timing attack needs
/// on the order of 256×len measurements). That property has two preconditions,
/// and this function owns the first: a fresh token per spawn.
///
/// The second — **not reusing a token when a failed handshake is retried** —
/// belongs to the supervisor, and it is the one that fails silently. See FU-44.
///
/// A weaker fallback (time, pid, an address) would keep the daemon running while
/// removing the property the comparison depends on, and nothing downstream could
/// tell. Refusing to start is the honest outcome.
///
/// # Errors
///
/// [`LaunchError::NoEntropy`] when the system source cannot be read.
pub fn mint_token() -> Result<String, LaunchError> {
    use std::io::Read;

    let mut bytes = [0u8; TOKEN_BYTES];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(LaunchError::NoEntropy)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// A started module process, plus the pipes to talk to it.
#[derive(Debug)]
pub struct Launched {
    /// The child. Its process GROUP is its own, so the supervisor can signal the
    /// whole tree rather than only the process it started.
    pub child: Child,
    /// The token given to this child, for the handshake to compare against.
    pub token: String,
}

/// Start a module.
///
/// # Its own process group, and why that is not a detail
///
/// A module written in a scripting language routinely starts helpers. Killing
/// only the pid we hold leaves those running — holding ports, holding the
/// package directory, and invisible to a `disable` that reported success. The
/// child is therefore put in a new process group at spawn, so the supervisor can
/// signal the group.
///
/// The token is passed in the ENVIRONMENT rather than on the command line:
/// arguments are world-readable through `ps` on most systems.
///
/// # Errors
///
/// [`LaunchError`] — resolution, entropy, or the OS refusing.
pub fn spawn(
    spawn_command: &SpawnCommand,
    package_dir: &Path,
    data_dir: &Path,
) -> Result<Launched, LaunchError> {
    use std::os::unix::process::CommandExt;

    let program = resolve(spawn_command, package_dir)?;
    let token = mint_token()?;

    let mut cmd = Command::new(&program);
    cmd.args(&spawn_command.args)
        .current_dir(package_dir)
        .env("A24_HANDSHAKE_TOKEN", &token)
        .env("A24_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 0 means "a new group whose id is this child's pid".
        .process_group(0);

    let child = cmd.spawn().map_err(LaunchError::Spawn)?;
    Ok(Launched { child, token })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn pkg() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("bin")).unwrap();
        t
    }

    fn exe(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn cmd(command: &str, args: &[&str]) -> SpawnCommand {
        SpawnCommand {
            command: command.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// **The check the manifest's lexical validation cannot do**, and the reason
    /// this function exists.
    ///
    /// `bin/node` passes `SpawnCommand::validate` — no absolute path, no `..` —
    /// while pointing at `/bin/sh`. A package that could do that would be
    /// reviewed by reading it and still run something else.
    #[test]
    fn a_symlink_inside_the_package_that_points_out_is_refused() {
        let t = pkg();
        let link = t.path().join("bin/node");
        std::os::unix::fs::symlink("/bin/sh", &link).unwrap();

        let spawn = cmd("bin/node", &[]);
        // It passes the manifest's own check…
        spawn.validate().expect("lexically it is clean");
        // …and is refused here, where the path is real.
        let err = resolve(&spawn, t.path()).expect_err("must be refused");
        assert!(matches!(err, LaunchError::EscapesPackage { .. }), "{err:?}");

        // Control: a real program at the same path resolves. Without this the
        // refusal above could be "symlinks never resolve" rather than "this one
        // leaves the package".
        std::fs::remove_file(&link).unwrap();
        exe(&link, "#!/bin/sh\nexit 0\n");
        let ok = resolve(&spawn, t.path()).expect("a real file inside the package");
        assert!(ok.starts_with(t.path().canonicalize().unwrap()));
    }

    /// A symlink that stays inside the package is fine — the rule is about
    /// leaving, not about links. Without this case the implementation could be
    /// "refuse all symlinks" and pass everything above.
    #[test]
    fn a_symlink_that_stays_inside_the_package_is_allowed() {
        let t = pkg();
        exe(&t.path().join("bin/real"), "#!/bin/sh\nexit 0\n");
        std::os::unix::fs::symlink("real", t.path().join("bin/alias")).unwrap();
        let ok = resolve(&cmd("bin/alias", &[]), t.path()).expect("stays inside");
        assert!(ok.ends_with("bin/real"), "{}", ok.display());
    }

    /// A bare name goes to `PATH` and is deliberately NOT held to the
    /// package-containment rule — that is how a Node or Python module names its
    /// interpreter. Holding it to that rule would push every such module into a
    /// wrapper script, which is the indirection the `spawn` field exists to
    /// avoid.
    #[test]
    fn a_bare_name_resolves_through_path_and_may_live_outside_the_package() {
        let t = pkg();
        let resolved = resolve(&cmd("sh", &[]), t.path()).expect("sh is on PATH");
        assert!(resolved.is_absolute(), "{}", resolved.display());
        assert!(
            !resolved.starts_with(t.path()),
            "the interpreter is outside the package, and that is the point"
        );
    }

    #[test]
    fn a_bare_name_that_is_not_on_path_is_a_clear_refusal() {
        let t = pkg();
        let err = resolve(&cmd("definitely-not-a-real-program-xyz", &[]), t.path())
            .expect_err("must not resolve");
        assert!(matches!(err, LaunchError::Unresolved(_)), "{err:?}");
    }

    /// The property the handshake's naive comparison rests on: **a fresh secret
    /// per spawn**. Not "usually different" — this is checked as a set.
    #[test]
    fn every_token_is_new() {
        let n = 64;
        let tokens: std::collections::BTreeSet<String> =
            (0..n).map(|_| mint_token().unwrap()).collect();
        assert_eq!(tokens.len(), n, "a token repeated within {n} draws");
        // And each is the full width — a short token is a weak one, and length
        // is the part a bad entropy path would silently change.
        for t in &tokens {
            assert_eq!(t.len(), TOKEN_BYTES * 2, "{t}");
            assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    /// The child must be in its OWN process group, so a supervisor can signal
    /// the whole tree. A module that starts helpers and is killed by pid alone
    /// leaves them running — holding ports, holding the package directory, and
    /// invisible to a `disable` that reported success.
    #[test]
    fn the_child_gets_its_own_process_group() {
        let t = pkg();
        // Print our own process-group id and exit.
        exe(
            &t.path().join("bin/mod"),
            "#!/bin/sh\nps -o pgid= -p $$ | tr -d ' '\n",
        );
        let launched = spawn(&cmd("bin/mod", &[]), t.path(), t.path()).expect("spawn");
        let pid = launched.child.id();
        let out = launched.child.wait_with_output().unwrap();
        let child_pgid: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();

        assert_eq!(
            child_pgid, pid,
            "the child's process group should be its own pid"
        );
        // Control: it is NOT this process's group — otherwise "its own group"
        // would be satisfied by inheriting ours whenever we happen to lead one.
        let ours = std::process::id();
        assert_ne!(child_pgid, ours);
    }

    /// The token reaches the child, and through the environment rather than the
    /// command line — arguments are world-readable through `ps`.
    #[test]
    fn the_token_reaches_the_child_out_of_sight_of_ps() {
        let t = pkg();
        // The child reports BOTH: the env var on the first line, its own argv on
        // the second. Asking the child is the whole point — an assertion on the
        // `args` vector built here would only be checking a value this test just
        // wrote, which is true whatever `spawn` does with it. (Measured: a
        // mutation that ALSO put the token in argv left that version green.)
        exe(
            &t.path().join("bin/mod"),
            "#!/bin/sh\necho \"$A24_HANDSHAKE_TOKEN\"\necho \"$@\"\n",
        );
        let launched = spawn(&cmd("bin/mod", &["--flag"]), t.path(), t.path()).expect("spawn");
        let token = launched.token.clone();
        let out = launched.child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut lines = stdout.lines();

        assert_eq!(lines.next().unwrap_or_default().trim(), token, "env var");
        let argv = lines.next().unwrap_or_default();
        assert!(
            !argv.contains(&token),
            "the token appeared in the child's argv, where `ps` can read it: {argv:?}"
        );
        // Control: the child really can see its arguments, so the assertion above
        // is not satisfied by argv being empty for some unrelated reason.
        assert!(
            argv.contains("--flag"),
            "the child saw no arguments: {argv:?}"
        );
    }
}
