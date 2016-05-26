// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::net;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;

use super::mock::{MockSocket, precise_time_ms};
use super::super::{
    Connection, ConnectionID, Config, Handler, Server, Stats
};

#[test]
fn test_server_tick_delay() {

    struct DelayServerHandler {
        pub last_tick_time: u32,
        pub tick_count: u32,
        pub accumulated: i32
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
                self.accumulated += (precise_time_ms() - self.last_tick_time) as i32;
            }

            self.last_tick_time = precise_time_ms();
            self.tick_count += 1;

            if self.tick_count == 5 {
                server.shutdown().unwrap();
            }

            // Fake some load inside of the tick handler
            let before = precise_time_ms();
            thread::sleep(Duration::from_millis(75));

            // Compensate for slow timers
            let spend = (precise_time_ms() - before) as i32;
            self.accumulated -= spend - 75;

        }

    }

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

#[test]
fn test_server_connections() {

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
                    ConnectionID(1...2) => check_messages(conn),
                    _ => unreachable!("Invalid connection ID")
                }
            }

            server.shutdown().unwrap();

        }

    }

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

#[test]
fn test_server_connection_remapping() {

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

#[test]
fn test_server_stats() {

    struct ServerStatsHandler {
        pub tick_count: u32,
    }

    impl Handler<Server> for ServerStatsHandler {

        fn connect(&mut self, _: &mut Server) {
        }

        fn tick_connections(
            &mut self, server: &mut Server,
            _: &mut HashMap<ConnectionID, Connection>
        ) {

            //conn.send(MessageKind::Instant, b"Hello World".to_vec());
            self.tick_count += 1;

            if self.tick_count == 20 {
                server.shutdown().unwrap();
            }

        }

    }

    let config = Config {
        send_rate: 20,
        .. Config::default()
    };

    let mut handler = ServerStatsHandler {
        tick_count: 0
    };

    let mut server = Server::new(config);
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

    assert_eq!(server.stats(), Stats {
        bytes_sent: 0,
        bytes_received: 0
    });

}

