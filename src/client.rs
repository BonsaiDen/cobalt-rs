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
use std::collections::VecDeque;


// Internal Dependencies ------------------------------------------------------
use shared::stats::{Stats, StatsCollector};
use shared::tick;
use super::{
    Config,
    Connection, ConnectionEvent,
    MessageKind,
    RateLimiter, PacketModifier, Socket
};


/// Enum of client related network events.
#[derive(Debug, PartialEq)]
pub enum ClientEvent {

    /// Emitted once a connection to a server has been established.
    Connection,

    /// Emitted when a initial connection attempt to a server failed.
    ConnectionFailed,

    /// Emitted when a existing connection to a server is lost.
    ConnectionLost,

    /// Emitted when a connection is closed programmatically.
    ConnectionClosed(bool),

    /// Emitted for each message received from a server.
    Message(Vec<u8>),

    /// Emitted for each packet which was not confirmed by a server
    /// within the specified limits.
    PacketLost(Vec<u8>),

    /// Emitted each time the connection's congestion state changes.
    ConnectionCongestionStateChanged(bool)

}

/// Implementation of a low latency socket client.
///
/// # Basic Usage
///
/// ```
/// use cobalt::{
///     BinaryRateLimiter, Client, Config, NoopPacketModifier, MessageKind, UdpSocket
/// };
///
/// // Create a new client that communicates over a udp socket
/// let mut client = Client::<UdpSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
///
/// // Initiate a connection to the server
/// client.connect("127.0.0.1:1234").expect("Failed to bind to socket");
///
/// // loop {
///
///     // Fetch connection events
///     while let Ok(event) = client.receive() {
///         // Handle events (e.g. Connection, Messages, etc.)
///     }
///
///     // Schedule a message to the send to the server
///     client.send(MessageKind::Instant, b"Ping".to_vec());
///
///     // Flush all pending outgoing messages.
///     //
///     // Also auto delay the current thread to achieve the configured tick rate.
///     client.flush(true);
///
/// // }
///
/// // Close the connection and unbind its socket
/// client.close();
/// ```
#[derive(Debug)]
pub struct Client<S: Socket, R: RateLimiter, M: PacketModifier> {
    config: Config,
    socket: Option<S>,
    connection: Option<Connection<R, M>>,
    tick_start: u64,
    tick_overflow: u64,
    peer_address: Option<SocketAddr>,
    local_address: Option<SocketAddr>,
    events: VecDeque<ClientEvent>,
    should_receive: bool,
    stats_collector: StatsCollector,
    stats: Stats
}

impl<S: Socket, R: RateLimiter, M: PacketModifier> Client<S, R, M> {

    /// Creates a new client with the given configuration.
    pub fn new(config: Config) -> Client<S, R, M> {
        Client {
            config: config,
            socket: None,
            connection: None,
            tick_start: 0,
            tick_overflow: 0,
            peer_address: None,
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

    /// Returns the address of the server the client is currently connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr, Error> {
        self.peer_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns the local address that the client is sending from.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns a mutable reference to underlying connection to the server.
    pub fn connection(&mut self) -> Result<&mut Connection<R, M>, Error> {
        if let Some(connection) = self.connection.as_mut() {
            Ok(connection)

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Returns a mutable reference to the client's underlying socket.
    pub fn socket(&mut self) -> Result<&mut S, Error> {
        if let Some(socket) = self.socket.as_mut() {
            Ok(socket)

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Returns the client's current configuration.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Overrides the client's current configuration.
    pub fn set_config(&mut self, config: Config) {

        self.config = config;
        self.stats_collector.set_config(config);

        if let Some(connection) = self.connection.as_mut() {
            connection.set_config(config);
        }

    }

    /// Establishes a connection with the server at the specified address.
    pub fn connect<A: ToSocketAddrs>(&mut self, addr: A) -> Result<(), Error> {

        if self.socket.is_none() {

            let socket = try!(S::new(
                "0.0.0.0:0",
                self.config.packet_max_size
            ));

            let peer_addr = try!(addr.to_socket_addrs()).nth(0).unwrap();
            let local_addr = try!(socket.local_addr());

            self.socket = Some(socket);
            self.peer_address = Some(peer_addr);
            self.local_address = Some(local_addr);

            self.connection = Some(Connection::new(
                self.config,
                local_addr,
                peer_addr,
                R::new(self.config),
                M::new(self.config)
            ));

            self.should_receive = true;

            Ok(())

        } else {
            Err(Error::new(ErrorKind::AlreadyExists, ""))
        }

    }

    /// Receives the next incoming message from the client's underlying
    /// connection.
    pub fn receive(&mut self) -> Result<ClientEvent, TryRecvError> {

        if self.socket.is_none() {
            Err(TryRecvError::Disconnected)

        } else {

            if self.should_receive {

                self.tick_start = tick::start();

                let tick_delay = 1000000000 / self.config.send_rate;
                let peer_address = self.peer_address.unwrap();

                // Receive all incoming UDP packets to our local address
                let mut bytes_received = 0;
                while let Ok((addr, packet)) = self.socket.as_mut().unwrap().try_recv() {
                    if addr == peer_address {
                        bytes_received += packet.len();
                        self.connection.as_mut().unwrap().receive_packet(
                            packet,
                            tick_delay / 1000000
                        );
                    }
                }

                self.stats_collector.set_bytes_received(bytes_received as u32);

                // Map connection events
                for e in self.connection.as_mut().unwrap().events() {
                    self.events.push_back(match e {
                        ConnectionEvent::Connected => ClientEvent::Connection,
                        ConnectionEvent::Failed => ClientEvent::ConnectionFailed,
                        ConnectionEvent::Lost => ClientEvent::ConnectionLost,
                        ConnectionEvent::Closed(p) => ClientEvent::ConnectionClosed(p),
                        ConnectionEvent::Message(payload) => ClientEvent::Message(payload),
                        ConnectionEvent::CongestionStateChanged(c) => ClientEvent::ConnectionCongestionStateChanged(c),
                        ConnectionEvent::PacketLost(payload) => ClientEvent::PacketLost(payload)
                    });
                }

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
    /// be send with the next `flush` call.
    pub fn send(&mut self, kind: MessageKind, payload: Vec<u8>) -> Result<(), Error> {
        if self.socket.is_some() {
            self.connection.as_mut().unwrap().send(kind, payload);
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Sends all queued messages over the client's underlying connection.
    pub fn flush(&mut self, auto_delay: bool) -> Result<(), Error> {
        if self.socket.is_some() {

            let peer_address = self.peer_address.unwrap();
            let bytes_sent = self.connection.as_mut().unwrap().send_packet(
                self.socket.as_mut().unwrap(),
                &peer_address
            );

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

    /// Resets the client, clearing all pending events and dropping any
    /// connection to the server, returning it into the `Connecting`
    /// state.
    ///
    /// This can be used to re-try a connection attempt if a previous one has
    /// failed.
    pub fn reset(&mut self) -> Result<(), Error> {
        if self.socket.is_some() {
            self.connection.as_mut().unwrap().reset();
            self.stats_collector.reset();
            self.stats = self.stats_collector.average();
            self.events.clear();
            self.tick_start = 0;
            self.tick_overflow = 0;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Drops the client's connection to the server, freeing the socket and
    /// clearing any state.
    pub fn close(&mut self) -> Result<(), Error> {
        if self.socket.is_some() {
            self.reset().ok();
            self.should_receive = false;
            self.peer_address = None;
            self.local_address = None;
            self.connection = None;
            self.socket = None;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

}

