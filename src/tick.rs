// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;


// STD Dependencies -----------------------------------------------------------
use std::cmp;
use std::thread;
use std::time::Duration;


// Internal Dependencies ------------------------------------------------------
use super::Config;


// Tick Rate Limiting ---------------------------------------------------------
pub fn start() -> u64 {
    clock_ticks::precise_time_ns()
}

pub fn end(
    tick_delay: u64,
    tick_start: u64,
    overflow: &mut u64,
    config: &Config
) {

    // Actual time taken by the tick
    let time_taken = clock_ticks::precise_time_ns() - tick_start;

    // Required delay reduction to keep tick rate
    let mut reduction = cmp::min(time_taken, tick_delay);

    if config.tick_overflow_recovery {

        // Keep track of how much additional time the current tick required
        *overflow += time_taken - reduction;

        // Try to reduce the existing overflow by reducing the reduction time
        // for the current frame.
        let max_correction = (tick_delay - reduction) as i64;
        let correction = cmp::min(
            (max_correction as f32 * config.tick_overflow_recovery_rate) as i64,
            max_correction
        );

        // This way we'll achieve a speed up effect in an effort to keep the
        // desired tick rate stable over a longer period of time
        let reduced_overflow = cmp::max(0, *overflow as i64 - correction) as u64;

        // Adjust the reduction amount to speed up
        reduction += *overflow - reduced_overflow;

        // Update remaining overflow
        *overflow = reduced_overflow;

    }

    thread::sleep(Duration::new(0, (tick_delay - reduction) as u32));

}

