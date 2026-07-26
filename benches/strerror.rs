//! Measures the migrated `strerror_r` path against the one it replaced.
//!
//! The migration removed the `libc` crate: `c_int` and `size_t` became `core::ffi` types, `strlen`
//! became a bounded search, `ERANGE` became a constant, and `strerror_r` became an `extern "C"`
//! declaration carrying the `__xpg_strerror_r` link name. None of that should cost anything — the
//! same C function is called either way — but "should" is not a measurement, and the original's
//! numbers stop existing the moment the code is replaced.
//!
//! So both implementations run here, in the same harness, on the same machine, in the same run.
//! `original` is the pre-migration code, reaching `strerror_r` through the `libc` crate — which is
//! why `libc` is a dev-dependency and a declared `test-exemption` in `AGENTS.md`. That is the
//! exemption's whole purpose: the original has to stay runnable to be compared against
//! (capability `migration-benchmarks`).
//!
//! Both sides render through `Display`, because that is the only way into the migrated
//! implementation from outside the crate and the two must be doing the same work to be compared.
//! Measuring `Errno::to_string` against a bare function call would have reported the formatter's
//! cost as a migration regression — it read as 1.8x slower before the original was given the same
//! surface.

// Rust guideline compliant 2026-07-24

use criterion::{criterion_group, criterion_main, Criterion};
use errno::Errno;

/// The pre-migration implementation, reaching the C library through the `libc` crate.
mod original {
    use core::fmt;
    use libc::{c_int, size_t, strerror_r, strlen};

    /// The crate's error code as it was before the migration, with the same `Display` surface.
    pub struct Errno(pub c_int);

    impl fmt::Display for Errno {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut buf = [0u8; 1024];
            // SAFETY: `strerror_r` writes at most `buflen` bytes into `buf`, which is a live local
            // array of that length. This mirrors the pre-migration code, so the comparison is
            // between the two implementations rather than between two different programs.
            let described = unsafe {
                let rc = strerror_r(self.0, buf.as_mut_ptr().cast(), buf.len() as size_t);
                if rc != 0 {
                    return write!(fmt, "OS error {} (strerror_r returned error {rc})", self.0);
                }
                let len = strlen(buf.as_ptr().cast());
                String::from_utf8_lossy(&buf[..len]).into_owned()
            };
            fmt.write_str(&described)
        }
    }
}

/// Runs both implementations over the same codes.
fn strerror(c: &mut Criterion) {
    // A code every system describes, and one nothing does — the second takes the failure path,
    // which is the half of the function whose shape the migration actually changed.
    let codes = [1, i32::MAX];

    let mut group = c.benchmark_group("strerror_r");
    group.bench_function("migrated", |b| {
        b.iter(|| {
            for code in codes {
                let _ = std::hint::black_box(Errno(code).to_string());
            }
        });
    });
    group.bench_function("original_libc", |b| {
        b.iter(|| {
            for code in codes {
                let _ = std::hint::black_box(original::Errno(code).to_string());
            }
        });
    });
    group.finish();
}

criterion_group!(benches, strerror);
criterion_main!(benches);
