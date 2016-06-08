// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::mock::{
    MockSocket,
    MockTickDelayServerHandler,
    MockConnectionServerHandler,
    MockConnectionRemapServerHandler,
    MockServerStatsHandler,
    MockTickRecorder
};
use super::super::{Config, Server, Stats};

#[test]
fn test_server_tick_delay_no_overflow() {

    let mut handler = MockTickDelayServerHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 0.0)
    };

    let mut server = Server::new(Config {
        send_rate: 30,
        tick_overflow_recovery: false,
        tick_overflow_recovery_rate: 0.0,
        .. Config::default()
    });
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

}

#[test]
fn test_server_tick_delay_overflow_half() {

    let mut handler = MockTickDelayServerHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 0.5)
    };

    let mut server = Server::new(Config {
        send_rate: 30,
        tick_overflow_recovery_rate: 0.5,
        .. Config::default()
    });
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

}

#[test]
fn test_server_tick_delay_overflow_one() {

    let mut handler = MockTickDelayServerHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 1.0)
    };

    let mut server = Server::new(Config {
        send_rate: 30,
        tick_overflow_recovery_rate: 1.0,
        .. Config::default()
    });
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

}

#[test]
fn test_server_connections() {

    let socket = MockSocket::from_address("127.0.0.1:0");
    socket.receive(vec![

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

    ]);

    let mut socket_handle = socket.handle();

    // Server
    let config = Config::default();
    let mut server = Server::new(config);
    let mut handler = MockConnectionServerHandler {
        connection_count: 0
    };
    server.bind_to_socket(&mut handler, socket).unwrap();

    // Expect 2 connections in total
    assert_eq!(handler.connection_count, 2);

    // Expect one packet to be send to each connection
    socket_handle.assert_sent_sort_by_addr(vec![
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
    ]);

}

#[test]
fn test_server_connection_remapping() {

    let socket = MockSocket::from_address("127.0.0.1:0");
    socket.receive(vec![

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

    ]);

    let mut socket_handle = socket.handle();

    // Server
    let config = Config::default();
    let mut server = Server::new(config);
    let mut handler = MockConnectionRemapServerHandler {
        connection_count: 0
    };
    server.bind_to_socket(&mut handler, socket).unwrap();

    // Expect one packet for connection 1 to be send to address 2
    socket_handle.assert_sent(vec![
        ("127.0.0.1:5678", [
            1, 2, 3, 4,  // Protocol Header
            0, 0, 0, 0,  // Ignored Connection ID
            0, 1,
            0, 0, 0, 1

        ].to_vec())

    ]);

    // expect 1 connection
    assert_eq!(handler.connection_count, 1);

}

#[test]
fn test_server_stats() {

    let config = Config {
        send_rate: 20,
        .. Config::default()
    };

    let mut handler = MockServerStatsHandler {
        tick_count: 0
    };

    let mut server = Server::new(config);
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

    assert_eq!(server.stats(), Stats {
        bytes_sent: 0,
        bytes_received: 0
    });

}

