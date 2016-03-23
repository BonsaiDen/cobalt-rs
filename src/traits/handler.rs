// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::collections::HashMap;
use super::super::{
    BinaryRateLimiter, Connection, ConnectionID, Config, RateLimiter
};

/// Trait for implementation of a client / server event proxy.
pub trait Handler<T> {

    // Factories

    /// Method that returns a new `RateLimiter` instance for use with a
    /// freshly instantiated `Connection`.
    fn rate_limiter(&self, config: &Config) -> Box<RateLimiter> {
        BinaryRateLimiter::new(config)
    }

    // Server only

    /// Method that is called once a `Server` has successfully bound itself
    /// to its local address.
    fn bind(&mut self, _: &mut T) {
    }

    /// Method that is called each time a `Server` "ticks". A "tick" occurs
    /// in-between the receiving and sending data from / to connections.
    fn tick_connections(
        &mut self, _: &mut T, _: &mut HashMap<ConnectionID, Connection>
    ) {
    }

    /// Method that is called once a `Server` is going to shutdown.
    fn shutdown(&mut self, _: &mut T) {
    }

    // Client Only

    /// Method that is called once a `Client` has successfully bound itself
    /// to its local address.
    fn connect(&mut self, _: &mut T) {
    }

    /// Method that is called each time a `Client` "ticks". A "tick" occurs
    /// in-between the receiving and sending data from / to connections.
    fn tick_connection(&mut self, _: &mut T, _: &mut Connection) {
    }

    /// Method that is called once a `Client` is going to close.
    fn close(&mut self, _: &mut T) {
    }

    // Connection specific

    /// Method that is called each time a new connection is established.
    fn connection(&mut self, _: &mut T, _: &mut Connection) {
    }

    /// Method that is called each time a connection fails to establish.
    fn connection_failed(&mut self, _: &mut T, _: &mut Connection) {
    }

    /// Method that is called each time the congestion state of connection
    /// changes.
    fn connection_congestion_state(&mut self, _: &mut T, _: &mut Connection, _: bool) {
    }

    /// Method that is called each time a connection is lost and dropped.
    fn connection_lost(&mut self, _: &mut T, _: &mut Connection) {
    }

    /// Method that is called each time a connection is programmatically closed.
    fn connection_closed(&mut self, _: &mut T, _: &mut Connection, _: bool) {
    }

    // Packet specific

    /// Method that is called each time a packet send by a connection is lost.
    ///
    /// > Note: This method is feature-gated and will only be included when the
    /// `packet_handler_lost` feature is enabled.
    fn connection_packet_lost(
        &mut self, _: &mut T, _: &mut Connection, _: &[u8]
    ) {

    }

    /// Method that is called for in-place compression purposes before a packet
    /// is send over the connection's underlying socket.
    ///
    /// The returned `usize` should indicate the number of data bytes left
    /// *after* the in-place compression has been applied.
    ///
    /// The default implementation does not actually perform any kind of
    /// compression and leaves the data untouched.
    ///
    /// > Note: This method is feature-gated and will only be included when the
    /// > `packet_handler_compress` feature is enabled.
    fn connection_packet_compress(
        &mut self, _: &mut T, _: &mut Connection, data: &mut [u8]

    ) -> usize {
        data.len()
    }

    /// Method that is called for decompression purposes after a packet is
    /// received over the connection's underlying socket.
    ///
    /// The default implementation does not actually perform any kind of
    /// decompression and returns the data as is.
    ///
    /// > Note: This method is feature-gated and will only be included when the
    /// > `packet_handler_compress` feature is enabled.
    fn connection_packet_decompress(
        &mut self, _: &mut T, _: &mut Connection, data: &[u8]

    ) -> Vec<u8> {
        data.to_vec()
    }

}

