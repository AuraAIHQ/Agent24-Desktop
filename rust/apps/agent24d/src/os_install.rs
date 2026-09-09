//! ME-3a — installing a package, and the two properties that make it worth
//! having a module of its own.
//!
//! **Atomic**: a package is either fully installed or not installed. There is no
//! state in which the packages root holds half of one.
//!
//! **Clean on failure**: a refused install leaves the packages root byte-for-byte
//! as it was. Not a partial directory, not a `.tmp`, not a changed mtime on the
//! parent.
//!
//! The second is the one that is easy to write a decorative test for. Asserting
//! that this function returned `Err` says nothing about what it left on disk, so
//! the tests here snapshot the whole tree before and after and compare sets. The
//! snapshot function is the INSTRUMENT for those tests, so it has negative
//! controls of its own — see [`tests::the_snapshot_sees_each_kind_of_difference`].

// TEMPORARY, and it must not outlive the next commit. Nothing calls this yet:
// the CLI that will (`agent24 os install`) is deliberately a separate change, so
// that the filesystem properties here and the argument/output contract there can
// each be reviewed against their own kind of failure. If this attribute is still
// here after the CLI lands, something was dropped — the intended callers are
// `os install` / `os uninstall`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::os_discovery::MANIFEST_FILE;
use agent24_domain::DomainOsManifest;

/// Why an install did not happen. Every variant means the packages root was left
/// untouched.
#[derive(Debug)]
pub enum InstallError {
    /// The source is not a package this kernel will accept.
    Source(String),
    /// A package by that name is already installed.
    AlreadyInstalled(String),
    /// The filesystem refused, or the staging directory and the destination are
    /// not on the same device (see [`install`] for why that matters).
    Filesystem(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(s) => write!(f, "source package rejected: {s}"),
            Self::AlreadyInstalled(n) => write!(
                f,
                "a domain OS named {n:?} is already installed; uninstall it first"
            ),
            Self::Filesystem(s) => write!(f, "filesystem: {s}"),
        }
    }
}

/// Install the package directory `src` into `packages_root`.
///
/// The destination name comes from the **validated manifest**, never from the
/// source directory's name. A directory called `sin90/` whose manifest says
/// `cos72` installs as `cos72` — the manifest is the module's sole identity
/// (`DomainOsManifest`), and letting the enclosing directory name decide would
/// give a package two identities that can disagree.
///
/// # Atomicity, and the assumption underneath it
///
/// The package is copied into a staging directory and then moved with a single
/// `rename`, so the only window in which anything can be half-done is that one
/// call — and `rename` of a directory is atomic **within one filesystem**.
///
/// Across filesystems it is not atomic at all: the implementation degrades to
/// copy-then-delete, and a failure mid-copy leaves exactly the partial state this
/// design exists to prevent. That is a CONFIGURATION property, not a code one —
/// it depends on where the staging directory landed on the operator's machine, so
/// a test passing here says nothing about their disk. Hence [`same_device`] is
/// checked at runtime, not merely covered by a test.
pub fn install(src: &Path, packages_root: &Path) -> Result<PathBuf, InstallError> {
    // Read and validate BEFORE touching the destination. A source that will be
    // rejected must never have caused a directory to be created.
    let manifest_path = src.join(MANIFEST_FILE);
    let bytes = std::fs::read(&manifest_path)
        .map_err(|e| InstallError::Source(format!("no readable {MANIFEST_FILE}: {e}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| InstallError::Source(format!("{MANIFEST_FILE} is not valid UTF-8: {e}")))?;
    let manifest =
        DomainOsManifest::from_yaml(&text).map_err(|e| InstallError::Source(e.to_string()))?;

    let dest = packages_root.join(manifest.name());
    if dest.exists() {
        return Err(InstallError::AlreadyInstalled(manifest.name().to_owned()));
    }

    std::fs::create_dir_all(packages_root)
        .map_err(|e| InstallError::Filesystem(format!("could not create packages root: {e}")))?;

    // Staging lives INSIDE the packages root, not in the system temp directory.
    // That is the whole point: `/tmp` is frequently a different volume (it is on
    // stock macOS), and a cross-device rename is not atomic.
    // The name carries a per-call counter as well as the pid. Without it, two
    // concurrent installs of the SAME package in one process share a staging path,
    // and the cleanup below has the second caller delete the first caller's
    // half-written tree. Measured, that still ends cleanly — one wins, one fails,
    // nothing partial is installed — but it ends cleanly for the WRONG REASON:
    // safety comes from the interrupted caller failing ENTIRELY, not from the two
    // never touching each other. The day this function grows a resumable path,
    // that borrowed guarantee disappears with no symptom except packages missing a
    // few files. Cheaper not to share the path in the first place.
    let staging = staging_path(packages_root, manifest.name());
    // Unreachable in practice — the counter is monotonic within the process and the
    // pid separates processes — but a crashed predecessor can leave an OLDER
    // staging directory behind. Those are not this call's to delete: removing a
    // path it did not create is how the shared-path race got its accidental
    // safety, and this function should not rely on that a second time.
    if staging.exists() {
        remove_quietly(&staging);
    }
    copy_tree(src, &staging).inspect_err(|_| remove_quietly(&staging))?;

    // NOTE, so nobody mistakes where the guarantee comes from: today's atomicity
    // comes from staging being CONSTRUCTED inside the destination's parent, not
    // from this check — the check cannot fire as long as that construction holds,
    // and the tests exercise the function, not the gate. Its value is entirely in
    // the future: the first time someone moves staging elsewhere, this is what
    // turns a silent loss of atomicity into a refused install.
    //
    // Belt and braces: staging is inside the destination's parent by construction,
    // so this should always hold. It is checked anyway because the cost of being
    // wrong is silent partial state on somebody else's machine, and because a
    // future change to where staging lives would otherwise break atomicity with no
    // visible symptom.
    if !same_device(&staging, packages_root).unwrap_or(false) {
        remove_quietly(&staging);
        return Err(InstallError::Filesystem(
            "staging directory is on a different filesystem from the packages root; \
             a rename across filesystems is not atomic"
                .to_owned(),
        ));
    }

    std::fs::rename(&staging, &dest).map_err(|e| {
        remove_quietly(&staging);
        InstallError::Filesystem(format!("could not move the package into place: {e}"))
    })?;
    Ok(dest)
}

/// Remove an installed package. Missing is not an error the caller has to handle
/// differently from removed — both end with "it is not installed".
pub fn uninstall(name: &str, packages_root: &Path) -> Result<bool, InstallError> {
    let dest = packages_root.join(name);
    if !dest.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dest).map_err(|e| {
        InstallError::Filesystem(format!("could not remove {}: {e}", dest.display()))
    })?;
    Ok(true)
}

/// A staging path that NO other call will ever produce.
///
/// Extracted so the property can be observed. It is not testable through
/// `install`: sequential calls each clean up after themselves, so a test written
/// against `install` passes whether or not the paths collide — which is exactly
/// what a mutation showed when this was first written as an assertion about
/// leftover debris.
fn staging_path(packages_root: &Path, name: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    packages_root.join(format!(".staging-{name}-{}-{seq}", std::process::id()))
}

/// Best-effort cleanup. A failure here is not reported: the caller is already
/// returning an error, and replacing "the install failed because X" with "cleanup
/// failed" would hide the reason the operator needs.
fn remove_quietly(p: &Path) {
    let _ = std::fs::remove_dir_all(p);
}

/// Copy a package tree. Symlinks are refused rather than followed, for the reason
/// `os_discovery` refuses a symlinked manifest: a package's identity is decided by
/// a file inside it, and a link lets that file live outside the tree being
/// validated.
fn copy_tree(src: &Path, dst: &Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(dst).map_err(|e| {
        InstallError::Filesystem(format!("could not create {}: {e}", dst.display()))
    })?;
    let entries = std::fs::read_dir(src)
        .map_err(|e| InstallError::Source(format!("could not read {}: {e}", src.display())))?;
    for e in entries {
        let e = e.map_err(|e| InstallError::Source(format!("could not read entry: {e}")))?;
        let ty = e
            .file_type()
            .map_err(|e| InstallError::Source(format!("could not stat entry: {e}")))?;
        let to = dst.join(e.file_name());
        if ty.is_symlink() {
            return Err(InstallError::Source(format!(
                "{} is a symlink; refusing to copy it",
                e.path().display()
            )));
        } else if ty.is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to).map_err(|err| {
                InstallError::Filesystem(format!("could not copy {}: {err}", e.path().display()))
            })?;
        }
    }
    Ok(())
}

/// Whether two paths sit on the same filesystem.
#[cfg(unix)]
fn same_device(a: &Path, b: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (std::fs::metadata(a).ok()?, std::fs::metadata(b).ok()?);
    Some(a.dev() == b.dev())
}

#[cfg(not(unix))]
fn same_device(_a: &Path, _b: &Path) -> Option<bool> {
    // No portable device id. Returning None means the caller's `unwrap_or(false)`
    // refuses the install rather than assuming atomicity it cannot verify.
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::collections::BTreeSet;

    // ---- the instrument, and its own negative controls -----------------------
    //
    // These tests exist because the "left nothing behind" assertions below are
    // only as good as this function. A snapshot that quietly sees nothing makes
    // "the tree is unchanged" pass for every possible bug.

    /// One entry in a filesystem snapshot: path, size, and a content hash.
    ///
    /// The hash is not redundant with the size. A half-written file can have the
    /// size the finished one would have had (a truncated copy that stopped on a
    /// block boundary, a file pre-allocated then partially filled), and without a
    /// hash "the set is equal" would hold across exactly the partial state this
    /// module exists to prevent. mtime is deliberately NOT in the tuple: it makes
    /// the snapshot unstable on fast filesystems, and the parent-mtime question is
    /// asked separately below.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Entry {
        rel: String,
        len: u64,
        sha: String,
    }

    fn snapshot(root: &Path) -> BTreeSet<Entry> {
        fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<Entry>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                let rel = p
                    .strip_prefix(base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .into_owned();
                let Ok(ty) = e.file_type() else { continue };
                if ty.is_dir() {
                    // Directories are recorded too: an empty leftover directory is
                    // exactly the kind of debris a size-only file scan misses.
                    out.insert(Entry {
                        rel: format!("{rel}/"),
                        len: 0,
                        sha: String::new(),
                    });
                    walk(base, &p, out);
                } else {
                    let bytes = std::fs::read(&p).unwrap_or_default();
                    use sha2::{Digest, Sha256};
                    let sha = format!("{:x}", Sha256::digest(&bytes));
                    out.insert(Entry {
                        rel,
                        len: bytes.len() as u64,
                        sha,
                    });
                }
            }
        }
        let mut out = BTreeSet::new();
        walk(root, root, &mut out);
        out
    }

    #[test]
    fn the_snapshot_sees_each_kind_of_difference() {
        // Three DIFFERENT bugs, so three separate assertions: missing a new file,
        // missing a same-size content change, and missing a leftover empty
        // directory are not one failure mode.
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        let base = snapshot(root);

        // (1) an extra file
        std::fs::write(root.join("b.txt"), b"x").unwrap();
        assert_ne!(base, snapshot(root), "an added file must be visible");
        std::fs::remove_file(root.join("b.txt")).unwrap();
        assert_eq!(
            base,
            snapshot(root),
            "and removing it must restore equality"
        );

        // (2) SAME SIZE, different content — the case a size-only snapshot misses,
        // and the shape a truncated-then-padded copy would take.
        std::fs::write(root.join("a.txt"), b"world").unwrap();
        assert_ne!(
            base,
            snapshot(root),
            "a same-size content change must be visible, or a partially written \
             file would pass as unchanged"
        );
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        assert_eq!(base, snapshot(root));

        // (3) an empty leftover directory — debris with no files in it at all
        std::fs::create_dir(root.join("leftover")).unwrap();
        assert_ne!(
            base,
            snapshot(root),
            "an empty leftover directory must be visible; a file-only walk sees \
             nothing here"
        );
    }

    // ---- fixtures -----------------------------------------------------------

    fn manifest_yaml(name: &str) -> String {
        format!(
            "name: {name}\nversion: \"0.1.0\"\nroute_namespace: /api/v1/{name}\n\
             event_module: {name}\ndata_dir: ~/.agent24/os/{name}/\n\
             kernel_capabilities: [events]\nimpl_kind: out_of_process_provider\n"
        )
    }

    /// A source package. `dir_name` is deliberately separate from the manifest
    /// name so tests can prove which one decides the installed identity.
    fn src_pkg(root: &Path, dir_name: &str, manifest_name: &str) -> PathBuf {
        let d = root.join(dir_name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MANIFEST_FILE), manifest_yaml(manifest_name)).unwrap();
        d
    }

    // ---- the happy path, which is also the CONTROL for every test below ------

    #[test]
    fn a_successful_install_changes_the_tree() {
        // Without this, "the tree is unchanged" in the failure tests is satisfiable
        // by an instrument that sees nothing at all. This is the positive control
        // for the whole file.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        let src = src_pkg(t.path(), "src", "cos72");

        let before = snapshot(&pkgs);
        let dest = install(&src, &pkgs).unwrap();
        let after = snapshot(&pkgs);

        assert_ne!(before, after, "a successful install must be visible");
        assert_eq!(dest, pkgs.join("cos72"));
        assert!(dest.join(MANIFEST_FILE).is_file());
    }

    #[test]
    fn the_installed_name_comes_from_the_manifest_not_the_directory() {
        // The manifest is the module's sole identity. Letting the enclosing
        // directory name decide would give a package two identities that can
        // disagree — and the one on disk is the one an operator would trust.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        let src = src_pkg(t.path(), "looks-like-sin90", "cos72");

        let dest = install(&src, &pkgs).unwrap();
        assert_eq!(dest.file_name().unwrap(), "cos72");
        assert!(!pkgs.join("looks-like-sin90").exists());
    }

    // ---- refusal leaves NOTHING behind ---------------------------------------

    #[test]
    fn an_invalid_manifest_leaves_the_packages_root_untouched() {
        // Asserting `is_err()` would say nothing about the disk. The claim is
        // about the filesystem, so the assertion is about the filesystem.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        // Pre-existing content, so "unchanged" is a stronger statement than "empty".
        let existing = src_pkg(&pkgs, "already", "already");

        let bad = t.path().join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join(MANIFEST_FILE), "name: [not, a, string]\n").unwrap();

        let before = snapshot(&pkgs);
        let err = install(&bad, &pkgs).unwrap_err();
        assert!(matches!(err, InstallError::Source(_)), "{err}");
        assert_eq!(
            before,
            snapshot(&pkgs),
            "a refused install must leave no trace"
        );
        assert!(
            existing.join(MANIFEST_FILE).is_file(),
            "and must not disturb what was there"
        );
    }

    #[test]
    fn a_failure_partway_through_the_copy_leaves_no_debris() {
        // Injection point notes, because the first two attempts were wrong:
        //
        //  - `chmod 0o555` does not work: root ignores permission bits, so under a
        //    root CI container the install would SUCCEED and the test would fail on
        //    the wrong assertion.
        //  - Pre-occupying the staging path does not work either: `install` clears
        //    a stale staging directory first (a crash must not wedge the next
        //    install), so the injection is removed before it can fire. That is the
        //    code behaving correctly; the test was wrong.
        //
        // What does work, and depends on neither privileges nor timing: a symlink
        // NESTED inside the package. `copy_tree` recurses, so the failure happens
        // after it has already created directories and copied at least one file.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        let existing = src_pkg(&pkgs, "already", "already");

        let src = src_pkg(t.path(), "src", "cos72");
        std::fs::create_dir_all(src.join("assets")).unwrap();
        std::fs::write(src.join("assets/ok.bin"), vec![7u8; 64]).unwrap();
        let outside = t.path().join("outside.txt");
        std::fs::write(&outside, b"not part of this package").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, src.join("assets/link.bin")).unwrap();

        let before = snapshot(&pkgs);
        let err = install(&src, &pkgs).unwrap_err();
        assert!(matches!(err, InstallError::Source(_)), "{err}");

        let after = snapshot(&pkgs);
        // The specific debris, named — so a failure says which invariant broke.
        assert!(
            !after.iter().any(|e| e.rel.starts_with(".staging")),
            "staging must be cleaned up: {after:#?}"
        );
        assert!(!pkgs.join("cos72").exists(), "no half-installed package");
        // And the general statement, which also covers debris nobody thought of.
        assert_eq!(
            before, after,
            "the packages root must be byte-for-byte unchanged"
        );
        assert!(existing.join(MANIFEST_FILE).is_file());
    }

    #[test]
    fn two_calls_never_produce_the_same_staging_path() {
        // Two concurrent installs of the SAME package must not share a staging
        // directory. Measured externally, sharing one still ends cleanly — but for
        // the wrong reason: the loser's tree is DELETED by the winner and the loser
        // then fails entirely. That guarantee is borrowed, and it disappears the
        // day this function can resume or retry.
        //
        // This asserts the path directly. An earlier version of this test asserted
        // "repeated installs leave no debris" instead, and a mutation that removed
        // the counter killed nothing — sequential calls clean up after themselves
        // either way, so the test held with and without the property.
        let root = Path::new("/tmp/whatever");
        let a = staging_path(root, "cos72");
        let b = staging_path(root, "cos72");
        assert_ne!(a, b, "the same package must not reuse a staging path");
        // And it must still be inside the packages root, or atomicity is gone.
        assert_eq!(a.parent(), Some(root));
        assert_eq!(b.parent(), Some(root));
    }

    #[test]
    fn installing_over_an_existing_package_is_refused_without_touching_it() {
        // The dangerous shape is not the refusal, it is a refusal that has already
        // clobbered the installed copy.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        let src = src_pkg(t.path(), "src", "cos72");
        install(&src, &pkgs).unwrap();
        // Mark the installed copy so a silent overwrite is detectable.
        std::fs::write(pkgs.join("cos72").join("MARKER"), b"original").unwrap();

        let before = snapshot(&pkgs);
        let err = install(&src, &pkgs).unwrap_err();
        assert!(matches!(err, InstallError::AlreadyInstalled(_)), "{err}");
        assert_eq!(
            before,
            snapshot(&pkgs),
            "the installed copy must be untouched"
        );
    }

    #[test]
    fn a_symlink_inside_the_package_is_refused() {
        // Same reason `os_discovery` refuses a symlinked manifest: a link lets part
        // of the package live outside the tree that was validated.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        let src = src_pkg(t.path(), "src", "cos72");
        let outside = t.path().join("secret.txt");
        std::fs::write(&outside, b"not yours").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, src.join("link.txt")).unwrap();

        let before = snapshot(&pkgs);
        let err = install(&src, &pkgs).unwrap_err();
        assert!(matches!(err, InstallError::Source(_)), "{err}");
        assert_eq!(before, snapshot(&pkgs), "and it must leave nothing behind");
    }

    #[test]
    fn uninstall_removes_it_and_reports_whether_there_was_anything() {
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        let src = src_pkg(t.path(), "src", "cos72");
        install(&src, &pkgs).unwrap();

        assert!(
            uninstall("cos72", &pkgs).unwrap(),
            "reports that it removed one"
        );
        assert!(!pkgs.join("cos72").exists());
        assert!(
            !uninstall("cos72", &pkgs).unwrap(),
            "removing something absent is not an error, but must be distinguishable"
        );
    }

    #[test]
    fn staging_lives_inside_the_packages_root_so_the_rename_is_atomic() {
        // The property is not "it works", it is WHERE staging is. `/tmp` is a
        // different volume on stock macOS, and a cross-device rename degrades to
        // copy-then-delete — not atomic, and the source of exactly the partial
        // state this module prevents. A test cannot observe atomicity directly, so
        // it observes the thing atomicity depends on.
        let t = tempfile::tempdir().unwrap();
        let pkgs = t.path().join("packages");
        std::fs::create_dir_all(&pkgs).unwrap();
        assert_eq!(
            same_device(&pkgs, &pkgs),
            Some(true),
            "the device check must actually work on this platform"
        );
        // The staging path is derived from the packages root, so it is on the same
        // device by construction. Pin that construction.
        let src = src_pkg(t.path(), "src", "cos72");
        install(&src, &pkgs).unwrap();
        assert_eq!(
            same_device(&pkgs.join("cos72"), &pkgs),
            Some(true),
            "the installed package must have landed on the packages root's device"
        );
    }
}
