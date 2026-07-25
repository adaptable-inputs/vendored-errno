//! Cross-platform interface to the `errno` variable.
//!
//! # Examples
//! ```
//! use errno::{Errno, errno, set_errno};
//!
//! // Get the current value of errno
//! let e = errno();
//!
//! // Set the current value of errno
//! set_errno(e);
//!
//! // Extract the error code as an i32
//! let code = e.0;
//!
//! // Display a human-friendly error message
//! println!("Error {}: {}", code, e);
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(target_os = "wasi", path = "wasi.rs")]
#[cfg_attr(target_os = "hermit", path = "hermit.rs")]
mod sys;

use core::fmt;
#[cfg(feature = "std")]
use std::error::Error;
#[cfg(feature = "std")]
use std::io;

/// Wraps a platform-specific error code.
///
/// The `Display` instance maps the code to a human-readable string. It
/// calls [`strerror_r`][1] under POSIX, and [`FormatMessageW`][2] on
/// Windows.
///
/// [1]: http://pubs.opengroup.org/onlinepubs/009695399/functions/strerror.html
/// [2]: https://msdn.microsoft.com/en-us/library/windows/desktop/ms679351%28v=vs.85%29.aspx
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Errno(pub i32);

impl fmt::Debug for Errno {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        sys::with_description(*self, |desc| {
            fmt.debug_struct("Errno")
                .field("code", &self.0)
                .field("description", &desc.ok())
                .finish()
        })
    }
}

impl fmt::Display for Errno {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        sys::with_description(*self, |desc| match desc {
            Ok(desc) => fmt.write_str(desc),
            Err(fm_err) => write!(
                fmt,
                "OS error {} ({} returned error {})",
                self.0,
                sys::STRERROR_NAME,
                fm_err.0
            ),
        })
    }
}

impl From<Errno> for i32 {
    fn from(e: Errno) -> Self {
        e.0
    }
}

#[cfg(feature = "std")]
impl Error for Errno {
    // TODO: Remove when MSRV >= 1.27
    #[allow(deprecated)]
    fn description(&self) -> &str {
        "system error"
    }
}

#[cfg(feature = "std")]
impl From<Errno> for io::Error {
    fn from(errno: Errno) -> Self {
        io::Error::from_raw_os_error(errno.0)
    }
}

/// Returns the platform-specific value of `errno`.
pub fn errno() -> Errno {
    sys::errno()
}

/// Sets the platform-specific value of `errno`.
pub fn set_errno(err: Errno) {
    sys::set_errno(err)
}

/// The value round-trips through the platform's `errno`, which is the crate's whole job.
#[test]
fn it_works() {
    let x = errno();
    set_errno(x);
}

/// Rendering goes through `strerror_r`; that it produces anything at all is the smoke test.
#[cfg(feature = "std")]
#[test]
fn it_works_with_to_string() {
    let x = errno();
    let _ = x.to_string();
}

/// The description for errno 1 is the one glibc gives, which is also the proof that the
/// XSI `__xpg_strerror_r` is what got linked: the GNU function of the same name returns
/// `char *`, so binding it would make the status non-zero and take the error path instead.
///
/// The upstream test branched over seven platforms; this fork is built for Linux, so it
/// asserts the Linux answer rather than carrying six arms no build here can reach.
#[cfg(feature = "std")]
#[test]
fn check_description() {
    set_errno(Errno(1));
    assert_eq!(errno().to_string(), "Operation not permitted");
    assert_eq!(
        format!("{:?}", errno()),
        "Errno { code: 1, description: Some(\"Operation not permitted\") }"
    );
}

/// A code the C library has no message for takes the failure path: `strerror_r` reports
/// `EINVAL`, which is neither zero nor `ERANGE`, so the description is an error and
/// `Display` falls back to naming the function that refused it.
#[cfg(feature = "std")]
#[test]
fn an_unknown_code_reports_the_function_that_refused_it() {
    let rendered = Errno(i32::MAX).to_string();
    assert!(
        rendered.starts_with(&format!(
            "OS error {} (strerror_r returned error ",
            i32::MAX
        )),
        "{rendered}"
    );
    let debug = format!("{:?}", Errno(i32::MAX));
    assert!(
        debug.contains("description: None"),
        "an unknown code has no description: {debug}"
    );
}

/// The code is the value, in both directions.
#[test]
fn a_code_converts_to_and_from_its_integer() {
    assert_eq!(i32::from(Errno(7)), 7);
    assert_eq!(Errno(7).0, 7);
}

/// The `Error` impl carries the deprecated `description`, which is still part of the
/// surface this fork must keep identical to the crate it replaces.
#[cfg(feature = "std")]
#[test]
#[allow(deprecated)]
fn the_error_impl_describes_itself() {
    use std::error::Error as _;
    assert_eq!(Errno(1).description(), "system error");
}

/// An `io::Error` built from a code keeps that code's kind.
#[cfg(feature = "std")]
#[test]
fn check_error_into_errno() {
    const ERROR_CODE: i32 = 1;

    let error = io::Error::from_raw_os_error(ERROR_CODE);
    let new_error: io::Error = Errno(ERROR_CODE).into();
    assert_eq!(error.kind(), new_error.kind());
}
