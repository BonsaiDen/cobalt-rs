extern crate clock_ticks;

use std::cmp;
use std::thread;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use super::{Config, Connection, Handler};

/// Implementation of a single-server client implementation with handler based
/// event dispatching.
pub struct Client {
    closed: bool,
    config: Config,
    peer_address: Option<SocketAddr>,
    local_address: Option<SocketAddr>
}

impl Client {

    /// Creates a new client with the given configuration.
    pub fn new(config: Config) -> Client {
        Client {
            closed: false,
            config: config,
            peer_address: None,
            local_address: None
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

    /// Establishes a connection with the server at the specified address by
    /// creating a local socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    pub fn connect<T: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, address: T
    ) -> Result<(), Error> {

        let socket = try!(UdpSocket::new(
            "127.0.0.1:0",
            self.config.packet_max_size
        ));

        self.connect_from_socket(handler, address, socket)

    }

    /// Establishes a connection with the server at the specified address by
    /// using the specified socket for message sending.
    ///
    /// The server must use a compatible connection / packet configuration.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    pub fn connect_from_socket<T: Socket, A: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, address: A, mut socket: T
    ) -> Result<(), Error> {

        // Parse remote address and create connection
        let peer_addr = try!(address.to_socket_addrs()).next().unwrap();
        let mut connection = Connection::new(
            self.config,
            peer_addr,
            handler.rate_limiter(&self.config)
        );

        // Store socket addresses
        self.peer_address = Some(peer_addr);
        self.local_address = Some(try!(socket.local_addr()));

        // Extract packet reader
        let reader = socket.reader().unwrap();

        // Invoke handler
        handler.connect(self);

        // Receive and send until we get closed.
        while !self.closed {

            // Get current time to correct tick delay in order to achieve
            // a more stable tick rate
            let begin = clock_ticks::precise_time_ns();

            // Receive all incoming UDP packets from the specified remote
            // address feeding them into out connection object for parsing
            while let Ok((addr, packet)) = reader.try_recv() {
                if addr == peer_addr {
                    connection.receive_packet(packet, self, handler);
                }
            }

            // Invoke handler
            handler.tick_connection(self, &mut connection);

            // Invoke the connection to send a outgoing packet
            connection.send_packet(&mut socket, &peer_addr, self, handler);

            // Limit ticks per second to the configured amount
            let spend = (clock_ticks::precise_time_ns() - begin) / 1000000 ;
            thread::sleep_ms(
                cmp::max(1000 / self.config.send_rate - spend as u32, 0)
            );

        }

        // Invoke handler
        handler.close(self);

        // Reset socket addresses
        self.peer_address = None;
        self.local_address = None;

        // Reset connection state
        connection.reset();

        // Close the UDP socket
        socket.shutdown();

        Ok(())

    }

    /// Closes the connection to the server.
    ///
    /// This exits the tick loop, resets the connection and shuts down the
    /// underlying socket the client was sending from.
    pub fn close(&mut self) -> Result<(), Error>{
        if self.closed {
            Err(Error::new(ErrorKind::NotConnected, ""))

        } else {
            self.closed = true;
            Ok(())
        }
    }

}

