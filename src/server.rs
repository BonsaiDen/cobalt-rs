// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io::{Error, ErrorKind};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use shared::stats::{StatsCollector, Stats};
use super::{Config, Connection, ConnectionID, MessageKind, Handler, tick};

/// Implementation of a multi-client server with handler based event dispatch.
#[derive(Debug)]
pub struct Server {
    closed: bool,
    config: Config,
    local_address: Option<SocketAddr>,
    statistics: StatsCollector
}

impl Server {

    /// Creates a new server with the given configuration.
    pub fn new(config: Config) -> Server {
        Server {
            closed: false,
            config: config,
            local_address: None,
            statistics: StatsCollector::new(config)
        }
    }

    /// Returns the local address that the server is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&mut self) -> Stats {
        self.statistics.average()
    }

    /// Returns a copy of the server's current configuration.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Overrides the server's existing configuration.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.statistics.set_config(config);
    }

    /// Binds the server to the specified local address by creating a socket
    /// and actively listens for incoming client connections.
    ///
    /// Clients connecting to the server must use a compatible connection
    /// / packet configuration in order to be able to connect.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the server and its connections.
    pub fn bind<A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Server>, addr: A

    ) -> Result<(), Error> {

        let socket = try!(UdpSocket::new(
            addr,
            self.config.packet_max_size
        ));

        self.bind_to_socket(handler, socket)

    }

    /// Binds the server to specified socket and actively listens for incoming
    /// client connections.
    ///
    /// Clients connecting to the server must use a compatible connection
    /// / packet configuration in order to be able to connect.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the server and its connections.
    pub fn bind_to_socket<S: Socket>(
        &mut self, handler: &mut Handler<Server>, mut socket: S

    ) -> Result<(), Error> {

        let local_addr = try!(socket.local_addr());
        let mut state = try!(
            self.bind_to_socket_sync(handler, local_addr, socket)
        );

        let tick_delay = 1000000000 / self.config.send_rate;
        while !self.closed {
            self.accept_receive_sync(handler, &mut state, tick_delay);
            self.tick_sync(handler, &mut state);
            self.send_sync(handler, &mut state, tick_delay);
        }

        self.shutdown_sync(handler, &mut state);

        Ok(())

    }

    /// Shuts down the server, closing all active client connections.
    ///
    /// This exits the tick loop, resets all connections and shuts down the
    /// underlying socket the server was bound to.
    pub fn shutdown(&mut self) -> Result<(), Error> {
        if self.closed {
            Err(Error::new(ErrorKind::NotConnected, ""))

        } else {
            self.closed = true;
            Ok(())
        }
    }

    // Non-Blocking Synchronous API -------------------------------------------

    /// Binds the server to the specified local address by creating a socket
    /// and actively listens for incoming client connections.
    ///
    /// Clients connecting to the server must use a compatible connection
    /// / packet configuration in order to be able to connect.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the server and its connections.
    ///
    /// This method returns a `ServerState` instance for this client, which can
    /// be used with other synchronous `Server` methods.
    pub fn bind_sync<A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Server>, addr: A

    ) -> Result<ServerState<UdpSocket>, Error> {

        let local_addr = try!(addr.to_socket_addrs()).nth(0).unwrap();
        let socket = try!(UdpSocket::new(
            local_addr,
            self.config.packet_max_size
        ));

        self.bind_to_socket_sync(handler, local_addr, socket)

    }

    fn bind_to_socket_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Server>,
        local_addr: SocketAddr,
        socket: S

    ) -> Result<ServerState<S>, Error> {

        let local_addr = try!(socket.local_addr());
        self.local_address = Some(local_addr);

        // Reset stats
        self.statistics.reset();

        // Invoke handler
        handler.bind(self);

        Ok(ServerState::new(socket, HashMap::new(), HashMap::new(), local_addr))

    }

    /// Accepts new incoming client connections and receives all their currently
    /// buffered incoming packets.
    pub fn accept_receive_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Server>,
        state: &mut ServerState<S>,
        tick_delay: u32
    ) {

        state.tick_start = tick::start();

        // Receive all incoming UDP packets to our local address
        let mut bytes_received = 0;
        while let Ok((addr, packet)) = state.socket.try_recv() {

            // Try to extract the connection id from the packet
            if let Some(id) = Connection::id_from_packet(&self.config, &packet) {

                // Retrieve or create a connection for the current
                // connection id
                let mut inserted_address: Option<SocketAddr> = None;
                let connection = state.connections.entry(id).or_insert_with(|| {

                    let mut conn = Connection::new(
                        self.config,
                        self.local_address.unwrap(),
                        addr,
                        handler.rate_limiter(&self.config)
                    );

                    inserted_address = Some(addr);
                    conn.set_id(id);
                    conn

                });

                // Also map the intitial address which is used by
                // the connection
                if let Some(addr) = inserted_address.take() {
                    state.addresses.insert(id, addr);
                }

                // Map the current remote address of the connection to
                // the latest address that sent a packet for the
                // connection id in question. This is done in order to
                // work in situations were the remote port of a
                // connection is switched around by NAT.
                if addr != connection.peer_addr() {
                    connection.set_peer_addr(addr);
                    state.addresses.remove(&id);
                    state.addresses.insert(id, addr);
                }

                // Statistics
                bytes_received += packet.len();

                // Then feed the packet into the connection object for
                // parsing
                connection.receive_packet(
                    packet, tick_delay / 1000000, self, handler
                );

            }

        }

        self.statistics.set_bytes_received(bytes_received as u32);

    }

    /// Performs exactly one tick on all underlying client connections.
    pub fn tick_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Server>,
        state: &mut ServerState<S>
    ) {
        handler.tick_connections(self, &mut state.connections);
    }

    /// Sends exactly one outgoing packet for each underlying client connection.
    pub fn send_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Server>,
        state: &mut ServerState<S>,
        tick_delay: u32
    ) {

        // List of dropped connections
        let mut dropped: Vec<ConnectionID> = Vec::new();

        // Create outgoing packets for all connections
        let mut bytes_sent = 0;
        for (id, conn) in &mut state.connections {

            // Resolve the last known remote address for this
            // connection and send the data
            let addr = &state.addresses[id];

            // Then invoke the connection to send a outgoing packet
            bytes_sent += conn.send_packet(&mut state.socket, addr, self, handler);

            // Collect all lost / closed connections
            if !conn.open() {
                dropped.push(*id);
            }

        }

        // Update statistics
        self.statistics.set_bytes_sent(bytes_sent);
        self.statistics.tick();

        // Remove any dropped connections and their address mappings
        for id in dropped.drain(..) {
            state.connections.remove(&id).unwrap().reset();
            state.addresses.remove(&id);
        }

        tick::end(tick_delay, state.tick_start, &mut state.tick_overflow, &self.config);

    }

    /// Shuts down the server, closing all active client connections.
    pub fn shutdown_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Server>,
        state: &mut ServerState<S>
    ) {

        // Invoke handler
        handler.shutdown(self);

        // Reset socket address
        self.local_address = None;

        // Reset all connection states
        state.reset();

    }

}

/// A structure used for synchronous calls on a `Server` instance.
#[derive(Debug)]
pub struct ServerState<S: Socket> {
    socket: S,
    tick_start: u64,
    tick_overflow: u32,
    connections: HashMap<ConnectionID, Connection>,
    addresses: HashMap<ConnectionID, SocketAddr>,
    local_address: SocketAddr,
    stats: Stats
}

impl<S: Socket> ServerState<S> {

    // We need to encapsulate the above objects because they cannot be
    // owned by the client itself without running into issues with multiple
    // bindings of Self when trying to both call a method on it's
    // connection and pass it to that method.
    fn new(
        socket: S,
        connections: HashMap<ConnectionID, Connection>,
        addresses: HashMap<ConnectionID, SocketAddr>,
        local_addr: SocketAddr

    ) -> ServerState<S> {
        ServerState {
            socket: socket,
            tick_start: 0,
            tick_overflow: 0,
            connections: connections,
            addresses: addresses,
            local_address: local_addr,
            stats: Stats {
                bytes_sent: 0,
                bytes_received: 0
            }
        }
    }

    /// Sends a message of the specified `kind` along with its `payload` over
    /// a specific client connection.
    pub fn send(&mut self, id: &ConnectionID, kind: MessageKind, payload: Vec<u8>) -> Result<(), Error> {
        if let Some(conn) = self.connections.get_mut(id) {
            conn.send(kind, payload);
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotFound, ""))
        }
    }

    /// Returns a mutable reference for the specified client connection.
    pub fn connection_mut(&mut self, id: &ConnectionID) -> Option<&mut Connection> {
        self.connections.get_mut(id)
    }

    /// Returns a mutable reference for the servers client connections.
    pub fn connections_mut(&mut self) -> &mut HashMap<ConnectionID, Connection> {
        &mut self.connections
    }

    /// Overrides the configuration of this servers's underlying connections.
    pub fn set_config(&mut self, config: Config) {
        for (_, conn) in &mut self.connections {
            conn.set_config(config);
        }
    }

    /// Resets this servers's underlying connection states.
    pub fn reset(&mut self) {
        for (_, conn) in &mut self.connections {
            conn.reset();
        }
    }

}

