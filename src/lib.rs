// Copyright (c) 2015-2016 Ivo Wetzel

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
//!
//! ## Getting started
//!
//! **cobalt** supports a number of different integration strategies, the most
//! forward one being the use of so called *handlers*. The `Handler` trait acts
//! as a event proxy for both server and client events that are emitted from
//! the underlying tick loop inside the library.
//!
//!
//! ## Handler based integration
//!
//! The handler based approach suits itself especially well for the server side
//! where all updates on any connections will be performed inside a single tick
//! handler.
//!
//! ```
//! use std::collections::HashMap;
//! use cobalt::{Config, Connection, ConnectionID, Handler, Server, MessageKind};
//!
//! struct GameServer {
//!     tick: u8
//! }
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
//!             for msg in conn.received() {
//!                 println!("Received message from client: {:?}", msg);
//!             }
//!         }
//!
//!         // Advance game state
//!         self.tick = self.tick.wrapping_add(1);
//!
//!         // Send state updates to players
//!         for (_, conn) in connections.iter_mut() {
//!             conn.send(MessageKind::Instant, b"Hello World".to_vec());
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
//! let mut handler = GameServer {
//!     tick: 0
//! };
//! let mut server = Server::new(Config::default());
//! server.bind(&mut handler, "127.0.0.1:7156").ok();
//! ```
//!
//! The client version of the handler based approach looks almost
//! identical - except for a few methods sporting different names.
//!
//! This method is best used when separate logic / rendering threads are used.
//!
//! ```
//! use cobalt::{Config, Connection, Handler, Client, MessageKind};
//!
//! struct GameClient {
//!     tick: u8
//! };
//!
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
//!         self.tick = self.tick.wrapping_add(1);
//!
//!         // Send some message to server
//!         conn.send(MessageKind::Instant, b"Hello World".to_vec());
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
//! let mut handler = GameClient {
//!     tick: 0
//! };
//! let mut client = Client::new(Config::default());
//! client.connect(&mut handler, "127.0.0.1:7156").ok();
//! ```
//!
//! ## Stream based integration
//!
//! For situations where a event loop is already in place (e.g. game engines),
//! a stream based approach can be used in order to achieve easier integration
//! in single threaded scenarios.
//!
//! ```
//! use cobalt::{ClientEvent, ClientStream, Config, MessageKind};
//!
//! // Create a new stream
//! let mut stream = ClientStream::new(Config {
//!     send_rate: 15,
//!     .. Default::default()
//! });
//!
//! // Initiate the connection
//! stream.connect("127.0.0.1:5555").ok();
//!
//! // Inside of the existing event loop
//! // loop {
//!
//!     // Receive incoming messages
//!     while let Ok(event) = stream.receive() {
//!         match event {
//!             ClientEvent::Connection => println!("Connection established"),
//!             ClientEvent::ConnectionLost => println!("Connection lost"),
//!             ClientEvent::Tick => {
//!                 // Handle network related logic, advance actual in game time
//!             },
//!             ClientEvent::Message(payload) => println!("Received message: {:?}", payload),
//!             _ => {}
//!         }
//!     }
//!
//!     // Send some messages
//!     stream.send(MessageKind::Instant, b"Hello World".to_vec()).ok();
//!
//!     // Send outgoing messages
//!     stream.flush().ok();
//!
//! // }
//! ```
//!
//! ## Low level synchronous integration
//!
//! For times when even the stream based abstraction is too much, there's also
//! the option to use the underlying synchronous client implement on top of
//! which both the handler and stream based approaches are built.
//!
//! ```
//! use cobalt::{Client, Config, Handler, MessageKind};
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
//! ).unwrap();
//!
//! // Receive from the server
//! client.receive_sync(&mut handler, &mut state, 0);
//!
//! // Tick the connection
//! client.tick_sync(&mut handler, &mut state);
//!
//! // Send a message
//! state.send(MessageKind::Instant, b"Hello World".to_vec());
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
mod tick;

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
    mod mock;
}

