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

        // Accumulate time so we can check that the artificial delay
        // was correct for by the servers tick loop
        if self.tick_count > 1 {
            self.accumulated += precise_time_ms() - self.last_tick_time;
        }

        self.last_tick_time = precise_time_ms();
        self.tick_count += 1;

        if self.tick_count == 5 {
            server.shutdown().unwrap();
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

        server.shutdown().unwrap();

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

        server.shutdown().unwrap();

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

// Generic receiving socket mock
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
        // random and cannot be accessed by the mocks
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

