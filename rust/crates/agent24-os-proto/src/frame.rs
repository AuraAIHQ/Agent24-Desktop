//! ME-3b-1 — reading one NDJSON frame, with a bound on its size.
//!
//! One line of the callback channel is one frame. This module reads exactly one
//! and hands back its bytes; it does not know that the bytes are JSON, and it
//! must not learn — the whole point of the bound is that it applies BEFORE
//! anything parses.
//!
//! # The property, and why it is not "it returns an error"
//!
//! An over-long frame must be refused **without consuming its payload**. A reader
//! that buffers the whole line and then measures it returns exactly the same
//! error and passes exactly the same `is_err()` test, while giving a module an
//! unbounded write into the kernel's address space. So the tests do not measure
//! the return value alone; they measure HOW MUCH WAS CONSUMED, through a wrapper
//! that counts (see `CountingBufRead` in the tests).
//!
//! ## What this module can and cannot promise — the difference is the caller's
//!
//! An earlier version of this comment said the payload is refused "before it is
//! in memory". **That is not true and a test caught it.** This module consumes at
//! most `MAX_FRAME_BYTES + 1`, but what the TRANSPORT has already pulled into a
//! buffer is decided by the `BufRead` it is handed: a `BufReader` built with an
//! 8 MiB capacity will read 8 MiB from the socket to answer one `fill_buf`, and
//! nothing here can stop it.
//!
//! So there are two properties, and only the first belongs to this module:
//!
//! 1. **`read_frame` consumes at most `MAX_FRAME_BYTES + 1`.** Always, whatever
//!    it is handed. Tested directly.
//! 2. **The transport is not drained by an over-long frame.** True only if the
//!    caller's buffer is bounded — a REQUIREMENT ON ME-3b-3, which constructs the
//!    reader over the child's pipe. `BufReader::new`'s default (8 KiB) satisfies
//!    it; `with_capacity(huge)` does not. Recorded here because the bound is
//!    worthless if the layer above quietly undoes it, and because nothing in this
//!    crate can enforce it.
//!
//! # This module has no production caller yet
//!
//! Its consumer is ME-3b-2b (the `initialize` wire shape). 🟢 library-only in
//! SPEC-MD-ME's sense — **not ✅**. As with `version`, the condition for removing
//! it from `main` is an EVENT, not a date: if 3b-2b is abandoned or routed
//! around, this leaves with it. A deadline in days would fire for a scheduling
//! reason rather than for the reason this expiry exists, and an expiry that fires
//! for the wrong reason gets extended — after which it is a comment, not an
//! expiry.

use std::io::BufRead;

/// The largest frame this kernel will read, payload only (the newline is not
/// counted). Printed in the error so an operator does not have to read this file
/// to learn the policy.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Why a frame could not be read.
///
/// Typed, not `String`, and this is a load-bearing choice rather than style: the
/// rule "any failure during the handshake disconnects" lives in ME-3b-3, which
/// owns the connection — this module does not. If it handed up a string, 3b-3
/// could only disconnect uniformly, and the operator's log could not tell
/// "frame too long" from "incompatible version". That is the shape of the #159
/// gate-6 story: a real distinction pressed into a shared expression.
#[derive(Debug)]
pub enum FrameError {
    /// The payload exceeded [`MAX_FRAME_BYTES`]. Carries the limit, not the
    /// actual size — the actual size is not known, because the read stopped.
    /// Reporting a size here would mean having consumed the thing this refusal
    /// exists to avoid consuming.
    TooLong { limit: usize },
    /// The peer closed the connection cleanly, mid-frame or before one started.
    /// Separate from an I/O error: for the caller, "they hung up" and "the socket
    /// broke" lead to different logs and, in 3b-3, to different restart
    /// accounting.
    Eof,
    /// The transport failed.
    Io(std::io::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong { limit } => {
                write!(
                    f,
                    "frame exceeds the {limit}-byte limit and was refused unread"
                )
            }
            Self::Eof => f.write_str("the peer closed the connection"),
            Self::Io(e) => write!(f, "transport error: {e}"),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Read one NDJSON frame: everything up to the next `\n`, with the newline
/// consumed and not returned.
///
/// Reads at most `MAX_FRAME_BYTES + 1` bytes of payload. The `+ 1` is what makes
/// the refusal decidable: a frame of exactly the limit is legal, so the reader
/// has to see one more byte to know the limit was passed. Nothing beyond that is
/// read — in particular, the rest of the over-long line is NOT drained. Draining
/// it would mean reading an unbounded amount of attacker-chosen data to be tidy,
/// which is the thing being refused. The connection is not recoverable after
/// this error, and it is 3b-3's job to close it.
///
/// A trailing final line without a newline is Eof, not a frame: `\n` is the
/// delimiter, and treating "the stream ended" as "the frame ended" would let a
/// truncated frame be parsed as a complete one.
///
/// # Errors
///
/// [`FrameError::TooLong`], [`FrameError::Eof`], [`FrameError::Io`].
pub fn read_frame(src: &mut impl BufRead) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    loop {
        // The bound is checked at the TOP, before anything is read, and this
        // placement is what makes the loop terminate rather than a happy accident.
        //
        // It used to be at the bottom, next to the append. Then `room` (below)
        // could evaluate to 0, the window would be empty, no newline would be
        // found in it, `consume(0)` would make no progress, and the loop would
        // spin forever — while a mutation test looked simply "slow". Termination
        // depended on two separate expressions agreeing about the limit; now it
        // depends on one. With the check here, `room` is at least 1 on every
        // iteration, so every iteration either finds the newline, consumes at
        // least one byte, or returns.
        //
        // The same move removes an underflow: `MAX + 1 - out.len()` panics in
        // debug and WRAPS in release once `out.len()` passes `MAX + 1`, and a
        // wrapped `room` is an unbounded read — the exact failure this function
        // exists to prevent, in the arithmetic meant to prevent it.
        if out.len() > MAX_FRAME_BYTES {
            return Err(FrameError::TooLong {
                limit: MAX_FRAME_BYTES,
            });
        }
        let available = match src.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        };
        if available.is_empty() {
            return Err(FrameError::Eof);
        }
        // Only ever look at as much as could still be legal. `fill_buf` may hand
        // over a megabyte; consuming all of it because it happened to be offered
        // is how the bound stops being a bound.
        let room = MAX_FRAME_BYTES + 1 - out.len();
        let window = &available[..available.len().min(room)];
        match window.iter().position(|&b| b == b'\n') {
            Some(i) => {
                out.extend_from_slice(&window[..i]);
                src.consume(i + 1);
                return Ok(out);
            }
            None => {
                let taken = window.len();
                // Guaranteed non-zero by the check at the top of the loop, and
                // asserted rather than assumed: if this ever became 0 the symptom
                // would be a hang, which is the hardest thing to see in a test run.
                debug_assert!(taken > 0, "no progress: the loop would spin");
                out.extend_from_slice(window);
                src.consume(taken);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::{BufReader, Cursor, Read};

    /// The INSTRUMENT for this module's central claim, and therefore the thing
    /// that gets checked before anything is measured with it.
    ///
    /// Two DIFFERENT quantities are counted, and keeping them apart is the point.
    /// This type counts the second; `Counted` below counts the first:
    ///
    /// - `Counted::consumed` — bytes `read_frame` took (its `consume` calls).
    ///   This is what this module controls, and what its bound is about.
    /// - `CountingBufRead::pulled` — bytes the transport handed up. Decided by
    ///   the buffer's capacity, i.e. by the CALLER, not by `read_frame`.
    ///
    /// The first version of these tests measured only `pulled` and asserted the
    /// transport was barely touched. That assertion failed on a large-capacity
    /// buffer — correctly: `read_frame` had consumed 1 MiB + 1 while `BufReader`
    /// had pulled 4 MiB into its own buffer. The failing test was right and the
    /// claim it was checking was too strong; see the module docs.
    struct CountingBufRead<R> {
        inner: R,
        pulled: usize,
    }

    impl<R: Read> Read for CountingBufRead<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.pulled += n;
            Ok(n)
        }
    }

    /// Wraps a `BufReader` so `consume` can be counted. `fill_buf` is passed
    /// straight through — the buffer underneath decides how much to pull, which
    /// is exactly the distinction being preserved.
    struct Counted<R> {
        inner: BufReader<CountingBufRead<R>>,
        consumed: usize,
    }

    impl<R: Read> Read for Counted<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inner.read(buf)
        }
    }

    impl<R: Read> BufRead for Counted<R> {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            self.inner.fill_buf()
        }
        fn consume(&mut self, amt: usize) {
            self.consumed += amt;
            self.inner.consume(amt);
        }
    }

    /// A frame source whose buffer holds at most `capacity` bytes.
    ///
    /// The capacity matters in both directions: too small and `fill_buf` never
    /// offers enough to expose a reader that over-consumes; large enough to hold
    /// the whole stream and the transport counter stops being able to say
    /// anything about this module at all.
    fn source(data: &[u8], capacity: usize) -> Counted<Cursor<Vec<u8>>> {
        Counted {
            inner: BufReader::with_capacity(
                capacity,
                CountingBufRead {
                    inner: Cursor::new(data.to_vec()),
                    pulled: 0,
                },
            ),
            consumed: 0,
        }
    }

    fn pulled<R>(s: &Counted<R>) -> usize {
        s.inner.get_ref().pulled
    }

    /// The instrument's own positive control — BOTH counters, because a stuck
    /// zero in either one makes a different set of assertions below vacuous.
    #[test]
    fn the_counters_actually_count() {
        let mut src = source(b"abc\n", 64);
        let frame = read_frame(&mut src).unwrap();
        assert_eq!(frame, b"abc");
        assert_eq!(
            src.consumed, 4,
            "the consume counter is stuck; every bound assertion here would be vacuous"
        );
        assert_eq!(
            pulled(&src),
            4,
            "the transport counter is stuck; the drain assertions would be vacuous"
        );
    }

    /// The criterion, in the two halves it needs.
    ///
    /// Halves, because either alone is satisfied by something broken: an
    /// implementation that reads NOTHING passes "at most N+1", and one that reads
    /// everything passes "a legal frame of exactly N works". (Review, PR-Daemon:
    /// "只有后者的话，一个恒返回 0 的计数器也能过".)
    #[test]
    fn an_over_long_frame_is_refused_without_consuming_its_payload() {
        // A frame far larger than the limit, followed by more traffic — so a
        // reader that drained the rest of the line would be visible in the count.
        let mut data = vec![b'x'; MAX_FRAME_BYTES * 4];
        data.push(b'\n');
        data.extend_from_slice(b"next\n");

        // capacity 1 is a progress stress: `fill_buf` offers a single byte at a
        // time, so any iteration that fails to consume would hang rather than
        // fail. It is here because a mutation that moved the bound check DID
        // hang, and a hang is the one outcome a test run reports as silence.
        for capacity in [1, 64, 8 * 1024, MAX_FRAME_BYTES * 8] {
            let mut src = source(&data, capacity);
            let err = read_frame(&mut src).expect_err("must refuse");
            assert!(matches!(err, FrameError::TooLong { .. }), "{err:?}");
            // Property 1 — this module's, and it holds for EVERY capacity,
            // including one large enough to offer the whole 4 MiB in a single
            // `fill_buf`. That last case is the one that matters: it is where a
            // reader that consumed whatever it was offered would be caught.
            assert!(
                src.consumed <= MAX_FRAME_BYTES + 1,
                "capacity={capacity}: consumed {} bytes for a refused frame",
                src.consumed
            );
            // Property 2 — NOT this module's. Whether the transport was drained
            // depends on the buffer it was handed, so it is asserted only for the
            // bounded capacities, and the large one is asserted to VIOLATE it.
            // Writing the violation down is what keeps this from looking like a
            // property `read_frame` provides: it is a requirement on ME-3b-3.
            if capacity <= 8 * 1024 {
                assert!(
                    pulled(&src) <= MAX_FRAME_BYTES + 1 + capacity,
                    "capacity={capacity}: transport drained of {} bytes",
                    pulled(&src)
                );
            } else {
                assert!(
                    pulled(&src) > MAX_FRAME_BYTES * 2,
                    "capacity={capacity}: expected the oversized buffer to have pulled \
                     far more than the limit — if it no longer does, the caveat in the \
                     module docs is stale and should be revisited, not deleted"
                );
            }
        }

        // The other half: a frame of EXACTLY the limit is legal and is read in
        // full. Without this, an implementation that refuses everything — or one
        // whose counter is stuck — passes the assertions above.
        let mut legal = vec![b'y'; MAX_FRAME_BYTES];
        legal.push(b'\n');
        let mut src = source(&legal, 8 * 1024);
        let frame = read_frame(&mut src).expect("a frame of exactly the limit is legal");
        assert_eq!(frame.len(), MAX_FRAME_BYTES);
        assert_eq!(
            src.consumed,
            legal.len(),
            "a legal frame must be consumed in full, newline included"
        );
    }

    #[test]
    fn frames_are_returned_one_at_a_time_without_the_newline() {
        let mut src = source(b"{\"a\":1}\n{\"b\":2}\n", 64);
        assert_eq!(read_frame(&mut src).unwrap(), b"{\"a\":1}");
        assert_eq!(read_frame(&mut src).unwrap(), b"{\"b\":2}");
        assert!(matches!(read_frame(&mut src).unwrap_err(), FrameError::Eof));
    }

    #[test]
    fn an_empty_frame_is_a_frame_not_an_end() {
        // A blank line is a legal NDJSON line that happens to parse as nothing.
        // Returning Eof for it would silently end a connection the peer thinks is
        // open; the parse layer above is where "this is not an object" belongs.
        let mut src = source(b"\nx\n", 64);
        assert_eq!(read_frame(&mut src).unwrap(), b"");
        assert_eq!(read_frame(&mut src).unwrap(), b"x");
    }

    #[test]
    fn a_final_line_without_a_newline_is_not_a_frame() {
        // The delimiter is `\n`, not "the stream ended". Accepting the tail as a
        // frame would let a TRUNCATED frame — a peer killed mid-write — be handed
        // up as a complete one, and the layer above has no way to tell.
        let mut src = source(b"complete\nhalf", 64);
        assert_eq!(read_frame(&mut src).unwrap(), b"complete");
        assert!(matches!(read_frame(&mut src).unwrap_err(), FrameError::Eof));
    }

    #[test]
    fn the_bytes_are_returned_untouched() {
        // Not a JSON reader: invalid UTF-8 and embedded control bytes come back
        // as they arrived. Validating here would put a second, weaker parser in
        // front of the real one.
        let mut src = source(b"\xff\xfe\x00 raw\n", 64);
        assert_eq!(read_frame(&mut src).unwrap(), b"\xff\xfe\x00 raw");
    }

    /// The error must say which of the three things happened — that is what
    /// `FrameError` being typed is FOR (3b-3 owns the connection and needs to log
    /// the distinction it cannot otherwise see).
    #[test]
    fn the_three_failures_are_distinguishable_and_the_limit_is_stated() {
        let too_long = FrameError::TooLong {
            limit: MAX_FRAME_BYTES,
        };
        assert!(too_long.to_string().contains(&MAX_FRAME_BYTES.to_string()));
        let eof = FrameError::Eof;
        let io = FrameError::Io(std::io::Error::other("boom"));
        let kinds = [&too_long, &eof, &io].map(std::mem::discriminant);
        assert_ne!(kinds[0], kinds[1]);
        assert_ne!(kinds[1], kinds[2]);
        assert_ne!(kinds[0], kinds[2]);
    }
}
