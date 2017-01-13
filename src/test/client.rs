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
    BinaryRateLimiter, Client, ClientEvent, Config, MessageKind,
    NoopPacketModifier
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

    assert!(client.connect("255.1.1.1:5678").is_ok());

    assert_eq!(client.connect("255.1.1.1:5678").unwrap_err().kind(), ErrorKind::AlreadyExists);
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

    assert!(client.connect("255.1.1.1:5678").is_ok());

    assert_eq!(client.receive(), Err(TryRecvError::Empty));

    assert!(client.flush(false).is_ok());

    assert!(client.close().is_ok());

}

#[test]
fn test_client_flush_without_delay() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("255.1.1.1:5678").ok();

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
    client.connect("255.1.1.1:5678").ok();

    let start = clock_ticks::precise_time_ms();
    for _ in 0..5 {
        client.receive().ok();
        client.flush(true).ok();
    }
    assert_epsilon!(clock_ticks::precise_time_ms() - start, 167, 33);

}

#[test]
fn test_client_connection_failure() {

    let mut client = client_init(Config {
        connection_init_threshold: 100,
        .. Config::default()
    });

    // Let the connection attempt time out
    thread::sleep(Duration::from_millis(200));

    let events = client_events(&mut client);
    assert_eq!(events, vec![ClientEvent::ConnectionFailed]);

    client.flush(false).ok();
    client.flush(false).ok();

    // We expect no additional packets to be send once the connection failed
    client.socket().unwrap().assert_sent_none();

}

#[test]
fn test_client_connection_success() {

    let mut client = client_init(Config {
        .. Config::default()
    });

    // Mock the receival of the first server packet which acknowledges the client
    let id = client.connection().unwrap().id().0;
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![ClientEvent::Connection]);

}

#[test]
fn test_client_connection_ignore_non_peer() {

    let mut client = client_init(Config {
        connection_init_threshold: 100,
        .. Config::default()
    });

    // We expect the client to ignore any packets from peers other than the
    // server it is connected to
    let id = client.connection().unwrap().id().0;
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5679", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![]);

}

#[test]
fn test_client_connection_loss_and_reconnect() {

    let mut client = client_init(Config {
        connection_drop_threshold: 100,
        .. Config::default()
    });

    // Mock the receival of the first server packet which acknowledges the client
    let id = client.connection().unwrap().id().0;
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![ClientEvent::Connection]);

    // Let the connection time out
    thread::sleep(Duration::from_millis(200));

    // Expect one last packet
    assert_eq!(client.socket().unwrap().sent_count(), 1);

    assert_eq!(client_events(&mut client), vec![ClientEvent::ConnectionLost]);

    client.flush(false).ok();
    client.flush(false).ok();

    // We expect no additional packets to be send once the connection failed
    client.socket().unwrap().assert_sent_none();

    // Reset the client connection
    client.reset().ok();

    // Mock the receival of the first server packet which acknowledges the client
    let id = client.connection().unwrap().id().0;
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![ClientEvent::Connection]);

    // Expect one last packet
    assert_eq!(client.socket().unwrap().sent_count(), 1);

}

// TODO test flow with server via two way mock socket
    // TODO test connection(id)
    // TODO test sending
    // TODO test receiving (and all events)
    //
    // TODO test stats
    // TODO test reset and stats
    // TODO test close and stats

// TODO test tick delay compensation in its own test file (just extract and re-use the old test code)

// TODO test clean programmtic closure

// Helpers --------------------------------------------------------------------
fn client_init(config: Config) -> Client<MockSocket, BinaryRateLimiter, NoopPacketModifier> {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(config);
    client.connect("255.1.1.1:5678").ok();

    // Verify initial connection packet
    let id = client.connection().unwrap().id().0;
    client.flush(false).ok();
    client.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0

        ].to_vec())
    ]);

    client

}

fn client_events(client: &mut Client<MockSocket, BinaryRateLimiter, NoopPacketModifier>) -> Vec<ClientEvent> {
    client.flush(false).ok();
    let mut events = Vec::new();
    while let Ok(event) = client.receive() {
        events.push(event);
    }
    events
}

