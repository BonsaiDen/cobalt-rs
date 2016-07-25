// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp;
use std::time::{Duration, Instant};
use super::super::{Config, RateLimiter};

/// Minimum time before switching back into good mode in milliseconds.
const MIN_GOOD_MODE_TIME_DELAY: u32 = 1000;

/// Maximum time before switching back into good mode in milliseconds.
const MAX_GOOD_MODE_TIME_DELAY: u32 = 60000;

#[derive(Debug, PartialEq)]
enum Mode {
    Good,
    Bad
}

/// Implementation of a binary state rate limiter for congestion avoidance.
///
/// It is based on the example design from the following article:
/// http://gafferongames.com/networking-for-game-programmers/reliability-and-flow-control/
#[derive(Debug)]
pub struct BinaryRateLimiter {
    tick: u32,
    max_tick: u32,
    mode: Mode,
    rtt_threshold: u32,
    last_bad_time: Instant,
    last_good_time: Instant,
    good_time_duration: Duration,
    delay_until_good_mode: u32
}

impl BinaryRateLimiter {

    /// Creates a new rate limiter.
    pub fn new(config: &Config) -> Box<BinaryRateLimiter> {

        let rate = config.send_rate as f32;
        let now = Instant::now();

        Box::new(BinaryRateLimiter {
            tick: 0,
            // Calculate about a third of normal send rate
            max_tick: (rate / (33.0 / (100.0 / rate))) as u32,
            mode: Mode::Good,
            rtt_threshold: 250,
            last_bad_time: now,
            last_good_time: now,
            good_time_duration: Duration::new(0, 0),
            delay_until_good_mode: MIN_GOOD_MODE_TIME_DELAY
        })

    }

}

impl RateLimiter for BinaryRateLimiter {

    fn update(&mut self, rtt: u32, _: f32) {

        // Check current network conditions
        let conditions = if rtt <= self.rtt_threshold {
            // Keep track of the time we are in good mode
            let now = Instant::now();
            self.good_time_duration += now - self.last_good_time;
            self.last_good_time = now;
            Mode::Good

        } else {
            // Remember the last time we were in bad mode
            self.last_bad_time = Instant::now();
            self.good_time_duration = Duration::new(0, 0);
            Mode::Bad
        };

        match self.mode  {

            Mode::Good => match conditions {

                // If we are currently in good mode, and conditions become bad,
                // immediately drop to bad mode
                Mode::Bad =>  {

                    self.mode = Mode::Bad;

                    // To avoid rapid toggling between good and bad mode, if we
                    // drop from good mode to bad in under 10 seconds
                    if self.last_bad_time.elapsed() < Duration::from_millis(10000) {

                        // We double the amount of time before bad mode goes
                        // back to good.
                        self.delay_until_good_mode *= 2;

                        // We also clamp this at a maximum
                        self.delay_until_good_mode = cmp::min(
                            self.delay_until_good_mode,
                            MAX_GOOD_MODE_TIME_DELAY
                        );

                    }

                },

                Mode::Good => {

                    // To avoid punishing good connections when they have short
                    // periods of bad behavior, for each 10 seconds the
                    // connection is in good mode, we halve the time before bad
                    // mode goes back to good.
                    if self.good_time_duration >= Duration::from_millis(10000) {
                        self.good_time_duration -= Duration::from_millis(10000);

                        // We also clamp this at a minimum
                        self.delay_until_good_mode = cmp::max(
                            self.delay_until_good_mode,
                            MIN_GOOD_MODE_TIME_DELAY
                        );

                    }

                }

            },

            Mode::Bad => {

                // If you are in bad mode, and conditions have been good for a
                // specific length of time return to good mode
                if self.last_bad_time.elapsed() > Duration::from_millis(self.delay_until_good_mode as u64) {
                    self.mode = Mode::Good;
                }

            }

        }

        // Tick wrapper for send rate reduction, max_tick is calculated to be
        // about a third of the configured send_rate
        self.tick += 1;
        if self.tick == self.max_tick {
            self.tick = 0;
        }

    }

    fn congested(&self) -> bool {
        self.mode == Mode::Bad
    }

    fn should_send(&self ) -> bool {
        // Send all packets when in good mode and about a third when in bad mode
        !self.congested() || self.tick == 0
    }

    fn reset(&mut self) {
        let now = Instant::now();
        self.tick = 0;
        self.mode = Mode::Good;
        self.last_bad_time = now;
        self.last_good_time = now;
        self.good_time_duration = Duration::new(0, 0);
        self.delay_until_good_mode = MIN_GOOD_MODE_TIME_DELAY;
    }

}

