use std::thread;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use super::{Config, Connection, Handler};

/// Implementation of a single-server client implementation with handler based
/// event dispatching.
pub struct Client {
    closed: bool,
    config: Config,
    address: Option<SocketAddr>
}

impl Client {

    /// Creates a new client with the given configuration.
    pub fn new(config: Config) -> Client {
        Client {
            closed: false,
            config: config,
            address: None
        }
    }

    /// Returns the address of the server the client is currently connected to.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.address
    }

    /// Tries to establish connection to the server specified by the address.
    ///
    /// The server must use a compatible configuration in order for
    /// the connection to be actually established.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    pub fn connect<T: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, address: T
    ) -> Result<(), Error> {

        // Create connection and parse remote address
        let peer_addr = try!(address.to_socket_addrs()).next().unwrap();
        let mut connection = Connection::new(
            self.config,
            peer_addr,
            handler.rate_limiter(&self.config)
        );

        // Create the UDP socket
        let mut socket = try!(UdpSocket::new(
            "127.0.0.1:0",
            self.config.packet_max_size
        ));

        self.address = Some(try!(socket.local_addr()));

        // Extract packet reader
        let reader = socket.reader().unwrap();

        // Invoke handler
        handler.connect(self);

        // Receive and send until we get closed.
        while !self.closed {

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
            thread::sleep_ms(1000 / self.config.send_rate);

        }

        // Invoke handler
        handler.close(self);
        self.address = None;

        // Reset connection state
        connection.reset();

        // Close the UDP socket
        socket.shutdown();

        Ok(())

    }

    /// Closes the clients connections to the server.
    pub fn close(&mut self) {
        self.closed = true;
    }

}

