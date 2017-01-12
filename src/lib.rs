// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! **cobalt** is a networking library which provides [virtual connections
//! over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! for both reliable and unreliable messages with optional in-order delivery.
//!
//! It is primarily designed to be used as the basis of real-time, low latency,
//! single server / multi client systems.
//!
//! The library provides the underlying architecture required for handling and
//! maintaining virtual connections over UDP sockets and takes care of sending
//! raw messages over the established client-server connections with minimal
//! overhead.
//!
//! **cobalt** is also fully configurable and can be integrated in a variety of
//! ways, as can be seen in the examples below.
//! ```
#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![deny(
    missing_docs,
    missing_debug_implementations, missing_copy_implementations,
    trivial_casts, trivial_numeric_casts,
    unsafe_code,
    unused_import_braces, unused_qualifications
)]
mod shared;
mod traits;
mod client;
mod server;


// Exports --------------------------------------------------------------------
pub use shared::{
    BinaryRateLimiter,
    Config,
    Connection,
    ConnectionID,
    ConnectionMap,
    ConnectionState,
    ConnectionEvent,
    MessageKind,
    NoopPacketModifier,
    UdpSocket
};
pub use traits::*;
pub use client::*;
pub use server::*;

#[cfg(test)]
mod test;

