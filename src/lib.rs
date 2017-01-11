// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! **cobalt** is a networking library which provides [virtual connections
//! over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! for both unreliable, reliable and optional in-order delivery of messages.
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
mod client;
mod server;
mod tick;

mod shared {
    pub mod binary_rate_limiter;
    pub mod config;
    pub mod connection;
    pub mod message_queue;
    pub mod noop_packet_modifier;
    pub mod udp_socket;
    pub mod stats;
}

mod traits {
    pub mod packet_modifier;
    pub mod rate_limiter;
    pub mod socket;
}

#[doc(inline)]
pub use shared::binary_rate_limiter::BinaryRateLimiter;

#[doc(inline)]
pub use shared::config::Config;

#[doc(inline)]
pub use shared::connection::{
    Connection,
    ConnectionID,
    ConnectionMap,
    ConnectionState,
    ConnectionEvent
};

#[doc(inline)]
pub use shared::message_queue::MessageKind;

#[doc(inline)]
pub use shared::noop_packet_modifier::NoopPacketModifier;

#[doc(inline)]
pub use shared::udp_socket::UdpSocket;

#[doc(inline)]
pub use shared::stats::Stats;

#[doc(inline)]
pub use traits::packet_modifier::PacketModifier;

#[doc(inline)]
pub use traits::rate_limiter::RateLimiter;

#[doc(inline)]
pub use traits::socket::Socket;

#[doc(inline)]
pub use client::Client;

#[doc(inline)]
pub use client::ClientEvent;

#[doc(inline)]
pub use server::Server;

#[doc(inline)]
pub use server::ServerEvent;

#[cfg(test)]
mod tests {
    mod client;
    mod connection;
    mod message_queue;
    mod mock;
    mod server;
}

