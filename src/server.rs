extern crate clock_ticks;

use std::cmp;
use std::thread;
use std::io::{Error, ErrorKind};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use super::{Config, Connection, ConnectionID, Handler};

/// Implementation of a multi-client server with handler based event dispatch.
pub struct Server {
    closed: bool,
    config: Config,
    local_address: Option<SocketAddr>
}

impl Server {

    /// Creates a new server with the given configuration.
    pub fn new(config: Config) -> Server {
        Server {
            closed: false,
            config: config,
            local_address: None
        }
    }

    /// Returns the local address that the server is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or(Error::new(ErrorKind::AddrNotAvailable, ""))
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

        // Store bound socket address
        self.local_address = Some(try!(socket.local_addr()));

        // Create connection management collections
        let mut dropped: Vec<ConnectionID> = Vec::new();
        let mut addresses: HashMap<ConnectionID, SocketAddr> = HashMap::new();
        let mut connections: HashMap<ConnectionID, Connection> = HashMap::new();

        // Invoke handler
        handler.bind(self);

        // Receive and send until we shut down.
        while !self.closed {

            // Get current time to correct tick delay in order to achieve
            // a more stable tick rate
            let begin = clock_ticks::precise_time_ns();

            // Receive all incoming UDP packets to our local address
            while let Ok((addr, packet)) = socket.try_recv() {

                // Try to extract the connection id from the packet
                match Connection::id_from_packet(&self.config, &packet) {
                    Some(id) => {

                        if !connections.contains_key(&id) {

                            // In case of a unknown ConnectionID we create a
                            // new connection and map it to the id
                            let conn = Connection::new(
                                self.config,
                                socket.local_addr().unwrap(),
                                addr,
                                handler.rate_limiter(&self.config)
                            );

                            connections.insert(id, conn);

                            // We also map the initial peer address to the id.
                            // this is done in order to reliable track the
                            // connection in case of address re-assignments by
                            // network address translation.
                            addresses.insert(id, addr);

                        }

                        // Check for changes in the peer address and update
                        // the address to ID mapping
                        let connection = connections.get_mut(&id).unwrap();
                        if addr != connection.peer_addr() {
                            connection.set_peer_addr(addr);
                            addresses.remove(&id).unwrap();
                            addresses.insert(id, addr);
                        }

                        // Then feed the packet into the connection object for
                        // parsing
                        connection.receive_packet(packet, self, handler);

                    },
                    None => { /* Ignore any invalid packets */ }
                }

            }

            // Invoke handler
            handler.tick_connections(self, &mut connections);

            // Create outgoing packets for all connections
            for (id, conn) in connections.iter_mut() {

                // Resolve the last known remote address for this
                // connection and send the data
                let addr = addresses.get(id).unwrap();

                // Then invoke the connection to send a outgoing packet
                conn.send_packet(&mut socket, addr, self, handler);

                // Collect all lost / closed connections
                if conn.open() == false {
                    dropped.push(*id);
                }

            }

            // Remove any dropped connections and their address mappings
            if dropped.is_empty() == false {

                for id in dropped.iter() {
                    connections.remove(id).unwrap().reset();
                    addresses.remove(id).unwrap();
                }

                dropped.clear();

            }

            // Calculate spend time in current loop iteration and limit ticks
            // accordingly
            let spend = (clock_ticks::precise_time_ns() - begin) / 1000000 ;
            thread::sleep_ms(
                cmp::max(1000 / self.config.send_rate - spend as u32, 0)
            );

        }

        // Invoke handler
        handler.shutdown(self);

        // Reset socket address
        self.local_address = None;

        // Reset all connection states
        for (_, conn) in connections.iter_mut() {
            conn.reset();
        }

        // Close the UDP socket
        socket.shutdown();

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

}

