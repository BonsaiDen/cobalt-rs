use std::thread;
use std::io::Error;
use std::net::ToSocketAddrs;
use shared::{Config, Connection, Handler, Socket};

/// A server client that uses a reliable UDP connection for unreliable packet
/// transmission.
pub struct Client {
    closed: bool,
    config: Config
}

impl Client {

    /// Creates a new client with the given connection configuration.
    pub fn new(config: Config) -> Client {
        Client {
            closed: false,
            config: config
        }
    }

    /// Tries to establish a reliable UDP based connection to the server
    /// specified by the address.
    ///
    /// The server must use a compatible connection configuration in order for
    /// the connection to be actually established.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the client and its connection.
    pub fn connect<T: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Client>, address: T
    ) -> Result<(), Error> {

        // Ticker for send rate control
        let mut tick = 0;
        let tick_delay = 1000 / self.config.send_rate;

        // Create connection and parse remote address
        let mut connection = Connection::new(self.config);
        let remote = try!(address.to_socket_addrs()).next().unwrap();

        // Create the UDP socket
        let mut socket = try!(Socket::new(
            "127.0.0.1:0",
            self.config.packet_max_size
        ));

        // Extract packet reader
        let reader = socket.reader().unwrap();

        // Invoke handler
        handler.connect(self);

        // Receive and send until we get closed.
        while !self.closed {

            // Receive all incoming UDP packets from the specified remote
            // address feeding them into out connection object for parsing
            while let Ok((addr, packet)) = reader.try_recv() {
                if addr == remote {
                    connection.receive(packet, self, handler);
                }
            }

            // Invoke handler
            handler.tick_connection(self, &mut connection);

            // Check if we should send a packet on the current tick
            // or whether we should reduce the number of sent packets due to
            // congestion
            if connection.is_congested() == false
                || tick % self.config.congestion_divider == 0 {

                // Then invoke the connection to send a outgoing packet
                connection.send(&mut socket, &remote, self, handler);

            }

            // Limit ticks per second to the configured amount
            thread::sleep_ms(tick_delay);

            tick += 1;

            if tick == self.config.send_rate {
                tick = 0;
            }

        }

        // Invoke handler
        handler.close(self);

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

