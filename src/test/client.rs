// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// STD Dependencies -----------------------------------------------------------
use std::thread;
use std::time::{Duration, Instant};
use std::io::ErrorKind;
use std::sync::mpsc::TryRecvError;


// Internal Dependencies ------------------------------------------------------
use super::MockSocket;
use ::{
    BinaryRateLimiter, Client, ClientEvent, Config, MessageKind,
    NoopPacketModifier
};


// Macros ---------------------------------------------------------------------
macro_rules! assert_millis_since {
    ($start:expr, $target:expr, $difference:expr) => {
        {
            let duration = $start.elapsed();
            let millis = (duration.subsec_nanos() / 1000000) as u64;

            let actual = (duration.as_secs() * 1000 + millis) as i64;
            let min = $target - $difference;
            let max = $target + $difference;
            if actual < min || actual > max {
                panic!(format!("Value {} not in range {} - {}", actual, min, max));
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
    assert_eq!(client.send(false).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.reset().unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.disconnect().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_connect_reset_close() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(client.connect("255.1.1.1:5678").is_ok());

    assert_eq!(client.connect("255.1.1.1:5678").unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert!(client.socket().is_ok());

    assert!(client.reset().is_ok());
    assert!(client.socket().is_ok());

    assert!(client.disconnect().is_ok());
    assert_eq!(client.socket().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(client.disconnect().unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.reset().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_without_connection() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());

    assert!(client.connect("255.1.1.1:5678").is_ok());

    assert_eq!(client.receive(), Err(TryRecvError::Empty));

    assert!(client.send(false).is_ok());

    assert!(client.disconnect().is_ok());

}

#[test]
fn test_client_flush_without_delay() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("255.1.1.1:5678").ok();

    let start = Instant::now();
    for _ in 0..5 {
        client.receive().ok();
        client.send(false).ok();
    }
    assert_millis_since!(start, 0, 16);

}

#[test]
fn test_client_connection_failure() {

    let mut client = client_init(Config {
        connection_init_threshold: Duration::from_millis(100),
        .. Config::default()
    });

    // Let the connection attempt time out
    thread::sleep(Duration::from_millis(200));

    let events = client_events(&mut client);
    assert_eq!(events, vec![ClientEvent::ConnectionFailed]);

    client.send(false).ok();
    client.send(false).ok();

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
    assert_eq!(client.bytes_sent(), 28);
    assert_eq!(client.bytes_received(), 0);

    // Send again to update states
    client.send(true).ok();

    assert_eq!(client.bytes_sent(), 42);
    assert_eq!(client.bytes_received(), 14);

}

#[test]
fn test_client_reset_events() {

    let mut client = client_init(Config {
        .. Config::default()
    });

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

    // Fetch events from the connection
    client.receive().ok();
    client.send(false).ok();

    // Reset events
    client.reset().ok();
    assert_eq!(client_events(&mut client), vec![]);

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

    // Fetch events from the connection
    client.receive().ok();
    client.send(false).ok();

    // Disconnect and clear events
    client.disconnect().ok();

    // Re-connect to make events accesible again
    client.connect("255.1.1.1:5678").ok();

    assert_eq!(client_events(&mut client), vec![]);

}

#[test]
fn test_client_connection_ignore_non_peer() {

    let mut client = client_init(Config {
        connection_init_threshold: Duration::from_millis(100),
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
        connection_drop_threshold: Duration::from_millis(100),
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
    assert_eq!(client.bytes_sent(), 28);
    assert_eq!(client.bytes_received(), 0);

    // Let the connection time out
    thread::sleep(Duration::from_millis(200));

    // Expect one last packet
    assert_eq!(client.socket().unwrap().sent_count(), 1);

    assert_eq!(client_events(&mut client), vec![ClientEvent::ConnectionLost]);

    client.send(false).ok();
    client.send(false).ok();

    // We expect no additional packets to be send once the connection failed
    client.socket().unwrap().assert_sent_none();

    // Reset the client connection
    client.reset().ok();

    // Stats should have been reset
    assert_eq!(client.bytes_sent(), 0);
    assert_eq!(client.bytes_received(), 0);

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
    assert_eq!(client.bytes_sent(), 14);
    assert_eq!(client.bytes_received(), 0);

}

#[test]
fn test_client_send() {

    let mut client = client_init(Config {
        connection_drop_threshold: Duration::from_millis(100),
        .. Config::default()
    });

    assert_eq!(client.bytes_sent(), 14);

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

    // States should not be updated before the next send() call
    assert_eq!(client.bytes_sent(), 28);
    assert_eq!(client.bytes_received(), 0);
    client.send(false).ok();

    assert_eq!(client.bytes_sent(), 42);
    assert_eq!(client.bytes_received(), 14);

    // Verify initial connection packets
    client.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            0,
            0, 0, 0, 0

        ].to_vec()),
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            2,
            0,
            0, 0, 0, 0

        ].to_vec())
    ]);

    // Send messages to server
    client.connection().unwrap().send(MessageKind::Instant, b"Foo".to_vec());
    client.connection().unwrap().send(MessageKind::Reliable, b"Bar".to_vec());

    // Packets should not be send before the next send() call
    client.socket().unwrap().assert_sent_none();

    client.send(false).ok();
    client.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            3,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 70, 111, 111,
            1, 0, 0, 3, 66, 97, 114

        ].to_vec())
    ]);

    assert_eq!(client.bytes_sent(), 70);

}

#[test]
fn test_client_receive() {

    let mut client = client_init(Config {
        connection_drop_threshold: Duration::from_millis(100),
        .. Config::default()
    });

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
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111,
            0, 0, 0, 3, 66, 97, 114
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![
        ClientEvent::Connection,
        ClientEvent::Message(b"Foo".to_vec()),
        ClientEvent::Message(b"Bar".to_vec())
    ]);

    // Stats should not be updated before next send() call
    assert_eq!(client.bytes_received(), 0);

    client.send(false).ok();
    assert_eq!(client.bytes_received(), 28);

    // Ignore duplicates
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111,
            0, 0, 0, 3, 66, 97, 114
        ])
    ]);

    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            1,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ])
    ]);

    assert_eq!(client_events(&mut client), vec![
        ClientEvent::Message(b"Baz".to_vec())
    ]);

}

#[test]
fn test_client_close_by_remote() {

    let mut client = client_init(Config::default());
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

    assert_eq!(client_events(&mut client), vec![
        ClientEvent::Connection
    ]);

    // Receive closure packet
    client.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:5678", vec![
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0, 128, // Most distant sequence numbers
            85, 85, 85, 85 // ack bitfield with every second bit set
        ])
    ]);

    // Expect closure by remote
    assert_eq!(client_events(&mut client), vec![
        ClientEvent::ConnectionClosed(true)
    ]);

}

#[test]
fn test_client_close_by_local() {

    let mut client = client_init(Config::default());
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

    assert_eq!(client_events(&mut client), vec![
        ClientEvent::Connection
    ]);

    client.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            1,
            0,
            0, 0, 0, 0

        ].to_vec())
    ]);

    // Close connection
    client.connection().unwrap().close();

    // Expect closure packet to be sent
    client.send(false).ok();
    client.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:5678", [
            1, 2, 3, 4,
            (id >> 24) as u8,
            (id >> 16) as u8,
            (id >> 8) as u8,
             id as u8,
            0,
            128,
            85, 85, 85, 85

        ].to_vec())
    ]);

    // Expect connection to be dropped after closing threshold
    thread::sleep(Duration::from_millis(165));
    assert_eq!(client_events(&mut client), vec![
        ClientEvent::ConnectionClosed(false)
    ]);

}

#[test]
#[cfg(target_os = "linux")]
fn test_client_flush_auto_delay() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("255.1.1.1:5678").ok();

    let start = Instant::now();
    for _ in 0..5 {
        client.receive().ok();
        client.send(true).ok();
    }
    assert_millis_since!(start, 167, 33);

}

#[test]
#[cfg(target_os = "linux")]
fn test_client_auto_delay_with_load() {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    client.connect("255.1.1.1:5678").ok();

    // Without load
    let start = Instant::now();
    for _ in 0..10 {
        client.receive().ok();
        client.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

    // With load
    let start = Instant::now();
    for _ in 0..10 {
        client.receive().ok();
        thread::sleep(Duration::from_millis(10));
        client.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

    // With more load
    let start = Instant::now();
    for _ in 0..10 {
        client.receive().ok();
        thread::sleep(Duration::from_millis(20));
        client.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

}

// TODO test congestion state changes
// TODO test packet lost events


// Helpers --------------------------------------------------------------------
fn client_init(config: Config) -> Client<MockSocket, BinaryRateLimiter, NoopPacketModifier> {

    let mut client = Client::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(config);
    client.connect("255.1.1.1:5678").ok();

    // Verify initial connection packet
    let id = client.connection().unwrap().id().0;
    client.send(false).ok();
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
    client.send(false).ok();
    let mut events = Vec::new();
    while let Ok(event) = client.receive() {
        events.push(event);
    }
    events
}

