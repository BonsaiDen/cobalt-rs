// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// Internal Dependencies ------------------------------------------------------
use ::Config;

/// A structure containing stats data average of the course of one second.
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Stats {

    /// Average number of bytes received over the last second.
    pub bytes_sent: u32,

    /// Average number of bytes received over the last second.
    pub bytes_received: u32

}

impl Stats {
    pub fn reset(&mut self) {
        self.bytes_sent = 0;
        self.bytes_received = 0;
    }
}

impl Default for Stats {
    fn default() -> Stats {
        Stats {
            bytes_sent: 0,
            bytes_received: 0
        }
    }
}

/// Structure to keep track of per second average stats of a Client or Server.
///
/// Uses a list of buckets and caluclates the average each time a new value is
/// pushed into the bucket list `O(1)`.
#[derive(Debug)]
pub struct StatsCollector {

    /// Internal tick value
    tick: u64,

    /// The collectors's configuration
    config: Config,

    /// Internal stat buckets for O(1) average calculation
    buckets: Vec<Stats>,

    /// Internal stat average for the current tick
    averages: Stats

}

impl StatsCollector {

    /// Creates a new stats object which averages incoming data over the given
    /// number of ticks per second.
    pub fn new(config: Config) -> StatsCollector {
        StatsCollector {
            tick: 0,
            config: config,
            buckets: (0..config.send_rate + 1).map(|_| {
                Stats::default()

            }).collect::<Vec<Stats>>(),
            averages: Stats::default()
        }
    }

    /// Overrides the collector's existing configuration.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.buckets = (0..config.send_rate + 1).map(|_| {
            Stats::default()

        }).collect::<Vec<Stats>>()
    }

    /// Sets the number of bytes sent for the current tick.
    pub fn set_bytes_sent(&mut self, bytes: u32) {
        let old_index = (self.tick as i32 + 1) % (self.config.send_rate + 1) as i32;
        let old_bytes = self.buckets[old_index as usize].bytes_sent;
        self.averages.bytes_sent = (self.averages.bytes_sent - old_bytes) + bytes;
        self.buckets[self.tick as usize].bytes_sent = bytes;
    }

    /// Sets the number of bytes received for the current tick.
    pub fn set_bytes_received(&mut self, bytes: u32) {
        let old_index = (self.tick as i32 + 1) % (self.config.send_rate + 1) as i32;
        let old_bytes = self.buckets[old_index as usize].bytes_received;
        self.averages.bytes_received = (self.averages.bytes_received - old_bytes) + bytes;
        self.buckets[self.tick as usize].bytes_received = bytes;
    }

    /// Steps the internal tick value used for average calculation.
    pub fn tick(&mut self) {
        self.tick = (self.tick + 1) % (self.config.send_rate + 1);
    }

    /// Returns the calculated averages from the last tick.
    pub fn average(&self) -> Stats {
        self.averages
    }

    /// Resets the internal data used for average calculation, but does not
    /// reset the last calculated averages.
    pub fn reset(&mut self) {
        self.averages.bytes_sent = 0;
        self.averages.bytes_received = 0;
        for d in &mut self.buckets {
            d.reset();
        }
    }

}

