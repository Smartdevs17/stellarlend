//! Performance characterization for the error-code-to-category lookup.
//!
//! The hot path of the framework is `lending_code_to_core(code) -> CoreError`
//! which is called once per failed contract invocation to log + tally. The
//! function is a `match` over 35 arms and must be fast enough to run on every
//! failed call without adding noticeable overhead.
//!
//! In a `#![no_std]` Soroban context we cannot measure wall-clock time, so
//! the test here:
//! 1. Asserts the API is total (every code 1..=35 returns a defined category).
//! 2. Runs a deterministic iteration count and asserts the function never
//!    panics. The actual throughput number is captured in the `gas-baseline`
//!    benchmarks crate (off-chain host).
//!
//! Run with: `cargo test -p stellarlend-errors --release benchmark`

#![cfg(test)]

use crate::mapping::lending_code_to_core;

const ITERATIONS: u32 = 100_000;

#[test]
fn benchmark_lookup_is_total_and_panic_free() {
    // Warm up: ensure first-time path doesn't dominate.
    for code in 1u32..=35 {
        let _ = lending_code_to_core(code).unwrap();
    }

    let mut sink: u32 = 0;
    for i in 0..ITERATIONS {
        let code = (i % 35) + 1;
        let core = lending_code_to_core(code).expect("all codes 1..=35 must map");
        sink = sink.wrapping_add(core as u32);
    }
    // The sink prevents the optimizer from eliding the loop. The exact value
    // is irrelevant — we just want the work to be observable to the optimizer
    // so a future change that breaks the API triggers a recompile.
    assert!(sink > 0);
}

#[test]
fn benchmark_full_chain_terminates() {
    let codes: [u32; 35] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    ];
    let mut sink: u32 = 0;
    for _ in 0..ITERATIONS {
        for &c in &codes {
            let core = lending_code_to_core(c).expect("mapped");
            sink = sink.wrapping_add(core as u32);
        }
    }
    assert!(sink > 0);
}

#[test]
fn benchmark_throughput_smoke() {
    // The function does exactly one `match` per call. On a host CPU a single
    // call is sub-microsecond; we don't have a wall clock here, but we can
    // at least assert the function completes a large batch without panicking
    // and returns a consistent answer on repeated calls.
    for _ in 0..10 {
        for code in 1u32..=35 {
            let first = lending_code_to_core(code);
            let second = lending_code_to_core(code);
            assert_eq!(first, second, "lookup must be deterministic");
        }
    }
}
