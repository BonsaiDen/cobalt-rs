//! The main crate for cobalt.
//!
//! ## Overview
//!
//! **cobalt** is a networking library which provides [virtual connections
//! over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/)
//! and provides a messaging layer for sending both unreliable as well as
//! reliable and optionally ordered messages.
//!
//! It is primarily designed to be used as the basis for realtime, latency
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
//! `Handler`, which receives will hav its methods called for all kinds of
//! different server / client related events.
//!
//! Below is a very basic example implementation for a game server.
//!
//! ```
//! struct GameServer {
//!     motd: String
//! }
//!
//! impl Handler<Server> for GameServer {
//!
//!     fn bind(&mut self, _: &mut Server) {
//!         // Load level, connect to master server etc.
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
//! ```
//!
//! And the client version would look almost identical except for a few methods
//! having differnt names.
//!
//! ```
//! struct GameClient;
//! impl Handler<Client> for GameClient {
//!
//!     fn connect(&mut self, _: &mut Client) {
//!         // Load game data etc.
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
//! ```
//!

/// Connection and message handling.
pub mod shared;

/// A multi-client UDP server implementation.
pub mod server;

/// A UDP client implementation.
pub mod client;

