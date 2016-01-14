extern crate clock_ticks;

use std::cmp;
use std::thread;
use std::time::Duration;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::stats::{StatsCollector, Stats};
use shared::udp_socket::UdpSocket;
use super::{Config, Connection, Handler};

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
            statistics: StatsCollector::new(config.send_rate)
        }
    }

    /// Returns the address of the server the client is currently connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr, Error> {
        self.peer_address.ok_or(Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns the local address that the client is sending from.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or(Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&mut self) -> Stats {
        self.statistics.average()
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

        while self.running {

            // Get current time to correct tick delay in order to achieve
            // a more stable tick rate
            let begin = clock_ticks::precise_time_ns();
            let tick_delay = 1000000000 / self.config.send_rate;

            self.receive_sync(handler, &mut state, tick_delay / 1000000);
            self.tick_sync(handler, &mut state);
            self.send_sync(handler, &mut state);

            // Limit ticks per second to the configured amount
            //
            // Note: This will get swapped out with the more precise Duration
            // API once thread::sleep() is stable.
            let spend = clock_ticks::precise_time_ns() - begin;
            thread::sleep(Duration::new(0, cmp::max(
                tick_delay - spend as u32,
                0
            )));

        }

        self.close_sync(handler, &mut state)

    }

    /// Asynchronously closes the connection to the server.
    ///
    /// This exits the tick loop, resets the connection and shuts down the
    /// underlying socket the client was sending and receiving from.
    pub fn close(&mut self) -> Result<(), Error>{
        if !self.running {
            Err(Error::new(ErrorKind::NotConnected, ""))

        } else {
            self.running = false;
            Ok(())
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

        self.peer_address = Some(peer_addr);
        self.local_address = Some(try!(socket.local_addr()));
        self.statistics.reset();
        self.running = true;
        self.closed = false;

        let connection = Connection::new(
            self.config,
            socket.local_addr().unwrap(),
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
        // address feeding them into out connection object for parsing
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

    /// Performs exactly on tick of the underlying connection.
    pub fn tick_sync<S: Socket>(
        &mut self, handler: &mut Handler<Client>, state: &mut ClientState<S>
    ) {
        if !self.closed {
            handler.tick_connection(self, &mut state.connection);
        }
    }

    /// Sends exactly on outgoing packet from the underlying connection.
    pub fn send_sync<S: Socket>(
        &mut self, handler: &mut Handler<Client>, state: &mut ClientState<S>
    ) {
        if !self.closed {
            let bytes_sent = state.connection.send_packet(
                &mut state.socket, &state.peer_address, self, handler
            );
            self.statistics.set_bytes_sent(bytes_sent);
            self.statistics.tick();
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
            state.socket.shutdown();

            self.peer_address = None;
            self.local_address = None;

            Ok(())

        }

    }

}

/// A structure used for synchronous calls on a `Client` instance.
#[derive(Debug)]
pub struct ClientState<S: Socket> {
    socket: S,
    connection: Connection,
    peer_address: SocketAddr
}

impl <S: Socket>ClientState< S> {

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
            peer_address: peer_addr
        }
    }

    /// Returns the average roundtrip time for the clients underlying
    /// connection.
    pub fn rtt(&self) -> u32 {
        self.connection.rtt()
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across the
    /// client's underlying connection.
    pub fn packet_loss(&self) -> f32 {
        self.connection.packet_loss()
    }

    /// Returns the socket address for the local end of this client's
    /// underlying connection.
    pub fn local_addr(&self) -> SocketAddr {
        self.connection.local_addr()
    }

    /// Returns the socket address for the remote end of this client's
    /// underlying connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.connection.peer_addr()
    }

}

