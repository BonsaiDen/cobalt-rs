// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::stats::{StatsCollector, Stats};
use shared::udp_socket::UdpSocket;
use super::{Config, ClientStream, Connection, Handler, MessageKind, tick};

/// Implementation of a single-server client with handler based event dispatch.
///
/// There are two ways of creating and connection a client instance:
///
/// 1. Blocking mode to be used in it dedicated thread, with asynchronous
/// `Handler` callbacks, available via the methods **without** the `sync`
/// postfix.
///
/// 2. Non-Blocking mode, with synchronous `Handler` callbacks, available via
/// the methods **with** the `sync` postfix.
#[derive(Debug)]
pub struct Client {
    closed: bool,
    running: bool,
    config: Config,
    peer_address: Option<SocketAddr>,
    local_address: Option<SocketAddr>,
    statistics: StatsCollector
}

impl Client {

    /// Creates a new client with the given configuration.
    pub fn new(config: Config) -> Client {
        Client {
            closed: false,
            running: false,
            config: config,
            peer_address: None,
            local_address: None,
            statistics: StatsCollector::new(config)
        }
    }

    /// Returns the address of the server the client is currently connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr, Error> {
        self.peer_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns the local address that the client is sending from.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or_else(|| Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&mut self) -> Stats {
        self.statistics.average()
    }

    /// Returns a copy of the client's current configuration.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Overrides the client's existing configuration.
    pub fn set_config<S: Socket>(&mut self, config: Config, state: &mut ClientState<S>) {
        self.config = config;
        self.statistics.set_config(config);
        state.set_config(config);
    }

    // Asynchronous, blocking API ---------------------------------------------

    /// Establishes a connection with the server at the specified address by
    /// creating a local socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    ///
    /// This method starts the tick loop, blocking the calling thread.
    pub fn connect<A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, addr: A

    ) -> Result<(), Error> {

        let socket = try!(UdpSocket::new(
            "0.0.0.0:0",
            self.config.packet_max_size
        ));

        self.connect_from_socket(handler, addr, socket)

    }

    /// Establishes a connection with the server at the specified address by
    /// using the specified socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    ///
    /// This method starts the tick loop, blocking the calling thread.
    pub fn connect_from_socket<S: Socket, A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, addr: A, socket: S

    ) -> Result<(), Error> {

        let mut state = try!(
            self.connect_from_socket_sync(handler, addr, socket)
        );

        let tick_delay = 1000000000 / self.config.send_rate;

        let mut tick_overflow = 0;
        while self.running {

            let tick_start = tick::start();

            self.receive_sync(handler, &mut state, tick_delay / 1000000);
            self.tick_sync(handler, &mut state);
            self.send_sync(handler, &mut state);

            tick::end(tick_delay, tick_start, &mut tick_overflow, &self.config);

        }

        self.close_sync(handler, &mut state)

    }

    /// Asynchronously closes the connection to the server.
    ///
    /// This exits the tick loop, resets the connection and shuts down the
    /// underlying socket the client was sending and receiving from.
    pub fn close(&mut self) -> Result<(), Error>{
        if self.running {
            self.running = false;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }


    // Non-Blocking, Synchronous API ------------------------------------------

    /// Establishes a connection with the server at the specified address by
    /// creating a local socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    ///
    /// This method returns a `ClientState` instance for this client, which can
    /// be used with other synchronous `Client` methods.
    pub fn connect_sync<A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, addr: A

    ) -> Result<ClientState<UdpSocket>, Error> {

        let socket = try!(UdpSocket::new(
            "0.0.0.0:0",
            self.config.packet_max_size
        ));

        self.connect_from_socket_sync(handler, addr, socket)

    }

    /// Establishes a connection with the server at the specified address by
    /// using the specified socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    ///
    /// This method returns a `ClientState` instance for this client, which can
    /// be used with other synchronous `Client` methods.
    pub fn connect_from_socket_sync<A: ToSocketAddrs, S: Socket>(
        &mut self, handler: &mut Handler<Client>, addr: A, socket: S

    ) -> Result<ClientState<S>, Error> {

        let peer_addr = try!(addr.to_socket_addrs()).nth(0).unwrap();
        let local_addr = try!(socket.local_addr());

        self.peer_address = Some(peer_addr);
        self.local_address = Some(local_addr);
        self.statistics.reset();
        self.running = true;
        self.closed = false;

        let connection = Connection::new(
            self.config,
            local_addr,
            peer_addr,
            handler.rate_limiter(&self.config)
        );

        handler.connect(self);

        Ok(ClientState::new(socket, connection, peer_addr))

    }

    /// Receives all currently buffered incoming packet from the underlying
    /// connection.
    pub fn receive_sync<S: Socket>(
        &mut self,
        handler: &mut Handler<Client>, state: &mut ClientState<S>,
        tick_delay: u32
    ) {

        // Receive all incoming UDP packets from the specified remote
        // address feeding them into our connection object for parsing
        if !self.closed {
            let mut bytes_received = 0;
            while let Ok((addr, packet)) = state.socket.try_recv() {
                if addr == state.peer_address {
                    bytes_received += packet.len();
                    state.connection.receive_packet(
                        packet, tick_delay, self, handler
                    );
                }
            }
            self.statistics.set_bytes_received(bytes_received as u32);
        }

    }

    /// Performs exactly one tick on the underlying connection.
    pub fn tick_sync<S: Socket>(
        &mut self, handler: &mut Handler<Client>, state: &mut ClientState<S>
    ) {
        if !self.closed {
            handler.tick_connection(self, &mut state.connection);
        }
    }

    /// Sends exactly one outgoing packet from the underlying connection.
    pub fn send_sync<S: Socket>(
        &mut self, handler: &mut Handler<Client>, state: &mut ClientState<S>
    ) {
        if !self.closed {
            let bytes_sent = state.connection.send_packet(
                &mut state.socket, &state.peer_address, self, handler
            );
            self.statistics.set_bytes_sent(bytes_sent);
            self.statistics.tick();
            state.stats = self.statistics.average();
        }
    }

    /// Closes the connection to the server.
    ///
    /// This resets the connection and shuts down the underlying socket the
    /// client was sending and receiving from.
    pub fn close_sync<S: Socket>(
        &mut self, handler: &mut Handler<Client>, state: &mut ClientState<S>

    ) -> Result<(), Error> {

        if self.closed {
            Err(Error::new(ErrorKind::NotConnected, ""))

        } else {

            self.closed = true;

            handler.close(self);
            state.connection.reset();

            self.peer_address = None;
            self.local_address = None;

            Ok(())

        }

    }

    /// Consumes the `Client` instance converting it into a `ClientStream`.
    pub fn into_stream(self) -> ClientStream {
        ClientStream::from_client(self)
    }

}

/// A structure used for synchronous calls on a `Client` instance.
#[derive(Debug)]
pub struct ClientState<S: Socket> {
    socket: S,
    connection: Connection,
    peer_address: SocketAddr,
    stats: Stats
}

impl<S: Socket> ClientState<S> {

    // We need to encapsulate the above objects because they cannot be
    // owned by the client itself without running into issues with multiple
    // bindings of Self when trying to both call a method on it's
    // connection and pass it to that method.
    fn new(
        socket: S,
        connection: Connection,
        peer_addr: SocketAddr

    ) -> ClientState<S> {
        ClientState {
            socket: socket,
            connection: connection,
            peer_address: peer_addr,
            stats: Stats {
                bytes_sent: 0,
                bytes_received: 0
            }
        }
    }

    /// Returns the average roundtrip time for this client's underlying
    /// connection.
    pub fn rtt(&self) -> u32 {
        self.connection.rtt()
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across this
    /// client's underlying connection.
    pub fn packet_loss(&self) -> f32 {
        self.connection.packet_loss()
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second, of this
    /// client's underlying connection.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Returns the socket address for the local end of this client's
    /// underlying connection.
    pub fn local_addr(&self) -> SocketAddr {
        self.connection.local_addr()
    }

    /// Returns the socket address for the remote end of this client's
    /// underlying connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_address
    }

    /// Overrides the configuration of this client's underlying connection.
    pub fn set_config(&mut self, config: Config) {
        self.connection.set_config(config);
    }

    /// Sends a message of the specified `kind` along with its `payload` over
    /// this client's underlying connection.
    pub fn send(&mut self, kind: MessageKind, payload: Vec<u8>) {
        self.connection.send(kind, payload);
    }

    /// Resets this client's underlying connection state.
    pub fn reset(&mut self) {
        self.connection.reset();
    }

}

