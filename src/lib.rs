// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! **cobalt** is low level a networking library which implements [virtual
//! connections over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! supporting both unreliable messaging and reliable messages with optional
//! in-order delivery.
//!
//! It is designed for use with real-time, low latency situations, for example
//! action oriented multiplayer games.
//!
//! The library provides the underlying architecture required for handling and
//! maintaining virtual connections over UDP sockets and takes care of sending
//! raw messages over the established client-server connections with minimal
//! overhead.
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

