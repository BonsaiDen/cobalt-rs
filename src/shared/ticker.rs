// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// STD Dependencies -----------------------------------------------------------
use std::cmp;
use std::thread;
use std::time::{Duration, Instant};


// Internal Dependencies ------------------------------------------------------
use ::shared::config::Config;


// Tick Rate Limiting ---------------------------------------------------------
#[derive(Debug)]
pub struct Ticker {
    tick_start: Instant,
    tick_overflow: u64,
    tick_overflow_recovery: bool,
    tick_overflow_recovery_rate: f32,
    tick_delay: u64
}

impl Ticker {

    pub fn new(config: Config) -> Ticker {
        Ticker {
            tick_start: Instant::now(),
            tick_overflow: 0,
            tick_overflow_recovery: config.tick_overflow_recovery,
            tick_overflow_recovery_rate: config.tick_overflow_recovery_rate,
            tick_delay: 1000000000 / config.send_rate
        }
    }

    pub fn set_config(&mut self, config: Config) {
        self.tick_overflow_recovery = config.tick_overflow_recovery;
        self.tick_overflow_recovery_rate = config.tick_overflow_recovery_rate;
        self.tick_delay = 1000000000 / config.send_rate
    }

    pub fn begin_tick(&mut self) {
        self.tick_start = Instant::now();
    }

    pub fn reset(&mut self) {
        self.tick_start = Instant::now();
        self.tick_overflow = 0;
    }

    pub fn end_tick(&mut self) {

        // Actual time taken by the tick
        let time_taken = nanos_from_duration(self.tick_start.elapsed());

        // Required delay reduction to keep tick rate
        let mut reduction = cmp::min(time_taken, self.tick_delay);

        if self.tick_overflow_recovery {

            // Keep track of how much additional time the current tick required
            self.tick_overflow += time_taken - reduction;

            // Try to reduce the existing overflow by reducing the reduction time
            // for the current frame.
            let max_correction = (self.tick_delay - reduction) as i64;
            let correction = cmp::min(
                (max_correction as f32 * self.tick_overflow_recovery_rate) as i64,
                max_correction
            );

            // This way we'll achieve a speed up effect in an effort to keep the
            // desired tick rate stable over a longer period of time
            let reduced_overflow = cmp::max(0, self.tick_overflow as i64 - correction) as u64;

            // Adjust the reduction amount to speed up
            reduction += self.tick_overflow - reduced_overflow;

            // Update remaining overflow
            self.tick_overflow = reduced_overflow;

        }

        thread::sleep(Duration::new(0, (self.tick_delay - reduction) as u32));

    }

}

// Helpers ---------------------------------------------------------------------
fn nanos_from_duration(d: Duration) -> u64 {
    d.as_secs() * 1000 * 1000000 + d.subsec_nanos() as u64
}

