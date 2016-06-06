// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::cmp;
use std::thread;
use std::time::Duration;

pub fn start() -> u64 {
    clock_ticks::precise_time_ns()
}

pub fn end(tick_delay: u32, begin: u64, time_overflow: &mut u32) {

    // Actual time taken by the tick
    let time_taken = (clock_ticks::precise_time_ns() - begin) as u32;

    // Required wait time to keep tick rate
    let mut wait = cmp::min(time_taken, tick_delay);

    // Keep track of how much additional time the current tick required
    *time_overflow += time_taken - wait;

    // Try to reduce the existing overflow by reducing the wait time for the
    // current frame. This we'll achieve a speed up effect in an effort to keep
    // the desired tick rate stable over a longer period of time
    // TODO should we stretch this out by only apply half of the available
    // correction on each successive tick?
    let available_correction = (tick_delay - wait) as i64;
    let reduced_overflow = cmp::max(
        0,
        *time_overflow as i64 - available_correction

    ) as u32;

    // Adjust the wait perio
    wait += *time_overflow - reduced_overflow;

    // Update remaining overflow
    *time_overflow = reduced_overflow;

    thread::sleep(Duration::new(0, tick_delay - wait));

}

