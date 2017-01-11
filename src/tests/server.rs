// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;


// STD Dependencies -----------------------------------------------------------
use std::io::ErrorKind;
use std::sync::mpsc::TryRecvError;


// Internal Dependencies ------------------------------------------------------
use super::mock::MockSocket;
use ::{
    BinaryRateLimiter, ConnectionID, Config, MessageKind, NoopPacketModifier, Server
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
    assert_eq!(server.connections().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(server.accept_receive(), Err(TryRecvError::Disconnected));
    assert_eq!(server.send(&conn_id, MessageKind::Instant, Vec::new()).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(server.flush(false).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(server.shutdown().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_server_bind_shutdown() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(server.bind("127.0.0.1:1234").is_ok());

    assert_eq!(server.bind("127.0.0.1:1234").unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert!(server.socket().is_ok());

    assert!(server.shutdown().is_ok());
    assert_eq!(server.socket().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(server.shutdown().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_server_flow_without_connections() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(server.bind("127.0.0.1:1234").is_ok());

    assert_eq!(server.accept_receive(), Err(TryRecvError::Empty));

    assert!(server.flush(false).is_ok());

    assert!(server.shutdown().is_ok());

}

#[test]
fn test_server_flush_without_delay() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.bind("127.0.0.1:1234").ok();

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
    server.bind("127.0.0.1:1234").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        server.accept_receive().ok();
        server.flush(true).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 167, 33);

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

// TODO test client connection lost

// TODO test address re-mapping of client connections
    // TODO test connections() and connection(id)

// TODO test tick delay compensation in its own test file (just extract and re-use the old test code)

