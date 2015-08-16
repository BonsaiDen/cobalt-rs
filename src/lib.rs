//! **cobalt** is a networking library which provides [virtual connections
//! over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! and along with a messaging layer for sending both unreliable, reliable and
//! ordered messages.
//!
//! It is primarily designed to be used as the basis for real-time, latency
//! limited, multi client systems such as fast paced action games.
//!
//! The library provides the underlying architecture required for handling and
//! maintaining virtual connections over a UDP socket and sending reliable
//! messages over the established client-server connections.
//!
//! One of the main goals is to keep the overall overhead for connection
//! management and message de-/serialization small, while also providing a
//! clean API for easy implementation of use defined server and client logic.
//!
//! Configuration is also an important factor when it comes to network
//! interaction and so cobalt allows to configure connections and their
//! behavior to the largest possible degree.
//!
//! ## Basic Server Handler
//!
//! The basis of a system that uses **cobalt** is to implementing a so called
//! `Handler`, which receives will have its methods called for all kinds of
//! different server / client related events.
//!
//! Below is a very basic example implementation for a game server.
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
//!

mod client;
mod server;

mod shared {
    pub mod binary_rate_limiter;
    pub mod config;
    pub mod connection;
    pub mod message_queue;
    pub mod udp_socket;
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
pub use traits::handler::Handler;

#[doc(inline)]
pub use traits::rate_limiter::RateLimiter;

#[doc(inline)]
pub use client::Client;

#[doc(inline)]
pub use server::Server;

