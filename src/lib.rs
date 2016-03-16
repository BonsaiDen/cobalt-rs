// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! **cobalt** is a networking library which provides [virtual connections
//! over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! along with a messaging layer for sending both unreliable, reliable as well
//! as ordered messages.
//!
//! It is primarily designed to be used as the basis for real-time, latency
//! bound, multi client systems.
//!
//! The library provides the underlying architecture required for handling and
//! maintaining virtual connections over UDP sockets and takes care of sending
//! reliable, raw messages over the established client-server connections with
//! minimal overhead.
//!
//! cobalt is also fully configurable and can be plugged in a variety of ways,
//! so that library users have the largest possible degree of control over
//! the behavior of their connections.
//!
//! ## Getting started
//!
//! When working with **cobalt** the most important part is implementing the so
//! called handlers. A `Handler` is a trait which acts as a event proxy for
//! both server and client events that are emitted from the underlying tick
//! loop.
//!
//! Below follows a very basic example implementation of a custom game server.
//!
//! ```
//! use std::collections::HashMap;
//! use cobalt::{Config, Connection, ConnectionID, Handler, Server};
//!
//! struct GameServer;
//! impl Handler<Server> for GameServer {
//!
//!     fn bind(&mut self, server: &mut Server) {
//!         // Since this is a runnable doc, we'll just exit right away
//!         server.shutdown().unwrap();
//!     }
//!
//!     fn tick_connections(
//!         &mut self, _: &mut Server,
//!         connections: &mut HashMap<ConnectionID, Connection>
//!     ) {
//!
//!         for (_, conn) in connections.iter_mut() {
//!             // Receive player input
//!         }
//!
//!         // Advance game state
//!
//!         for (_, conn) in connections.iter_mut() {
//!             // Send state to players
//!         }
//!
//!     }
//!
//!     fn shutdown(&mut self, _: &mut Server) {
//!         // Logging and things
//!     }
//!
//!     fn connection(&mut self, _: &mut Server, _: &mut Connection) {
//!         // Create Player, send MOTD etc.
//!     }
//!
//!     fn connection_lost(&mut self, _: &mut Server, _: &mut Connection) {
//!         // Remove Player
//!     }
//!
//! }
//!
//! let mut handler = GameServer;
//! let mut server = Server::new(Config::default());
//! server.bind(&mut handler, "127.0.0.1:7156").expect("Failed to bind server.");
//! ```
//!
//! And the client version would look almost identical except for a few methods
//! having different names.
//!
//! ```
//! use cobalt::{Config, Connection, Handler, Client};
//!
//! struct GameClient;
//! impl Handler<Client> for GameClient {
//!
//!     fn connect(&mut self, client: &mut Client) {
//!         // Since this is a runnable doc, we'll just exit right away
//!         client.close().unwrap();
//!     }
//!
//!     fn tick_connection(&mut self, _: &mut Client, conn: &mut Connection) {
//!
//!         // Receive incoming state from the server
//!         for msg in conn.received() {
//!             println!("Received message {:?}", msg);
//!         }
//!
//!         // Advance game state
//!
//!         // Send input to server
//!
//!     }
//!
//!     fn close(&mut self, _: &mut Client) {
//!         // Exit game
//!     }
//!
//!     fn connection(&mut self, _: &mut Client, _: &mut Connection) {
//!         // Request map list and settings from server
//!     }
//!
//!     fn connection_failed(&mut self, _: &mut Client, _: &mut Connection) {
//!         // Failed to connect to the server
//!     }
//!
//!     fn connection_lost(&mut self, _: &mut Client, _: &mut Connection) {
//!         // Inform the user
//!     }
//!
//! }
//!
//! let mut handler = GameClient;
//! let mut client = Client::new(Config::default());
//! client.connect(&mut handler, "127.0.0.1:7156").expect("Failed to connect.");
//! ```
//!
//! ## Integration with existing event loops
//!
//! When a client already provides its own event loop via a rendering framework
//! or game engine, a **synchronous** version of the `Client` interface is also
//! available in order to ease integration in such cases.
//!
//! In these cases it can also make sense to use a `EventQueue` like
//! implementation of the `Handler` trait.
//!
//! ```
//! use cobalt::{Client, Config, Handler};
//!
//! struct SyncHandler;
//! impl Handler<Client> for SyncHandler {}
//!
//! let mut handler = SyncHandler;
//! let mut client = Client::new(Config::default());
//! let mut state = client.connect_sync(
//!     &mut handler,
//!     "127.0.0.1:7156"
//!
//! ).expect("Failed to connect.");
//!
//! // Receive from and tick the client connection
//! client.receive_sync(&mut handler, &mut state, 0);
//! client.tick_sync(&mut handler, &mut state);
//!
//! // Handle received message and send new ones here
//!
//! // Send any pending messages via the connection
//! client.send_sync(&mut handler, &mut state);
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
mod client_stream;
mod server;

mod shared {
    pub mod binary_rate_limiter;
    pub mod config;
    pub mod connection;
    pub mod message_queue;
    pub mod udp_socket;
    pub mod stats;
}

mod traits {
    pub mod handler;
    pub mod rate_limiter;
    pub mod socket;
}

#[doc(inline)]
pub use shared::config::Config;

#[doc(inline)]
pub use shared::connection::{
    Connection,
    ConnectionID,
    ConnectionMap,
    ConnectionState
};

#[doc(inline)]
pub use shared::message_queue::MessageKind;

#[doc(inline)]
pub use shared::binary_rate_limiter::BinaryRateLimiter;

#[doc(inline)]
pub use shared::udp_socket::UdpSocket;

#[doc(inline)]
pub use shared::stats::Stats;

#[doc(inline)]
pub use traits::handler::Handler;

#[doc(inline)]
pub use traits::rate_limiter::RateLimiter;

#[doc(inline)]
pub use traits::socket::Socket;

#[doc(inline)]
pub use client::Client;

#[doc(inline)]
pub use client::ClientState;

#[doc(inline)]
pub use client_stream::ClientStream;

#[doc(inline)]
pub use client_stream::ClientEvent;

#[doc(inline)]
pub use server::Server;

#[cfg(test)]
mod tests {
    mod client;
    mod client_stream;
    mod connection;
    mod message_queue;
    mod server;
}

