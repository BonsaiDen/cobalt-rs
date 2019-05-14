// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// STD Dependencies -----------------------------------------------------------
use std::cmp;
use std::time::{Duration, Instant};


// Internal Dependencies ------------------------------------------------------
use ::{Config, RateLimiter};

/// Minimum time before switching back into good mode in milliseconds.
const MIN_GOOD_MODE_TIME_DELAY: u64 = 1000;

/// Maximum time before switching back into good mode in milliseconds.
const MAX_GOOD_MODE_TIME_DELAY: u64 = 60000;

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
    good_time_duration: u64,
    delay_until_good_mode: u64
}

impl RateLimiter for BinaryRateLimiter {

    fn new(config: Config) -> BinaryRateLimiter {

        let rate = config.send_rate as f32;

        BinaryRateLimiter {
            tick: 0,
            // Calculate about a third of normal send rate
            max_tick: (rate / (33.0 / (100.0 / rate))) as u32,
            mode: Mode::Good,
            rtt_threshold: 250,
            last_bad_time: Instant::now(),
            last_good_time: Instant::now(),
            good_time_duration: 0,
            delay_until_good_mode: MIN_GOOD_MODE_TIME_DELAY
        }

    }

    fn update(&mut self, rtt: u32, _: f32) {

        // Check current network conditions
        let conditions = if rtt <= self.rtt_threshold {
            // Keep track of the time we are in good mode
            self.good_time_duration += time_since(&self.last_good_time);
            self.last_good_time = Instant::now();
            Mode::Good

        } else {
            // Remember the last time we were in bad mode
            self.last_bad_time = Instant::now();
            self.good_time_duration = 0;
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
                    if time_since(&self.last_bad_time) < 10000 {

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
                    if self.good_time_duration >= 10000 {
                        self.good_time_duration -= 10000;

                        self.delay_until_good_mode /= 2;

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
                if time_since(&self.last_bad_time) > self.delay_until_good_mode {
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
        // Send all packets when in good mode and about only a third when in
        // bad mode
        !self.congested() || self.tick == 0
    }

    fn reset(&mut self) {
        self.tick = 0;
        self.mode = Mode::Good;
        self.last_bad_time = Instant::now();;
        self.last_good_time = Instant::now();
        self.good_time_duration = 0;
        self.delay_until_good_mode = MIN_GOOD_MODE_TIME_DELAY;
    }

}

fn time_since(i: &Instant) -> u64 {
    millis_from_duration(i.elapsed())
}

fn millis_from_duration(d: Duration) -> u64 {
    d.as_secs() * 1000 + (d.subsec_nanos() as u64 / 1000000)
}

#[cfg(test)]
mod test {

    use std::thread;
    use std::time::Duration;

    use ::{Config, RateLimiter};
    use super::BinaryRateLimiter;

    #[test]
    fn test_modes() {

        let mut rl = BinaryRateLimiter::new(Config::default());

        // Default to good mode
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

        // Update with values that continue good mode
        rl.update(51, 0.0);
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

        rl.update(151, 0.0);
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

        rl.update(250, 0.0);
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

        // Update with values that will trigger bad mode
        rl.update(251, 0.0);

        // Should now be in bad mode
        assert_eq!(rl.congested(), true);

        // Should not send for the next two update calls
        assert_eq!(rl.should_send(), false);
        rl.update(251, 0.0);
        assert_eq!(rl.should_send(), false);
        rl.update(251, 0.0);

        // Every third tick should send while in bad mode
        assert_eq!(rl.should_send(), true);

        // Sleep until the limiter will transition back into good mode
        thread::sleep(Duration::from_millis(2100));
        rl.update(12, 0.0);
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

    }

    #[test]
    fn test_reset() {

        let mut rl = BinaryRateLimiter::new(Config::default());

        // Default to good mode
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

        // Update with values that will trigger bad mode
        rl.update(251, 0.0);
        assert_eq!(rl.congested(), true);
        assert_eq!(rl.should_send(), false);

        // Reset should put the limiter back into good mode
        rl.reset();
        assert_eq!(rl.congested(), false);
        assert_eq!(rl.should_send(), true);

    }

}

