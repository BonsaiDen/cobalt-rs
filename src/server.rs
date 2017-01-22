// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// STD Dependencies -----------------------------------------------------------
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::TryRecvError;
use std::collections::{HashMap, VecDeque};


// Internal Dependencies ------------------------------------------------------
use shared::stats::{Stats, StatsCollector};
use shared::ticker::Ticker;
use super::{
    Config,
    ConnectionID, Connection, ConnectionEvent,
    RateLimiter, PacketModifier, Socket
};


/// Enum of server network events.
#[derive(Debug, PartialEq)]
pub enum ServerEvent {

    /// Event emitted once a new client connection has been established.
    Connection(ConnectionID),

    /// Event emitted when a existing client connection is lost.
    ConnectionLost(ConnectionID),

    /// Event emitted when a client connection is closed programmatically.
    ConnectionClosed(ConnectionID, bool),

    /// Event emitted for each message received from a client connection.
    Message(ConnectionID, Vec<u8>),

    /// Event emitted each time a client's connection congestion state changes.
    ConnectionCongestionStateChanged(ConnectionID, bool),

    /// Event emitted each time a client connection packet is lost.
    PacketLost(ConnectionID, Vec<u8>)

}


/// Implementation of a multi-client low latency socket server.
///
/// # Basic Usage
///
/// ```
/// use cobalt::{
///     BinaryRateLimiter, Server, Config, NoopPacketModifier, MessageKind, UdpSocket
/// };
///
/// // Create a new server that communicates over a udp socket
/// let mut server = Server::<UdpSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
///
/// // Make the server listen on port `1234` on all interfaces.
/// server.listen("0.0.0.0:1234").expect("Failed to bind to socket.");
///
/// // loop {
///
///     // Accept incoming connections and fetch their events
///     while let Ok(event) = server.accept_receive() {
///         // Handle events (e.g. Connection, Messages, etc.)
///     }
///
///     // Send a message to all connected clients
///     for (_, conn) in server.connections() {
///         conn.send(MessageKind::Instant, b"Ping".to_vec());
///     }
///
///     // Send all outgoing messages.
///     //
///     // Also auto delay the current thread to achieve the configured tick rate.
///     server.send(true);
///
/// // }
///
/// // Shutdown the server (freeing its socket and closing all its connections)
/// server.shutdown();
/// ```
///
#[derive(Debug)]
pub struct Server<S: Socket, R: RateLimiter, M: PacketModifier> {
    config: Config,
    socket: Option<S>,
    connections: HashMap<ConnectionID, Connection<R, M>>,
    addresses: HashMap<ConnectionID, SocketAddr>,
    ticker: Ticker,
    local_address: Option<SocketAddr>,
    events: VecDeque<ServerEvent>,
    should_receive: bool,
    stats_collector: StatsCollector,
    stats: Stats
}

impl<S: Socket, R: RateLimiter, M: PacketModifier> Server<S, R, M> {

    /// Creates a new server with the given configuration.
    pub fn new(config: Config) -> Server<S, R, M> {
        Server {
            config: config,
            socket: None,
            connections: HashMap::new(),
            addresses: HashMap::new(),
            ticker: Ticker::new(config),
            local_address: None,
            events: VecDeque::new(),
            should_receive: false,
            stats_collector: StatsCollector::new(config),
            stats: Stats {
                bytes_sent: 0,
                bytes_received: 0
            }
        }
    }

    /// Returns the number of bytes sent over the last second.
    pub fn bytes_sent(&self) -> u32 {
        self.stats.bytes_sent
    }

    /// Returns the number of bytes received over the last second.
    pub fn bytes_received(&self) -> u32 {
        self.stats.bytes_received
    }

    /// Returns the local address that the client is sending from.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns a mutable reference to the specified client connection.
    pub fn connection(&mut self, id: &ConnectionID) -> Result<&mut Connection<R, M>, Error> {
        if self.socket.is_some() {
            if let Some(conn) = self.connections.get_mut(id) {
                Ok(conn)

            } else {
                Err(Error::new(ErrorKind::NotFound, ""))
            }

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Returns a mutable reference to the servers client connections.
    pub fn connections(&mut self) -> &mut HashMap<ConnectionID, Connection<R, M>> {
        &mut self.connections
    }

    /// Returns a mutable reference to the server's underlying socket.
    pub fn socket(&mut self) -> Result<&mut S, Error> {
        if let Some(socket) = self.socket.as_mut() {
            Ok(socket)

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Returns the server's current configuration.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Overrides the server's current configuration.
    pub fn set_config(&mut self, config: Config) {

        self.config = config;
        self.ticker.set_config(config);
        self.stats_collector.set_config(config);

        for (_, conn) in &mut self.connections {
            conn.set_config(config);
        }

    }

    /// Binds the server to listen the specified address.
    pub fn listen<A: ToSocketAddrs>(&mut self, addr: A) -> Result<(), Error> {

        if self.socket.is_none() {

            let local_addr = try!(addr.to_socket_addrs()).nth(0).unwrap();
            let socket = try!(S::new(
                local_addr,
                self.config.packet_max_size
            ));

            self.socket = Some(socket);
            self.local_address = Some(local_addr);
            self.should_receive = true;

            Ok(())

        } else {
            Err(Error::new(ErrorKind::AlreadyExists, ""))
        }

    }

    /// Accepts new incoming client connections from the server's underlying
    /// server and receives and returns messages from them.
    pub fn accept_receive(&mut self) -> Result<ServerEvent, TryRecvError> {

        if self.socket.is_none() {
            Err(TryRecvError::Disconnected)

        } else {

            if self.should_receive {

                self.ticker.begin_tick();

                // Receive all incoming UDP packets to our local address
                let mut bytes_received = 0;
                while let Ok((addr, packet)) = self.socket.as_mut().unwrap().try_recv() {

                    // Try to extract the connection id from the packet
                    if let Some(id) = Connection::<R, M>::id_from_packet(&self.config, &packet) {
                        bytes_received += self.receive_connection_packet(id, addr, packet);
                    }

                }

                self.stats_collector.set_bytes_received(bytes_received as u32);
                self.should_receive = false;

            }

            if let Some(event) = self.events.pop_front() {
                Ok(event)

            } else {
                Err(TryRecvError::Empty)
            }

        }

    }

    /// Sends all queued messages to the server's underlying client connections.
    ///
    /// If `auto_tick` is specified as `true` this method will block the
    /// current thread for the amount of time which is required to limit the
    /// number of calls per second (when called inside a loop) to the server's
    /// configured `send_rate`.
    pub fn send(&mut self, auto_tick: bool) -> Result<(), Error> {
        if self.socket.is_some() {

            // List of dropped connections
            let mut dropped: Vec<ConnectionID> = Vec::new();

            // Create outgoing packets for all connections
            let mut bytes_sent = 0;
            for (id, connection) in &mut self.connections {

                // Resolve the last known remote address for this
                // connection and send the data
                let addr = &self.addresses[id];

                // Then invoke the connection to send a outgoing packet
                bytes_sent += connection.send_packet(
                    self.socket.as_mut().unwrap(),
                    addr
                );

                // Collect all lost / closed connections
                if !connection.open() {
                    // Map any remaining connection events
                    map_connection_events(&mut self.events, connection);
                    dropped.push(*id);
                }

            }

            // Remove any dropped connections and their address mappings
            for id in dropped {
                self.connections.remove(&id).unwrap().reset();
                self.addresses.remove(&id);
            }

            self.stats_collector.set_bytes_sent(bytes_sent);
            self.stats_collector.tick();
            self.stats = self.stats_collector.average();

            self.should_receive = true;

            if auto_tick {
                self.ticker.end_tick();
            }

            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Shuts down all of the server's client connections, clearing any state
    /// and freeing the server's socket.
    pub fn shutdown(&mut self) -> Result<(), Error> {
        if self.socket.is_some() {
            self.should_receive = false;
            self.stats_collector.reset();
            self.stats.reset();
            self.events.clear();
            self.connections.clear();
            self.addresses.clear();
            self.ticker.reset();
            self.local_address = None;
            self.socket = None;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    // Internal ---------------------------------------------------------------
    fn receive_connection_packet(
        &mut self,
        id: ConnectionID,
        addr: SocketAddr,
        packet: Vec<u8>

    ) -> usize {

        let packet_length = packet.len();

        // Get existing connection
        if self.connections.contains_key(&id) {

            let connection = self.connections.get_mut(&id).unwrap();

            // Check if the packet was actually consumed by the connection.
            //
            // If it was, see if the address we received the packet from differs
            // from the last known address of the connection.
            //
            // If it does, we re-map the connections address to the new one,
            // effectively tracking the clients sending / receiving port.
            //
            // This is done so that when the clients NAT decides to switch the
            // port the connection doesn't end up sending packets into the void.
            if connection.receive_packet(packet) && addr != connection.peer_addr() {
                connection.set_peer_addr(addr);
                self.addresses.remove(&id);
                self.addresses.insert(id, addr);
            }

            // Map any connection events
            map_connection_events(&mut self.events, connection);

            packet_length

        // Or insert new one
        } else {

            let mut conn = Connection::new(
                self.config,
                self.local_address.unwrap(),
                addr,
                R::new(self.config),
                M::new(self.config)
            );

            conn.set_id(id);

            self.connections.insert(id, conn);
            self.addresses.insert(id, addr);

            // Receive first packet
            let connection = self.connections.get_mut(&id).unwrap();

            if connection.receive_packet(packet) {

                // Map any connection events
                map_connection_events(&mut self.events, connection);

            }

            packet_length

        }

    }

}

// Helpers --------------------------------------------------------------------
fn map_connection_events<R: RateLimiter, M: PacketModifier>(
    server_events: &mut VecDeque<ServerEvent>,
    connection: &mut Connection<R, M>
) {
    let id = connection.id();
    for event in connection.events() {
        server_events.push_back(match event {
            ConnectionEvent::Connected => ServerEvent::Connection(id),
            ConnectionEvent::Lost => ServerEvent::ConnectionLost(id),
            ConnectionEvent::FailedToConnect => unreachable!(),
            ConnectionEvent::Closed(p) => ServerEvent::ConnectionClosed(id, p),
            ConnectionEvent::Message(payload) => ServerEvent::Message(id, payload),
            ConnectionEvent::CongestionStateChanged(c) => ServerEvent::ConnectionCongestionStateChanged(id, c),
            ConnectionEvent::PacketLost(payload) => ServerEvent::PacketLost(id, payload)
        })
    }
}

