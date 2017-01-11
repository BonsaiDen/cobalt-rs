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
    BinaryRateLimiter, Client, Config, MessageKind, NoopPacketModifier
};


// Macros ---------------------------------------------------------------------
macro_rules! assert_epsilon {
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
fn test_client_configuration() {

    let config = Config {
        send_rate: 30,
        .. Config::default()
    };

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(config);
    assert_eq!(client.config(), config);

    let config = Config {
        send_rate: 60,
        .. Config::default()
    };

    client.set_config(config);
    assert_eq!(client.config(), config);

}

#[test]
fn test_client_internals() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        send_rate: 30,
        .. Config::default()
    });

    assert_eq!(client.socket().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_disconnected() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        send_rate: 30,
        .. Config::default()
    });

    assert_eq!(client.bytes_sent(), 0);
    assert_eq!(client.bytes_received(), 0);

    assert_eq!(client.peer_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(client.local_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(client.connection().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(client.receive(), Err(TryRecvError::Disconnected));
    assert_eq!(client.send(MessageKind::Instant, Vec::new()).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.flush(false).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.reset().unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.close().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_connect_reset_close() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(client.connect("127.0.0.1:1234").is_ok());

    assert_eq!(client.connect("127.0.0.1:1234").unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert!(client.socket().is_ok());

    assert!(client.reset().is_ok());
    assert!(client.socket().is_ok());

    assert!(client.close().is_ok());
    assert_eq!(client.socket().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(client.close().unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.reset().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_without_connection() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(client.connect("127.0.0.1:1234").is_ok());

    assert_eq!(client.receive(), Err(TryRecvError::Empty));

    assert!(client.flush(false).is_ok());

    assert!(client.close().is_ok());

}

#[test]
fn test_client_flush_without_delay() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("127.0.0.1:1234").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        client.receive().ok();
        client.flush(false).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 0, 16);

}

#[test]
fn test_client_flush_auto_delay() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("127.0.0.1:1234").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        client.receive().ok();
        client.flush(true).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 167, 33);

}

// TODO test connection success

// TODO test connection failure sleep and expect failure

// TODO test flow with server via two way mock socket
    // TODO test connection(id)
    // TODO test sending
    // TODO test receiving (and all events)
    //
    // TODO test stats
    // TODO test reset and stats
    // TODO test close and stats

// TODO test connection loss

// TODO test reset and re-connect to mock server

// TODO test tick delay compensation in its own test file (just extract and re-use the old test code)

