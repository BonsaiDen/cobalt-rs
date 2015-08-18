use std::collections::HashMap;
use super::super::{
    BinaryRateLimiter, Connection, ConnectionID, Config, RateLimiter
};

/// Trait for implementation a proxy which receives client and server events.
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

    /// Method that is called each time a packet send by a connection is lost.
    fn connection_packet_lost(
        &mut self, _: &mut T, _: &mut Connection, _: &[u8]
    ) {

    }

    /// Method that is called each time the congestion state of connection
    /// changes.
    fn connection_congestion_state(&mut self, _: &mut T, _: &mut Connection, _: bool) {
    }

    /// Method that is called each time a connection is lost and dropped.
    fn connection_lost(&mut self, _: &mut T, _: &mut Connection) {
    }

}

