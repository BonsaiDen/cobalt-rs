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
//!         server.shutdown();
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
//! server.bind(&mut handler, "127.0.0.1:7156").unwrap();
//! ```
//!
//! And the client version would look almost identical except for a few methods
//! having different names.
//!
//! ```
//! use std::collections::HashMap;
//! use cobalt::{Config, Connection, Handler, Client};
//!
//! struct GameClient;
//! impl Handler<Client> for GameClient {
//!
//!     fn connect(&mut self, client: &mut Client) {
//!         // Since this is a runnable doc, we'll just exit right away
//!         client.close();
//!     }
//!
//!     fn tick_connection(&mut self, _: &mut Client, conn: &mut Connection) {
//!
//!         for msg in conn.received() {
//!             // Receive state from server
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
//!     fn connection_failed(&mut self, client: &mut Client, _: &mut Connection) {
//!         // Failed to connect to the server
//!     }
//!
//!     fn connection_lost(&mut self, client: &mut Client, _: &mut Connection) {
//!         // Inform the user
//!     }
//!
//! }
//!
//! let mut handler = GameClient;
//! let mut client = Client::new(Config::default());
//! client.connect(&mut handler, "127.0.0.1:7156").unwrap();
//! ```
#![deny(
    missing_docs,
    missing_debug_implementations, missing_copy_implementations,
    trivial_casts, trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces, unused_qualifications
)]
mod client;
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
pub use shared::connection::{Connection, ConnectionID, ConnectionState};

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
pub use server::Server;

#[cfg(test)]
mod tests {
    mod client;
    mod connection;
    mod message_queue;
    mod server;
}

