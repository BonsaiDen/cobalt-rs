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
use shared::tick;
use super::{
    Config,
    ConnectionID, Connection, ConnectionEvent,
    MessageKind,
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
///     // Send a message to all of the servers clients
///     for (_, conn) in server.connections() {
///         conn.send(MessageKind::Instant, b"Ping".to_vec());
///     }
///
///     // Flush all pending outgoing messages.
///     //
///     // Also auto delay the current thread to achieve the configured tick rate.
///     server.flush(false).is_ok();
///
/// // }
///
/// // Shutdown the server (freeing its socket and closing all its connections)
/// server.shutdown().is_ok();
/// ```
///
#[derive(Debug)]
pub struct Server<S: Socket, R: RateLimiter, M: PacketModifier> {
    config: Config,
    socket: Option<S>,
    connections: HashMap<ConnectionID, Connection<R, M>>,
    addresses: HashMap<ConnectionID, SocketAddr>,
    tick_start: u64,
    tick_overflow: u64,
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
            tick_start: 0,
            tick_overflow: 0,
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
        self.stats_collector.set_config(config);
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

                self.tick_start = tick::start();

                let tick_delay = 1000000000 / self.config.send_rate;
                let local_address = self.local_address.unwrap();

                // TODO optimize unnecessary copying
                let config = self.config;

                // Receive all incoming UDP packets to our local address
                let mut bytes_received = 0;
                while let Ok((addr, packet)) = self.socket.as_mut().unwrap().try_recv() {

                    // Try to extract the connection id from the packet
                    if let Some(id) = Connection::<R, M>::id_from_packet(&self.config, &packet) {

                        // Retrieve or create a connection for the current
                        // connection id
                        let mut inserted_address: Option<SocketAddr> = None;
                        let connection = self.connections.entry(id).or_insert_with(|| {

                            let mut conn = Connection::new(
                                config,
                                local_address,
                                addr,
                                R::new(config),
                                M::new(config)
                            );

                            inserted_address = Some(addr);
                            conn.set_id(id);
                            conn

                        });

                        // Also map the intitial address which is used by
                        // the connection
                        if let Some(addr) = inserted_address.take() {
                            self.addresses.insert(id, addr);
                        }

                        // Map the current remote address of the connection to
                        // the latest address that sent a packet for the
                        // connection id in question. This is done in order to
                        // work in situations were the remote port of a
                        // connection is switched around by NAT.
                        if addr != connection.peer_addr() {
                            // TODO verify packet sequence
                            connection.set_peer_addr(addr);
                            self.addresses.remove(&id);
                            self.addresses.insert(id, addr);
                        }

                        // Statistics
                        bytes_received += packet.len();

                        // Then feed the packet into the connection object for
                        // parsing
                        connection.receive_packet(packet, tick_delay / 1000000);

                        // Map connection events
                        for e in connection.events() {
                            self.events.push_back(match e {
                                ConnectionEvent::Connected => ServerEvent::Connection(id),
                                ConnectionEvent::Closed(p) => ServerEvent::ConnectionClosed(id, p),
                                ConnectionEvent::Message(payload) => ServerEvent::Message(id, payload),
                                ConnectionEvent::CongestionStateChanged(c) => ServerEvent::ConnectionCongestionStateChanged(id, c),
                                ConnectionEvent::PacketLost(payload) => ServerEvent::PacketLost(id, payload),
                                _ => unreachable!()
                            })
                        }

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

    /// Queues a message of the specified `kind` along with its `payload` to
    /// be send to the specified client connection with the next `flush` call.
    pub fn send(&mut self, id: &ConnectionID, kind: MessageKind, payload: Vec<u8>) -> Result<(), Error> {
        if self.socket.is_some() {
            if let Some(conn) = self.connections.get_mut(id) {
                conn.send(kind, payload);
                Ok(())

            } else {
                Err(Error::new(ErrorKind::NotFound, ""))
            }

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Sends all queued messages to the server's underlying client connections.
    pub fn flush(&mut self, auto_delay: bool) -> Result<(), Error> {
        if self.socket.is_some() {

            // List of dropped connections
            let mut dropped: Vec<ConnectionID> = Vec::new();

            // Create outgoing packets for all connections
            let mut bytes_sent = 0;
            for (id, conn) in &mut self.connections {

                // Resolve the last known remote address for this
                // connection and send the data
                let addr = &self.addresses[id];

                // Then invoke the connection to send a outgoing packet
                bytes_sent += conn.send_packet(
                    self.socket.as_mut().unwrap(),
                    addr
                );

                // Collect all lost / closed connections
                if !conn.open() {
                    dropped.push(*id);
                }

            }

            // Remove any dropped connections and their address mappings
            for id in dropped {
                self.events.push_back(ServerEvent::ConnectionLost(id));
                self.connections.remove(&id).unwrap().reset();
                self.addresses.remove(&id);
            }

            self.stats_collector.set_bytes_sent(bytes_sent);
            self.stats_collector.tick();
            self.stats = self.stats_collector.average();

            self.should_receive = true;

            if auto_delay {
                tick::end(
                    1000000000 / self.config.send_rate,
                    self.tick_start,
                    &mut self.tick_overflow,
                    &self.config
                );
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
            self.connections.clear();
            self.addresses.clear();
            self.local_address = None;
            self.socket = None;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

}

