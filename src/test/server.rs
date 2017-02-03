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
    BinaryRateLimiter, ConnectionID, Config, MessageKind, NoopPacketModifier,
    Server, ServerEvent
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
    assert_eq!(server.send(false).unwrap_err().kind(), ErrorKind::NotConnected);
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

    assert!(server.send(false).is_ok());

    assert!(server.shutdown().is_ok());

}

#[test]
fn test_server_flush_without_delay() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    let start = Instant::now();
    for _ in 0..5 {
        server.accept_receive().ok();
        server.send(false).ok();
    }
    assert_millis_since!(start, 0, 16);

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

    assert_eq!(server.connections().keys().collect::<Vec<&ConnectionID>>(), vec![&ConnectionID(151521030)]);

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

    {
        let mut keys = server.connections().keys().collect::<Vec<&ConnectionID>>();
        keys.sort();
        assert_eq!(keys, vec![
            &ConnectionID(67108865),
            &ConnectionID(151521030)
        ]);
    }

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

    // Shutdown and drop all connections
    server.shutdown().ok();

    assert_eq!(server_events(&mut server), vec![]);

    // All connections should have been removed
    assert!(server.connections().keys().collect::<Vec<&ConnectionID>>().is_empty());
    assert!(server.connection(&ConnectionID(151521030)).is_err());
    assert!(server.connection(&ConnectionID(67108865)).is_err());

}

#[test]
fn test_server_connection_address_remap() {

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

    // Test send to initial address
    server.send(false).ok();
    server.socket().unwrap().assert_sent(vec![
        ("255.1.1.1:1000", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0

        ].to_vec())
    ]);

    // Receive from same connection id but different address
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.4:2000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            0,
            0, 0, 0, 0
        ])
    ]);

    // Trigger receival and address re-map
    server.accept_receive().ok();

    // Check send to new address
    server.send(false).ok();
    server.socket().unwrap().assert_sent(vec![
        ("255.1.1.4:2000", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            1,
            0, 0, 0, 1

        ].to_vec())
    ]);

    // Re-map should not happen for packets with older sequence numbers
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.8:4000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    server.accept_receive().ok();
    server.send(false).ok();

    // Verify send to first re-mapped address
    server.socket().unwrap().assert_sent(vec![
        ("255.1.1.4:2000", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            2,
            1,
            0, 0, 0, 1

        ].to_vec())
    ]);

    assert_eq!(server_events(&mut server), vec![
        // There should be no connection event here since the connection id
        // got remapped to the new peer address
    ]);

}

#[test]
fn test_server_reset_events() {

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

    // Fetch events from the connections
    server.accept_receive().ok();
    server.send(false).ok();

    // Shutdown should clear events
    server.shutdown().ok();

    // Re-bind and make events accesible again
    server.listen("127.0.0.1:1234").ok();
    assert!(server_events(&mut server).is_empty());

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

    assert!(server.connection(&ConnectionID(1)).is_err());

    // Send via connection handle
    server.connection(&ConnectionID(151521030)).unwrap().send(MessageKind::Instant, b"Foo".to_vec());
    server.connection(&ConnectionID(151521030)).unwrap().send(MessageKind::Instant, b"Bar".to_vec());

    // Check connections map entries
    assert_eq!(server.connections().keys().collect::<Vec<&ConnectionID>>(), vec![&ConnectionID(151521030)]);

    // Stats should not be updated before send is called
    assert_eq!(server.bytes_sent(), 0);
    assert_eq!(server.bytes_received(), 0);

    // No messages should be send before send is called
    server.socket().unwrap().assert_sent_none();

    // Both messages should be send after the send call
    server.send(false).ok();
    server.socket().unwrap().assert_sent(vec![("255.1.1.1:1000", [
        1, 2, 3, 4,
        9, 8, 7, 6,
        0,
        0,
        0, 0, 0, 0,
        0, 0, 0, 3, 70, 111, 111,
        0, 0, 0, 3, 66, 97, 114

    ].to_vec())]);

    // Stats should be updated after send call
    assert_eq!(server.bytes_sent(), 28);
    assert_eq!(server.bytes_received(), 14);

    // Switch connection to new address
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.2:1001", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            0,
            0, 0, 0, 0
        ])
    ]);

    server.accept_receive().ok();

    // Send to new address
    server.connection(&ConnectionID(151521030)).unwrap().send(MessageKind::Instant, b"Baz".to_vec());

    // Check connections map entries
    assert_eq!(server.connections().keys().collect::<Vec<&ConnectionID>>(), vec![&ConnectionID(151521030)]);

    // Message should be send to the new address of the connection
    server.send(false).ok();
    server.socket().unwrap().assert_sent(vec![
        ("255.1.1.2:1001", [
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            1,
            0, 0, 0, 1,
            0, 0, 0, 3, 66, 97, 122

        ].to_vec())
    ]);

    assert_eq!(server.bytes_sent(), 49);
    assert_eq!(server.bytes_received(), 28);

    // Shutdown and reset stats
    server.shutdown().ok();
    assert_eq!(server.bytes_sent(), 0);
    assert_eq!(server.bytes_received(), 0);

}

#[test]
fn test_server_receive() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    // Accept incoming connections
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            0,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111
        ])
    ]);

    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            0,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(151521030)),
        ServerEvent::Message(ConnectionID(151521030), b"Baz".to_vec()),
        ServerEvent::Connection(ConnectionID(84214017)),
        ServerEvent::Message(ConnectionID(84214017), b"Foo".to_vec())
    ]);

    // Should ignore duplicates
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            0,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111
        ])
    ]);

    assert!(server_events(&mut server).is_empty());

    // Receive additional messages
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            1,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            1,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Message(ConnectionID(151521030), b"Foo".to_vec()),
        ServerEvent::Message(ConnectionID(84214017), b"Baz".to_vec())
    ]);

}

#[test]
fn test_server_connection_close() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    // Accept incoming connections
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            0,
            0,
            0, 0, 0, 0
        ])
    ]);

    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0,
            0,
            0, 0, 0, 0,
            0, 0, 0, 3, 66, 97, 122
        ]),
        ("255.1.1.2:2000", vec![
            1, 2, 3, 4,
            5, 5, 1, 1,
            0,
            0,
            0, 0, 0, 0,
            1, 0, 0, 3, 70, 111, 111
        ])
    ]);

    assert_eq!(server_events(&mut server), vec![
        ServerEvent::Connection(ConnectionID(151521030)),
        ServerEvent::Connection(ConnectionID(84214017)),
    ]);

    // Receive closure packet
    server.socket().unwrap().mock_receive(vec![
        ("255.1.1.1:1000", vec![
            1, 2, 3, 4,
            9, 8, 7, 6,
            0, 128, // Most distant sequence numbers
            85, 85, 85, 85 // ack bitfield with every second bit set
        ])
    ]);

    // Expect closure by remote
    assert_eq!(server_events(&mut server), vec![
        ServerEvent::ConnectionClosed(ConnectionID(151521030), true)
    ]);

    // Connection should still exist after next send() call
    server.send(false).ok();
    assert!(server.connection(&ConnectionID(151521030)).is_ok());

    // But should be gone after second to next send() call
    server.send(false).ok();
    assert!(server.connection(&ConnectionID(151521030)).is_err());

    // Close via connection handle
    server.connection(&ConnectionID(84214017)).unwrap().close();

    // Connection should still be there during closing threshold
    assert_eq!(server_events(&mut server), vec![]);

    // Expect connection to be dropped after closing threshold
    thread::sleep(Duration::from_millis(165));
    assert_eq!(server_events(&mut server), vec![
        ServerEvent::ConnectionClosed(ConnectionID(84214017), false)
    ]);

}

#[test]
fn test_server_connection_loss() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config {
        connection_drop_threshold: Duration::from_millis(100),
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

    assert!(server.connection(&ConnectionID(151521030)).is_ok());
    assert_eq!(server.connections().keys().collect::<Vec<&ConnectionID>>(), vec![&ConnectionID(151521030)]);

    // Let the connection attempt time out
    thread::sleep(Duration::from_millis(200));

    let events = server_events(&mut server);

    //  Connection should still be there when fetching the events
    assert_eq!(events, vec![ServerEvent::ConnectionLost(ConnectionID(151521030))]);
    assert!(server.connection(&ConnectionID(151521030)).is_ok());
    assert_eq!(server.connections().len(), 1);


    // But connection be gone after next send() call
    server.send(false).ok();
    assert!(server.connection(&ConnectionID(151521030)).is_err());
    assert_eq!(server.connections().len(), 0);

    // We expect no additional packets to be send once the connection was lost
    server.socket().unwrap().assert_sent_none();

}

#[test]
#[cfg(target_os = "linux")]
fn test_server_flush_auto_delay() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    let start = Instant::now();
    for _ in 0..5 {
        server.accept_receive().ok();
        server.send(true).ok();
    }
    assert_millis_since!(start, 167, 33);

}

#[test]
#[cfg(target_os = "linux")]
fn test_server_auto_delay_with_load() {

    let mut server = Server::<MockSocket, BinaryRateLimiter, NoopPacketModifier>::new(Config::default());
    server.listen("127.0.0.1:1234").ok();

    // Without load
    let start = Instant::now();
    for _ in 0..10 {
        server.accept_receive().ok();
        server.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

    // With load
    let start = Instant::now();
    for _ in 0..10 {
        server.accept_receive().ok();
        thread::sleep(Duration::from_millis(10));
        server.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

    // With more load
    let start = Instant::now();
    for _ in 0..10 {
        server.accept_receive().ok();
        thread::sleep(Duration::from_millis(20));
        server.send(true).ok();
    }

    assert_millis_since!(start, 330, 16);

}

// TODO test congestion state changes
// TODO test packet lost events


// Helpers --------------------------------------------------------------------
fn server_events(server: &mut Server<MockSocket, BinaryRateLimiter, NoopPacketModifier>) -> Vec<ServerEvent> {
    server.send(false).ok();
    let mut events = Vec::new();
    while let Ok(event) = server.accept_receive() {
        events.push(event);
    }
    events
}

