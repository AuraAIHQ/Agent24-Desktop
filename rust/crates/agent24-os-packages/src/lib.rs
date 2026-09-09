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
