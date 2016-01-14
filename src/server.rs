// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::cmp;
use std::thread;
use std::time::Duration;
use std::io::{Error, ErrorKind};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use shared::stats::{StatsCollector, Stats};
use super::{Config, Connection, ConnectionID, Handler};

/// Implementation of a multi-client server with handler based event dispatch.
#[derive(Debug)]
pub struct Server {
    closed: bool,
    config: Config,
    local_address: Option<SocketAddr>,
    statistics: StatsCollector
}

impl Server {

    /// Creates a new server with the given configuration.
    pub fn new(config: Config) -> Server {
        Server {
            closed: false,
            config: config,
            local_address: None,
            statistics: StatsCollector::new(config)
        }
    }

    /// Returns the local address that the server is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.local_address.ok_or(Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&mut self) -> Stats {
        self.statistics.average()
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

        // Reset stats
        self.statistics.reset();

        // List of dropped connections
        let mut dropped: Vec<ConnectionID> = Vec::new();

        // Mappping of connections to their remote sender address
        let mut addresses: HashMap<ConnectionID, SocketAddr> = HashMap::new();

        // Mapping of the actual connection objects
        let mut connections: HashMap<ConnectionID, Connection> = HashMap::new();

        // Invoke handler
        handler.bind(self);

        // Receive and send until we shut down.
        while !self.closed {

            // Get current time to correct tick delay in order to achieve
            // a more stable tick rate
            let begin = clock_ticks::precise_time_ns();
            let tick_delay = 1000000000 / self.config.send_rate;

            // Receive all incoming UDP packets to our local address
            let mut bytes_received = 0;
            while let Ok((addr, packet)) = socket.try_recv() {

                // Try to extract the connection id from the packet
                match Connection::id_from_packet(&self.config, &packet) {
                    Some(id) => {

                        // Retrieve or create a connection for the current
                        // connection id
                        let connection = connections.entry(id).or_insert_with(|| {

                            // Also map the intitial address which is used by
                            // the connection
                            addresses.insert(id, addr);

                            let mut conn = Connection::new(
                                self.config,
                                socket.local_addr().unwrap(),
                                addr,
                                handler.rate_limiter(&self.config)
                            );

                            conn.set_id(id);
                            conn

                        });

                        // Map the current remote address of the connection to
                        // the latest address that sent a packet for the
                        // connection id in question. This is done in order to
                        // work in situations were the remote port of a
                        // connection is switched around by NAT.
                        if addr != connection.peer_addr() {
                            connection.set_peer_addr(addr);
                            addresses.remove(&id).unwrap();
                            addresses.insert(id, addr);
                        }

                        // Statistics
                        bytes_received += packet.len();

                        // Then feed the packet into the connection object for
                        // parsing
                        connection.receive_packet(
                            packet, tick_delay / 1000000, self, handler
                        );

                    },
                    None => { /* Ignore any invalid packets */ }
                }

            }

            self.statistics.set_bytes_received(bytes_received as u32);

            // Invoke handler
            handler.tick_connections(self, &mut connections);

            // Create outgoing packets for all connections
            let mut bytes_sent = 0;
            for (id, conn) in connections.iter_mut() {

                // Resolve the last known remote address for this
                // connection and send the data
                let addr = addresses.get(id).unwrap();

                // Then invoke the connection to send a outgoing packet
                bytes_sent += conn.send_packet(&mut socket, addr, self, handler);

                // Collect all lost / closed connections
                if conn.open() == false {
                    dropped.push(*id);
                }

            }

            // Update statistics
            self.statistics.set_bytes_sent(bytes_sent);
            self.statistics.tick();

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
            let spend = clock_ticks::precise_time_ns() - begin;
            thread::sleep(Duration::new(0, cmp::max(
                1000000000 / self.config.send_rate - spend as u32,
                0
            )));

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

