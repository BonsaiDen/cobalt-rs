// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// STD Dependencies -----------------------------------------------------------
use std::time::Duration;


/// Structure defining connection and message configuration options.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Config {

    /// Number of packets send per second. Default is `30`.
    pub send_rate: u64,

    /// Maximum bytes that can be received / send in one packet. Default
    /// `1400`.
    pub packet_max_size: usize,

    /// 32-Bit Protocol ID used to identify UDP related packets. Default is
    /// `[1, 2, 3, 4]`.
    pub protocol_header: [u8; 4],

    /// Maximum roundtrip-time in milliseconds before a packet is considered
    /// lost. Default is `1000`.
    pub packet_drop_threshold: Duration,

    /// Maximum time in milliseconds until the first packet must be received
    /// before a connection attempt fails. Default is `100`.
    pub connection_init_threshold: Duration,

    /// Maximum time in milliseconds between any two packets before the
    /// connection gets dropped. Default is `1000`.
    pub connection_drop_threshold: Duration,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Instant` into a packet via a `MessageQueue`.
    pub message_quota_instant: f32,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Reliable` into a packet via a `MessageQueue`.
    pub message_quota_reliable: f32,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Ordered` into a packet via a `MessageQueue`.
    pub message_quota_ordered: f32,

    /// Whether to keep track of ticks which exceed their maximum running time
    /// and speed up successive ticks in order to keep the desired target
    /// `send_rate` stable.
    ///
    /// Each tick has a limit of the number of milliseconds in can take before
    /// the `send_rate` drops below the specified ticks per second
    /// target (`1000 / send_rate` milliseconds). Ticks which fall below this
    /// threshold will normally sleep for the remaining amount of time.
    ///
    /// Ticks which exceed this threshold will normally cause the `send_rate`
    /// to drop below the target; however, with `tick_overflow_recovery`
    /// enabled any tick which falls below the threshold will give up some of
    /// its remaining sleep time in order to allow the `send_rate` to catch up
    /// again.
    ///
    /// How much of each tick's sleep time is used for speedup purposes is
    /// determined by the value of `tick_overflow_recovery`.
    ///
    /// Default is `true`.
    pub tick_overflow_recovery: bool,

    /// Determines how much of each tick's sleep time may be used to reduce the
    /// current tick overflow time in order to smooth out the `send_rate`.
    ///
    /// Values must be in the range of `0.0` to `1.0` where `0.25` would be a
    /// quarter of the tick's sleep time.
    ///
    /// Example: For a `send_rate` of `30` with a maximum sleep time of `33.33`
    /// milliseconds, a `tick_overflow_recovery_rate` value of `0.5` would
    /// allow up to `16.66` milliseconds of sleep to be skipped.
    ///
    /// Values smaller than `0.0` or bigger than `1.0` will have no effect.
    ///
    /// Default is `1.0`.
    pub tick_overflow_recovery_rate: f32

}

impl Default for Config {

    fn default() -> Config {
        Config {
            send_rate: 30,
            protocol_header: [1, 2, 3, 4],
            packet_max_size: 1400,
            packet_drop_threshold: Duration::from_millis(1000),
            connection_init_threshold: Duration::from_millis(100),
            connection_drop_threshold: Duration::from_millis(1000),
            message_quota_instant: 60.0,
            message_quota_reliable: 20.0,
            message_quota_ordered: 20.0,
            tick_overflow_recovery: true,
            tick_overflow_recovery_rate: 1.0
        }
    }

}

