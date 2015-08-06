use std::thread;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::collections::HashMap;
use shared::{Config, Connection, ConnectionID, Handler, UdpSocket};

/// A multi-client server that uses a reliable UDP connection for
/// unreliable packet transmission.
pub struct Server {
    closed: bool,
    config: Config
}

impl Server {

    /// Creates a new server with the given connection configuration.
    pub fn new(config: Config) -> Server {
        Server {
            closed: false,
            config: config
        }
    }

    /// Tries to bind a reliable UDP based server to the specified local
    /// address which actively listens and manages incoming client connections.
    ///
    /// The clients must use a compatible connection configuration in order for
    /// connections to be actually established.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the server and its connections.
    pub fn bind<T: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Server>, address: T
    ) -> Result<(), Error> {

        // Internal ticker for send rate control
        let mut tick = 0;
        let tick_delay = 1000 / self.config.send_rate;

        // Create the UDP socket
        let mut socket = try!(UdpSocket::new(
            address,
            self.config.packet_max_size
        ));

        // Extract packet reader
        let reader = socket.reader().unwrap();

        // Create connection management collections
        let mut dropped: Vec<ConnectionID> = Vec::new();
        let mut addresses: HashMap<ConnectionID, SocketAddr> = HashMap::new();
        let mut connections: HashMap<ConnectionID, Connection> = HashMap::new();

        // Invoke handler
        handler.bind(self);

        // Receive and send until we shut down.
        while !self.closed {

            // Receive all incoming UDP packets to our local address
            while let Ok((addr, packet)) = reader.try_recv() {

                // Try to extract the connection id from the packet
                match Connection::id_from_packet(&self.config, &packet) {
                    Some(id) => {

                        // In case of a unknown connection id create a new
                        // connection and insert it into out hash map
                        if !connections.contains_key(&id) {
                            connections.insert(
                                id, Connection::new(self.config)
                            );
                        }

                        // In addition map the sender address to the connection
                        // id, this is done in order to reliable track the
                        // connection in case of address re-assignments in some
                        // NAT.
                        addresses.insert(id, addr);

                        // Then feed the packet into the connection object for
                        // parsing
                        connections.get_mut(&id).unwrap().receive(
                            packet, self, handler
                        );

                    },
                    None => { /* Ignore any invalid packets */ }
                }

            }

            // Invoke handler
            handler.tick_connections(self, &mut connections);

            // Create outgoing packets for all connections
            for (id, conn) in connections.iter_mut() {

                // If not congested send at full rate otherwise send
                // at reduced rate
                if !conn.is_congested() ||
                    tick % self.config.congestion_divider == 0 {

                    // Resolve the last known remote address for this
                    // connection and send the data
                    let addr = addresses.get(id).unwrap();

                    // Then invoke the connection to send a outgoing packet
                    conn.send(&mut socket, addr, self, handler);

                }

                // Collect all lost / closed connections
                if conn.is_open() == false {
                    dropped.push(*id);
                }

            }

            // Remove any dropped connections and their address mappings
            for id in dropped.iter() {
                connections.remove(id).unwrap();
                addresses.remove(id).unwrap();
            }

            dropped.clear();

            // Next Tick
            thread::sleep_ms(tick_delay);
            tick += 1;

            if tick == self.config.send_rate {
                tick = 0;
            }

        }

        // Invoke handler
        handler.shutdown(self);

        // Reset all connection states
        for (_, conn) in connections.iter_mut() {
            conn.reset();
        }

        // Close the UDP socket
        socket.shutdown();

        Ok(())

    }

    /// Shutsdown the server, closing all its active connections.
    pub fn shutdown(&mut self) {
        self.closed = true;
    }

}

