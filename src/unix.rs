//! Implementation of `errno` functionality for Unix systems.
//!
//! Adapted from `src/libstd/sys/unix/os.rs` in the Rust distribution.
//!
//! Migrated off the `libc` crate for werewolfdb. The C symbols still come from the system libc
//! at link time — that is what an errno crate is for — but the *crate* is gone, which is what
//! `cargo tree --invert libc` measures and what `MIGRATE-DEPENDENCIES.MD` is about. `rustix` is
//! not the replacement here and cannot be: cargo rejects `errno -> rustix` as a dependency cycle,
//! since rustix depends on errno, and rustix has no `strerror_r` in any case. See the `## errno`
//! section of `MIGRATION-PLAN.md`.

// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::ffi::{c_char, c_int};
use core::str;

use crate::Errno;

// `ERANGE` is 34 on Linux and Android, and on every BSD, Apple, Solaris, AIX and NTO target —
// but not everywhere: it is 68 on emscripten and wasi, 38 on vxworks, 1073741858 on hurd and
// -2147454959 on haiku. This fork is built and tested on Linux only, so rather than carry a
// table `libc` used to maintain, the targets whose value differs refuse to compile. A wrong
// constant here would silently mis-handle the buffer-too-small path; a build failure will not.
#[cfg(any(
    target_os = "emscripten",
    target_os = "haiku",
    target_os = "hurd",
    target_os = "vxworks",
))]
compile_error!(
    "this fork of `errno` defines ERANGE for Linux; \
     see MIGRATION-PLAN.md before adding a target whose value differs"
);

/// The value of `ERANGE` on the targets this crate is built for.
const ERANGE: c_int = 34;

/// Decides what a `strerror_r` status means, given the current `errno`.
///
/// Zero is success. A negative status is glibc older than 2.13, which reported failure that way and left the
/// reason in `errno`. `ERANGE` says only that the buffer was too small, and a truncated message
/// is more use than an error, so it is not a failure to report.
///
/// Separated from the call because neither interesting case can be produced on a current system:
/// no glibc in use returns a negative status, and no message approaches the 1024-byte buffer.
/// Logic that cannot be reached where it runs is still logic, and this is where it is tested.
fn describe_failure(rc: c_int, current: Errno) -> Option<Errno> {
    if rc == 0 {
        return None;
    }
    let reported = if rc < 0 { current } else { Errno(rc) };
    (reported != Errno(ERANGE)).then_some(reported)
}

/// The length of a NUL-terminated message held in a fixed buffer.
///
/// This replaces the `strlen` call the `libc` crate supplied. The length is already bounded by
/// the buffer, which is the one thing `strlen` could not assume — so where C had to trust the
/// terminator, this cannot read past the end whether one is there or not. A buffer with no NUL
/// cannot come back from a call that succeeded; taking all of it is the harmless reading if one
/// ever did, and separating it is what makes that claim testable.
fn message_len(buf: &[u8]) -> usize {
    buf.iter().position(|&byte| byte == 0).unwrap_or(buf.len())
}

fn from_utf8_lossy(input: &[u8]) -> &str {
    match str::from_utf8(input) {
        Ok(valid) => valid,
        // SAFETY: `valid_up_to()` is the length of the longest prefix that is valid UTF-8, so
        // the slice up to it is valid by construction.
        Err(error) => unsafe { str::from_utf8_unchecked(&input[..error.valid_up_to()]) },
    }
}

pub fn with_description<F, T>(err: Errno, callback: F) -> T
where
    F: FnOnce(Result<&str, Errno>) -> T,
{
    let mut buf = [0u8; 1024];
    // SAFETY: `strerror_r` writes at most `buflen` bytes into `buf`, and `buf` is a live local
    // array of exactly that length. The pointer is valid for the whole call, and nothing else
    // borrows the buffer while it runs.
    let rc = unsafe { strerror_r(err.0, buf.as_mut_ptr().cast::<c_char>(), buf.len()) };
    if let Some(failed) = describe_failure(rc, errno()) {
        return callback(Err(failed));
    }
    // The message is NUL-terminated, so the NUL is where it ends. Searching the buffer in safe
    // Rust replaces the `strlen` call: the length is already bounded by `buf.len()`, which is
    // the one thing `strlen` could not assume. A buffer with no NUL cannot arise from a call
    // that succeeded, and taking all of it is the harmless reading if it ever did.
    callback(Ok(from_utf8_lossy(&buf[..message_len(&buf)])))
}

pub const STRERROR_NAME: &str = "strerror_r";

pub fn errno() -> Errno {
    // SAFETY: `errno_location` returns a pointer to the calling thread's `errno`, which the C
    // library guarantees is valid and correctly aligned for the life of that thread.
    unsafe { Errno(*errno_location()) }
}

pub fn set_errno(Errno(errno): Errno) {
    // SAFETY: as above — the pointer is the thread's own `errno`, and is valid to write.
    unsafe {
        *errno_location() = errno;
    }
}

extern "C" {
    // glibc ships two `strerror_r`s, and the plain symbol is the GNU one, which returns
    // `char *` rather than `int`. Binding that by accident compiles and links, and is silently
    // wrong: the returned pointer would be read as a status, so `rc != 0` becomes almost always
    // true and `Errno(rc)` becomes a truncated address. The XSI-conformant function is exposed
    // as `__xpg_strerror_r`. musl and OpenHarmony provide only the XSI behaviour under the plain
    // name. This is what the `libc` crate does; dropping the crate does not drop the problem it
    // was solving.
    #[cfg_attr(
        not(any(target_env = "musl", target_env = "ohos")),
        link_name = "__xpg_strerror_r"
    )]
    fn strerror_r(errnum: c_int, buf: *mut c_char, buflen: usize) -> c_int;

    #[cfg_attr(
        any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "visionos",
            target_os = "freebsd"
        ),
        link_name = "__error"
    )]
    #[cfg_attr(
        any(
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "android",
            target_os = "espidf",
            target_os = "vxworks",
            target_os = "cygwin",
            target_env = "newlib"
        ),
        link_name = "__errno"
    )]
    #[cfg_attr(
        any(target_os = "solaris", target_os = "illumos"),
        link_name = "___errno"
    )]
    #[cfg_attr(target_os = "haiku", link_name = "_errnop")]
    #[cfg_attr(
        any(
            target_os = "linux",
            target_os = "hurd",
            target_os = "redox",
            target_os = "dragonfly",
            target_os = "emscripten",
        ),
        link_name = "__errno_location"
    )]
    #[cfg_attr(target_os = "aix", link_name = "_Errno")]
    #[cfg_attr(target_os = "nto", link_name = "__get_errno_ptr")]
    fn errno_location() -> *mut c_int;
}

#[cfg(test)]
mod tests {
    use super::{describe_failure, from_utf8_lossy, message_len, Errno, ERANGE};

    /// Zero is success; a positive status is the reason itself; a negative one means the reason is in `errno`,
    /// which is how glibc before 2.13 reported failure. `ERANGE` alone is not a failure — it
    /// says the buffer was too small, and a truncated message still describes the error.
    #[test]
    fn a_status_is_read_as_a_reason_or_as_truncation() {
        assert_eq!(describe_failure(0, Errno(99)), None, "zero is success");
        assert_eq!(
            describe_failure(22, Errno(99)),
            Some(Errno(22)),
            "a positive status is the reason"
        );
        assert_eq!(
            describe_failure(-1, Errno(22)),
            Some(Errno(22)),
            "a negative status leaves the reason in errno"
        );
        assert_eq!(
            describe_failure(ERANGE, Errno(99)),
            None,
            "the buffer was too small, which is not a failure to report"
        );
        assert_eq!(
            describe_failure(-1, Errno(ERANGE)),
            None,
            "...however it was reported"
        );
    }

    /// A message ends at its NUL. Without one it ends at the buffer, which is the difference
    /// between this and the `strlen` it replaces: C would have read on.
    #[test]
    fn a_message_ends_at_its_nul_or_at_the_buffer() {
        assert_eq!(message_len(b"ok\0rest"), 2);
        assert_eq!(message_len(b"\0"), 0);
        assert_eq!(message_len(b"no terminator"), 13);
        assert_eq!(message_len(b""), 0);
    }

    /// A message the C library returned that is not valid UTF-8 is truncated at the last
    /// character that was, rather than rejected: a mangled description is still more use
    /// than none, and no locale is guaranteed to hand back UTF-8.
    ///
    /// Exercised directly because `strerror_r` cannot be made to produce one on demand.
    #[test]
    fn a_description_that_is_not_utf8_keeps_the_part_that_is() {
        assert_eq!(from_utf8_lossy(b"ok"), "ok");
        assert_eq!(from_utf8_lossy(b"half\xff\xfe"), "half");
        assert_eq!(from_utf8_lossy(b"\xff"), "");
        assert_eq!(from_utf8_lossy(b""), "");
    }
}
