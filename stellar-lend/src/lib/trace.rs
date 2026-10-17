//! Call tracing and profiling utilities

use std::time::Instant;

/// Track the execution of a function call
pub struct CallTracker {
    start: Instant,
    name: String,
}

impl CallTracker {
    /// Start tracking a function call
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    /// Print the trace with timing
    pub fn finish(self) {
        let elapsed = self.elapsed_ms();
        println!("✓ {} completed in {}ms", self.name, elapsed);
    }

    /// Print with custom message
    pub fn finish_with(self, message: impl std::fmt::Display) {
        let elapsed = self.elapsed_ms();
        println!("✓ {}: {} ({}ms)", self.name, message, elapsed);
    }
}
