// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;


// STD Dependencies -----------------------------------------------------------
use std::thread;
use std::time::Duration;
use std::io::ErrorKind;
use std::sync::mpsc::TryRecvError;


// Internal Dependencies ------------------------------------------------------
use super::mock::MockSocket;
use ::{
    BinaryRateLimiter, ConnectionID, Config, MessageKind, NoopPacketModifier,
    Server, ServerEvent
};


// Macros ---------------------------------------------------------------------
macro_rules! assert_epsilon {
    // TODO re-use macros across tests
    ($value:expr, $target:expr, $difference:expr) => {
        {
            let actual = ($value) as i64;
            let min = $target - $difference;
            let max = $target + $difference;
            if actual < min || actual > max {
                panic!(format!("Value {} not in range {} - {}", $value, min, max));
            }
        }
    }
}


// Tests ----------------------------------------------------------------------
#[test]
fn test_server_configuration() {

    let config = Config {
        send_rate: 30,
        .. Config::default()
    };

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(config);
    assert_eq!(server.config(), config);

    let config = Config {
        send_rate: 60,
        .. Config::default()
    };

    server.set_config(config);
    assert_eq!(server.config(), config);

}

#[test]
fn test_server_internals() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        send_rate: 30,
        .. Config::default()
    });

    assert_eq!(server.socket().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_server_disconnected() {

    let conn_id = ConnectionID(0);

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        send_rate: 30,
        .. Config::default()
    });

    assert_eq!(server.bytes_sent(), 0);
    assert_eq!(server.bytes_received(), 0);

    assert_eq!(server.local_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(server.connection(&conn_id).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(server.connections().len(), 0);

    assert_eq!(server.accept_receive(), Err(TryRecvError::Disconnected));
    assert_eq!(server.send(&conn_id, MessageKind::Instant, Vec::new()).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(server.flush(false).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(server.shutdown().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_server_listen_shutdown() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(server.listen("127.0.0.1:1234").is_ok());

    assert_eq!(server.listen("127.0.0.1:1234").unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert!(server.socket().is_ok());

    assert!(server.shutdown().is_ok());
    assert_eq!(server.socket().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(server.shutdown().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_server_flow_without_connections() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(server.listen("127.0.0.1:1234").is_ok());

    assert_eq!(server.accept_receive(), Err(TryRecvError::Empty));

    assert!(server.flush(false).is_ok());

    assert!(server.shutdown().is_ok());

}

#[test]
fn test_server_flush_without_delay() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        server.accept_receive().ok();
        server.flush(false).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 0, 16);

}

#[test]
fn test_server_flush_auto_delay() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        server.accept_receive().ok();
        server.flush(true).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 167, 33);

}

#[test]
fn test_server_connection() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    // Accept a incoming connection
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(151521030))
    ]);

    // Accept another incoming connection
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:2000", vec![
            1, 2, 3, 4,
            4, 0, 0, 1,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(67108865))
    ]);

    // Switch the first connection to another peer address
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.2:1003", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        // There should be no connection event here since the connection id
        // got remapped to the new peer address
    ]);

}

#[test]
fn test_server_send() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    // Accept a incoming connection
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(151521030))
    ]);

    // Send via server
    server.send(&ConnectionID(151521030), MessageKind::Instant, b"Foo".to_vec()).ok();

    // Send via connection handle
    server.connection(&ConnectionID(151521030)).unwrap().send(MessageKind::Instant, b"Bar".to_vec());

    // Stats should not be updated before flush is called
    assert_eq!(server.bytes_sent(), 0);
    assert_eq!(server.bytes_received(), 0);

    // No messages should be send before flush is called
    server.socket().unwrap().assert_sent_none();

    // Both messages should be send after the flush call
    server.flush(false).ok();
    server.socket().unwrap().assert_sent(vec![("255.1.1.1:1000", [
        1, 2, 3, 4,
        9, 8, 7, 6,
        0,
        0,
        0, 0, 0, 0,
        0, 0, 0, 3, 70, 111, 111,
        0, 0, 0, 3, 66, 97, 114

    ].to_vec())]);

    // Stats should be updated after flush call
    assert_eq!(server.bytes_sent(), 28);
    assert_eq!(server.bytes_received(), 14);

    // Switch connection to new address
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.2:1001", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    server.accept_receive().ok();

    // Send to new address
    server.send(&ConnectionID(151521030), MessageKind::Instant, b"Baz".to_vec()).ok();

    // Message should be send to the new address of the connection
    server.flush(false).ok();
    server.socket().unwrap().assert_sent(vec![
        ("255.1.1.2:1001", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122

        ].to_vec())
    ]);

    assert_eq!(server.bytes_sent(), 49);
    assert_eq!(server.bytes_received(), 28);

}

#[test]
fn test_server_connection_loss() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        connection_drop_threshold: 100,
        .. Config::default()
    });
    server.listen("127.0.0.1:1234").ok();

    // Accept a incoming connection
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(151521030))
    ]);

    // Let the connection attempt time out
    thread::sleep(Duration::from_millis(200));

    let events = server_events(&mut server);
    assert_eq!(events, vec![ServerEvent::ConnectionLost(ConnectionID(151521030))]);

    server.flush(false).ok();

    // We expect no additional packets to be send once the connection was lost
    server.socket().unwrap().assert_sent_none();

}

// TODO test flow with client via two way mock socket
    // TODO test packet destination check
    // TODO test connections() and connection(id)
    // TODO test stats
    // TODO test shutdown and stats

// TODO test flow with multiple clients via two way mock socket :)
    // TODO test connections() and connection(id)
    // TODO test stats
    // TODO test shutdown and stats

// TODO test address re-mapping of client connections
    // TODO test connections() and connection(id)

// TODO test tick delay compensation in its own test file (just extract and re-use the old test code)

fn server_events(server: &mut Server<MockSocket, BinaryRateLimiter, NoopPacketModifier>) -> Vec<ServerEvent> {
    server.flush(false).ok();
    let mut events = Vec::new();
    while let Ok(event) = server.accept_receive() {
        events.push(event);
    }
    events
}

