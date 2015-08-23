extern crate clock_ticks;

use std::thread;
use std::cmp;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::collections::HashMap;
use traits::socket::Socket;
use shared::udp_socket::UdpSocket;
use super::{Config, Connection, ConnectionID, Handler};

/// Implementation of a multi-client server implementation with handler based
/// event dispatching.
pub struct Server {
    closed: bool,
    config: Config,
    address: Option<SocketAddr>
}

impl Server {

    /// Creates a new server with the given configuration.
    pub fn new(config: Config) -> Server {
        Server {
            closed: false,
            config: config,
            address: None
        }
    }

    /// Returns the local address the server is currently bound to.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.address
    }

    /// Binds the server to the specified local address by creating a socket
    /// and actively listens for incoming client connections.
    ///
    /// Clients connecting to the server must use a compatible connection
    /// / packet configuration in order to be able to connect.
    ///
    /// The `handler` is a struct that implements the `Handler` trait in order
    /// to handle events from the server and its connections.
    pub fn bind<T: ToSocketAddrs>(
        &mut self, handler: &mut Handler<Server>, address: T
    ) -> Result<(), Error> {

        let socket = try!(UdpSocket::new(
            address,
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
    pub fn bind_to_socket<T: Socket>(
        &mut self, handler: &mut Handler<Server>, mut socket: T
    ) -> Result<(), Error> {

        // Extract bound address
        self.address = Some(try!(socket.local_addr()));

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

            // Get current time to correct tick delay in order to achieve
            // a more stable tick rate
            let begin = clock_ticks::precise_time_ns();

            // Receive all incoming UDP packets to our local address
            while let Ok((addr, packet)) = reader.try_recv() {

                // Try to extract the connection id from the packet
                match Connection::id_from_packet(&self.config, &packet) {
                    Some(id) => {

                        if !connections.contains_key(&id) {

                            // In case of a unknown ConnectionID we create a
                            // new connection and map it to the id
                            let conn = Connection::new(
                                self.config,
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
        self.address = None;

        // Reset all connection states
        for (_, conn) in connections.iter_mut() {
            conn.reset();
        }

        // Close the UDP socket
        socket.shutdown();

        Ok(())

    }

    /// Shuts down the server, closing all active connections.
    pub fn shutdown(&mut self) {
        self.closed = true;
    }

}


#[cfg(test)]
mod tests {

    extern crate clock_ticks;

    use std::net;
    use std::thread;
    use std::io::Error;
    use std::collections::HashMap;
    use std::sync::mpsc::channel;
    use super::super::traits::socket::SocketReader;
    use super::super::{
        Connection, ConnectionID, Config, Handler, Server, Socket
    };

    fn precise_time_ms() -> u32 {
        (clock_ticks::precise_time_ns() / 1000000) as u32
    }

    struct DelayServerHandler {
        pub last_tick_time: u32,
        pub tick_count: u32,
        pub accumulated: u32
    }

    impl Handler<Server> for DelayServerHandler {

        fn bind(&mut self, _: &mut Server) {
            self.last_tick_time = precise_time_ms();
        }

        fn tick_connections(
            &mut self, server: &mut Server,
            _: &mut HashMap<ConnectionID, Connection>
        ) {

            // Accumulate time so we can check that the artifical delay
            // was correct for by the servers tick loop
            if self.tick_count > 1 {
                self.accumulated += precise_time_ms() - self.last_tick_time;
            }

            self.last_tick_time = precise_time_ms();
            self.tick_count += 1;

            if self.tick_count == 5 {
                server.shutdown();
            }

            // Fake some load inside of the tick handler
            thread::sleep_ms(75);

        }

    }

    #[test]
    fn test_server_tick_delay() {

        let config = Config {
            send_rate: 10,
            .. Config::default()
        };

        let mut handler = DelayServerHandler {
            last_tick_time: 0,
            tick_count: 0,
            accumulated: 0
        };

        let mut server = Server::new(config);
        server.bind(&mut handler, "127.0.0.1:0").unwrap();

        assert!(handler.accumulated <= 350);

    }

    fn check_messages(conn: &mut Connection) {

        let mut messages = Vec::new();
        for m in conn.received() {
            messages.push(m);
        }

        assert_eq!(messages, [
            [72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100].to_vec()

        ].to_vec())

    }

    struct ConnectionServerHandler {
        pub connection_count: i32
    }

    impl Handler<Server> for ConnectionServerHandler {

        fn connection(&mut self, _: &mut Server, _: &mut Connection) {
            self.connection_count += 1;
        }

        fn tick_connections(
            &mut self, server: &mut Server,
            connections: &mut HashMap<ConnectionID, Connection>
        ) {

            // expect 1 message from each connection
            for (id, conn) in connections.iter_mut() {
                match *id {
                    ConnectionID(1) => check_messages(conn),
                    ConnectionID(2) => check_messages(conn),
                    _ => unreachable!("Invalid connection ID")
                }
            }

            server.shutdown();

        }

    }

    #[test]
    fn test_server_connections() {

        let config = Config::default();

        let mut handler = ConnectionServerHandler {
            connection_count: 0
        };

        let mut socket = MockSocket::new([

            // create a new connection from address 1
            ("127.0.0.1:1234", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 1, // Connection ID
                0, 0,
                0, 0, 0, 0

            ].to_vec()),

            // create a new connection from address 2
            ("127.0.0.1:5678", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 2, // Connection ID
                0, 0,
                0, 0, 0, 0

            ].to_vec()),

            // send message from address 1
            ("127.0.0.1:1234", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 1, // Connection ID
                1, 0,
                0, 0, 0, 0,

                // Hello World
                0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

            ].to_vec()),

            // send message from address 2
            ("127.0.0.1:5678", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 2, // Connection ID
                1, 0,
                0, 0, 0, 0,

                // Hello World
                0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

            ].to_vec())

        ].to_vec());

        // Expect one packet to be send to each connection
        socket.expect([
            // TODO order depends on how the ID is hashed...
            ("127.0.0.1:1234", [
                1, 2, 3, 4,  // Protocol Header
                0, 0, 0, 0,  // Ignored Connection ID
                0, 1,
                0, 0, 0, 1

            ].to_vec()),

            ("127.0.0.1:5678", [
                1, 2, 3, 4,  // Protocol Header
                0, 0, 0, 0,  // Ignored Connection ID
                0, 1,
                0, 0, 0, 1

            ].to_vec())

        ].to_vec());

        let mut server = Server::new(config);
        server.bind_to_socket(&mut handler, socket).unwrap();

        // expect 2 connections
        assert_eq!(handler.connection_count, 2);

    }

    struct ConnectionRemapServerHandler {
        pub connection_count: i32
    }

    impl Handler<Server> for ConnectionRemapServerHandler {

        fn connection(&mut self, _: &mut Server, conn: &mut Connection) {
            let ip = net::Ipv4Addr::new(127, 0, 0, 1);
            let addr = net::SocketAddr::V4(net::SocketAddrV4::new(ip, 1234));
            assert_eq!(conn.peer_addr(), addr);
            self.connection_count += 1;
        }

        fn tick_connections(
            &mut self, server: &mut Server,
            connections: &mut HashMap<ConnectionID, Connection>
        ) {

            // expect 1 message from the connection
            for (id, conn) in connections.iter_mut() {
                let ip = net::Ipv4Addr::new(127, 0, 0, 1);
                let addr = net::SocketAddr::V4(net::SocketAddrV4::new(ip, 5678));
                assert_eq!(*id, ConnectionID(1));
                assert_eq!(conn.peer_addr(), addr);
                check_messages(conn);
            }

            server.shutdown();

        }

    }

    #[test]
    fn test_server_connection_remapping() {

        let config = Config::default();

        let mut handler = ConnectionRemapServerHandler {
            connection_count: 0
        };

        let mut socket = MockSocket::new([

            // create a new connection from address 1
            ("127.0.0.1:1234", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 1, // Connection ID
                0, 0,
                0, 0, 0, 0,

            ].to_vec()),

            // send message from address 2, for connection 1
            ("127.0.0.1:5678", [
                1, 2, 3, 4, // Protocol Header
                0, 0, 0, 1, // Connection ID
                1, 0,
                0, 0, 0, 0,

                // Hello World
                0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

            ].to_vec())

        ].to_vec());

        // Expect one packet for connection 1 to be send to address 2
        socket.expect([
            ("127.0.0.1:5678", [
                1, 2, 3, 4,  // Protocol Header
                0, 0, 0, 0,  // Ignored Connection ID
                0, 1,
                0, 0, 0, 1

            ].to_vec())

        ].to_vec());

        let mut server = Server::new(config);
        server.bind_to_socket(&mut handler, socket).unwrap();

        // expect 1 connection
        assert_eq!(handler.connection_count, 1);

    }

    // Generic receicing socket mock
    struct MockSocket {
        messages: Option<Vec<(&'static str, Vec<u8>)>>,
        send_packets: Vec<(&'static str, Vec<u8>)>,
        send_count: usize
    }

    impl MockSocket {

        pub fn new(messages: Vec<(&'static str, Vec<u8>)>) -> MockSocket {
            MockSocket {
                send_count: 0,
                send_packets: Vec::new(),
                messages: Some(messages)
            }
        }

        pub fn expect(&mut self, send_packets: Vec<(&'static str, Vec<u8>)>) {
            self.send_count = send_packets.len();
            self.send_packets = send_packets;
        }

    }

    impl Socket for MockSocket {

        fn reader(&mut self) -> Option<SocketReader> {

            let (sender, receiver) = channel::<(net::SocketAddr, Vec<u8>)>();
            for (addr, data) in self.messages.take().unwrap().into_iter() {
                let src: net::SocketAddr = addr.parse().ok().unwrap();
                sender.send((src, data.clone())).ok();
            }

            Some(receiver)

        }

        fn send<T: net::ToSocketAddrs>(
            &mut self, addr: T, data: &[u8])
        -> Result<usize, Error> {

            // Don't run out of expected packets
            if self.send_packets.len() == 0 {
                panic!(format!("Expected at most {} packet(s) to be send over socket.", self.send_count));
            }

            let addr_to: net::SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();

            // Search for the next packet with the matching address
            let mut index: i32 = -1;
            for (i, p) in self.send_packets.iter().enumerate() {

                // Verify receiver address
                let to: net::SocketAddr = p.0.parse().ok().unwrap();
                if to == addr_to {
                    index = i as i32;
                    break;
                }

            }

            if index == -1 {
                panic!(format!("Expected no more packet(s) to be send over socket to address {}", addr_to));
            }

            let expected = self.send_packets.remove(index as usize);

            // Verify packet data we ignore the connection ID here since it is
            // random and cannot be accessed by the mockes
            assert_eq!(&data[0..4], &expected.1[0..4]);
            assert_eq!(&data[8..], &expected.1[8..]);

            Ok(0)

        }

        fn local_addr(&self) -> Result<net::SocketAddr, Error> {
            let ip = net::Ipv4Addr::new(0, 0, 0, 0);
            Ok(net::SocketAddr::V4(net::SocketAddrV4::new(ip, 12345)))
        }

        fn shutdown(&mut self) {

        }

    }

}

