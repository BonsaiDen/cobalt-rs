// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// STD Dependencies -----------------------------------------------------------
use std::f32;
use std::thread;
use std::time::Duration;
use std::net::SocketAddr;


// Internal Dependencies ------------------------------------------------------
use super::MockSocket;
use ::{
    Connection, ConnectionID, ConnectionState, ConnectionEvent, Socket,
    Config, MessageKind, PacketModifier, BinaryRateLimiter, NoopPacketModifier,
    RateLimiter
};

macro_rules! assert_f32_eq {
    ($a:expr, $b:expr) => {
        assert!(($a - $b).abs() < f32::EPSILON);
    }
}


// Tests ----------------------------------------------------------------------
#[test]
fn test_create() {

    let mut conn = create_connection(None);
    assert_eq!(conn.open(), true);
    assert_eq!(conn.congested(), false);
    assert!(conn.state() == ConnectionState::Connecting);
    assert_eq!(conn.rtt(), 0);
    assert_f32_eq!(conn.packet_loss(), 0.0);

    let local_address: SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let peer_address: SocketAddr = "255.1.1.2:5678".parse().unwrap();

    assert_eq!(conn.local_addr(), local_address);
    assert_eq!(conn.peer_addr(), peer_address);
    assert_eq!(conn.events().len(), 0);

}

#[test]
fn test_debug_fmt() {
    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();
    conn.send_packet(&mut socket, &address);
    assert_ne!(format!("{:?}", conn), "");
}


#[test]
fn test_set_config() {
    // TODO how to check that the config was actually modified?
    let mut conn = create_connection(None);
    conn.set_config(Config {
        send_rate: 10,
        .. Config::default()
    });
}

#[test]
fn test_id_from_packet() {

    let config = Config {
        protocol_header: [1, 3, 3, 7],
        .. Config::default()
    };

    // Extract ID from matching protocol header
    assert_eq!(Connection::<BinaryRateLimiter, NoopPacketModifier>::id_from_packet(
        &config,
        &[1, 3, 3, 7, 1, 2, 3, 4]

    ), Some(ConnectionID(16909060)));

    // Ignore ID from non-matching protocol header
    assert_eq!(Connection::<BinaryRateLimiter, NoopPacketModifier>::id_from_packet(
        &config,
        &[9, 0, 0, 0, 1, 2, 3, 4]

    ), None);

    // Ignore packet data with len < 8
    assert_eq!(Connection::<BinaryRateLimiter, NoopPacketModifier>::id_from_packet(
        &config,
        &[9, 0, 0, 0, 1, 2, 3]

    ), None);

    // Ignore packet data with len < 4
    assert_eq!(Connection::<BinaryRateLimiter, NoopPacketModifier>::id_from_packet(
        &config,
        &[9, 0, 0]

    ), None);

}

#[test]
fn test_close_local() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    // Initiate closure
    conn.close();
    assert_eq!(conn.open(), true);
    assert!(conn.state() == ConnectionState::Closing);

    // Receival of packets should have no effect on the connection state
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0 // bitfield

    ].to_vec());

    assert!(conn.state() == ConnectionState::Closing);

    // Connection should now be sending closing packets
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, 128, 85, 85, 85, 85  // closure packet data

    ].to_vec())]);

    // Connection should keep sending closure packets until closing threshold is exceeded
    thread::sleep(Duration::from_millis(90));

    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, 128, 85, 85, 85, 85

    ].to_vec())]);

    // Connection should close once the closing threshold is exceeded
    thread::sleep(Duration::from_millis(100));
    conn.send_packet(&mut socket, &address);
    socket.assert_sent_none();

    assert_eq!(conn.open(), false);
    assert!(conn.state() == ConnectionState::Closed);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![ConnectionEvent::Closed(false)]);

}

#[test]
fn test_close_remote() {

    let mut conn = create_connection(None);
    assert!(conn.state() == ConnectionState::Connecting);

    // Receive initial packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0 // bitfield

    ].to_vec());

    assert!(conn.state() == ConnectionState::Connected);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![ConnectionEvent::Connected]);

    // Receive closure packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, 128, 85, 85, 85, 85 // closure packet data

    ].to_vec());

    assert_eq!(conn.open(), false);
    assert!(conn.state() == ConnectionState::Closed);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![ConnectionEvent::Closed(true)]);

}

#[test]
fn test_connecting_failed() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    thread::sleep(Duration::from_millis(500));
    conn.send_packet(&mut socket, &address);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![ConnectionEvent::FailedToConnect]);

    // Ignore any further packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 128, 85, 85, 85, 85 // closure packet data

    ].to_vec());

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![]);

}

#[test]
fn test_reset() {
    let mut conn = create_connection(None);
    conn.close();
    assert!(conn.state() == ConnectionState::Closing);

    conn.reset();
    assert_eq!(conn.open(), true);
    assert!(conn.state() == ConnectionState::Connecting);
}

#[test]
fn test_send_sequence_wrap_around() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    for i in 0..256 {

        conn.send_packet(&mut socket, &address);

        socket.assert_sent(vec![("255.1.1.2:5678", [
            // protocol id
            1, 2, 3, 4,

            // connection id
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            i as u8, // local sequence number
            0, // remote sequence number
            0, 0, 0, 0  // ack bitfield

        ].to_vec())]);

    }

    // Should now wrap around
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, // local sequence number
        0, // remote sequence number
        0, 0, 0, 0  // ack bitfield

    ].to_vec())]);

}

#[test]
fn test_send_and_receive_packet() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    // Test Initial Packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, // local sequence number
        0, // remote sequence number
        0, 0, 0, 0  // ack bitfield

    ].to_vec())]);

    // Test sending of written data
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,
        1, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec())]);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![]);

    // Write buffer should get cleared
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        2, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec())]);

    // Test receiving of a packet with acknowledgements for two older packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        17, // local sequence number
        2, // remote sequence number we confirm
        0, 0, 0, 3, // confirm the first two packets

    ].to_vec());

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![ConnectionEvent::Connected]);

    // Receive additional packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        18, // local sequence number
        3, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec());

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        19, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec());

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        27, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec());

    // Test Receive Ack Bitfield
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        3, // local sequence number
        27, // remove sequence number set by receive_packet)

        // Ack bitfield
        0, 0, 3, 128 // 0000_0000 0000_0000 0000_0011 1000_0000

    ].to_vec())]);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![]);

}

#[test]
fn test_receive_packet() {

    let mut conn = create_connection(None);

    assert_eq!(conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec()), true);

    // Ignore message with same sequence number
    assert_eq!(conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec()), false);

    assert_eq!(conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        1, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec()), true);

    assert_eq!(conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        2, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec()), true);

    // Ignore packet with older sequence number
    assert_eq!(conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        1, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec()), false);

}

#[test]
fn test_receive_packet_ack_overflow() {

    let mut conn = create_connection(None);

    // Receive more packets than the ack queue can hold
    for i in 0..33 {
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            i,
            0,
            0, 0, 0, 0

        ].to_vec());
    }

    // Verify ack bit field
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, // local sequence number
        32, // remote sequence to ack

        // The remote sequence values is already confirmed via the value above
        // so the highest bit is not set in this case
        127, 255, 255, 255

    ].to_vec())]);

}

#[test]
fn test_send_and_receive_messages() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    // Test Message Sending
    conn.send(MessageKind::Instant, b"Foo".to_vec());
    conn.send(MessageKind::Instant, b"Bar".to_vec());
    conn.send(MessageKind::Reliable, b"Test".to_vec());
    conn.send(MessageKind::Ordered, b"Hello".to_vec());
    conn.send(MessageKind::Ordered, b"World".to_vec());

    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0,

            // Foo
            0, 0, 0, 3, 70, 111, 111,

            // Bar
            0, 0, 0, 3, 66, 97, 114,

            // Test
            1, 0, 0, 4, 84, 101, 115, 116,

            // Hello
            2, 0, 0, 5, 72, 101, 108, 108, 111,

            // World
            2, 1, 0, 5, 87, 111, 114, 108, 100

        ].to_vec())
    ]);

    // Test Message Receiving
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0,
        0, 0, 0, 0,

        // Foo
        0, 0, 0, 3, 70, 111, 111,

        // Bar
        0, 0, 0, 3, 66, 97, 114,

        // Test
        1, 0, 0, 4, 84, 101, 115, 116,

        // We actually test inverse receiving order here!

        // World
        2, 1, 0, 5, 87, 111, 114, 108, 100,

        // Hello
        2, 0, 0, 5, 72, 101, 108, 108, 111

    ].to_vec());

    // Get received messages
    let messages: Vec<ConnectionEvent> = conn.events().collect();

    assert_eq!(messages, vec![
        ConnectionEvent::Connected,
        ConnectionEvent::Message(b"Foo".to_vec()),
        ConnectionEvent::Message(b"Bar".to_vec()),
        ConnectionEvent::Message(b"Test".to_vec()),
        ConnectionEvent::Message(b"Hello".to_vec()),
        ConnectionEvent::Message(b"World".to_vec())
    ]);

    // Test Received dismissing
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        1,
        1,
        0, 0, 0, 0,

        // Foo
        0, 0, 0, 3, 70, 111, 111

    ].to_vec());

    // send_packet should dismiss any received messages which have not been fetched
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            1,
            0, 0, 0, 1

        ].to_vec())
    ]);

    let messages: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(messages.len(), 0);

}

#[test]
fn test_receive_invalid_packets() {

    let mut conn = create_connection(None);

    // Empty packet
    conn.receive_packet([].to_vec());

    // Garbage packet
    conn.receive_packet([
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14

    ].to_vec());

}

#[test]
fn test_rtt() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    assert_eq!(conn.rtt(), 0);

    // First packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);

    thread::sleep(Duration::from_millis(500));
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Expect RTT value to have moved by 10% of the overall roundtrip time
    assert!(conn.rtt() >= 40);

    // Second packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        1,
        1, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Third packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            2,
            1,
            0, 0, 0, 1
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        2,
        2, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Fourth packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            3,
            2,
            0, 0, 0, 3
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        3,
        3, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Fifth packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            4,
            3,
            0, 0, 0, 7
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        4,
        4, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Sixth packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            5,
            4,
            0, 0, 0, 15
        ].to_vec())
    ]);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        5,
        5, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Expect RTT to have reduced by 10%
    assert!(conn.rtt() <= 40);

}

#[test]
fn test_rtt_tick_correction() {

    let mut conn = create_connection(None);
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    assert_eq!(conn.rtt(), 0);

    // First packet
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);

    thread::sleep(Duration::from_millis(500));
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec());

    // Expect RTT value to have been reduced after normal tick
    assert!(conn.rtt() <= 55);

}

#[test]
fn test_packet_loss() {

    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: Duration::from_millis(10),
        .. Config::default()
    };

    let mut conn = create_connection(Some(config));
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    conn.send(MessageKind::Instant, b"Packet Instant".to_vec());
    conn.send(MessageKind::Reliable, b"Packet Reliable".to_vec());
    conn.send(MessageKind::Ordered, b"Packet Ordered".to_vec());

    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0,

            // Packet 1
            0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,

            // Packet 2
            1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

            // Packet 3
            2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

        ].to_vec())
    ]);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events.len(), 0);

    assert_f32_eq!(conn.packet_loss(), 0.0);

    // Wait a bit so the packets will definitely get dropped
    thread::sleep(Duration::from_millis(20));

    // Now receive a packet and check for the lost packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 2, 0, 0, // Set ack seq to non-0 so we trigger the packet loss
        0, 0

    ].to_vec());

    // RTT should be left untouched the lost packet
    assert_eq!(conn.rtt(), 0);

    // But packet loss should spike up
    assert_f32_eq!(conn.packet_loss(), 100.0);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![
        ConnectionEvent::Connected,
        ConnectionEvent::PacketLost(vec![
            0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,
            1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,
            2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100
        ])
    ]);

    // The messages from the lost packet should have been re-inserted into
    // the message_queue and should be send again with the next packet.
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            0,
            0, 0, 0, 0,

            // Packet 2
            1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

            // Packet 3
            2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

        ].to_vec())
    ]);

    // Fully receive the next packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 1, 0, 0,
        0, 0

    ].to_vec());

    // Packet loss should now go down
    assert_f32_eq!(conn.packet_loss(), 50.0);

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events.len(), 0);

}

#[test]
fn test_packet_modification() {

    #[derive(Debug, Copy, Clone)]
    struct TestPacketModifier;

    impl PacketModifier for TestPacketModifier {

        fn new(_: Config) -> TestPacketModifier {
            TestPacketModifier
        }

        fn outgoing(&mut self, data: &[u8]) -> Option<Vec<u8>> {

            assert_eq!([
                0, 0, 0, 3, 70, 111, 111, // Foo
                0, 0, 0, 3, 66, 97, 114 // Bar
            ].to_vec(), data);

            // Remove messages
            Some(Vec::new())

        }

        fn incoming(&mut self, data: &[u8]) -> Option<Vec<u8>> {

            assert_eq!([1, 2, 3, 4, 128, 96, 7].to_vec(), data);

            // Inject to messages
            Some(vec![
                0, 0, 0, 3, 70, 111, 111, // Foo
                0, 0, 0, 3, 66, 97, 114 // Bar
            ])

        }

    }


    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: Duration::from_millis(10),
        .. Config::default()
    };

    let mut conn = create_connection_with_modifier::<TestPacketModifier>(Some(config));
    let mut socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    let address = conn.peer_addr();

    // First we send a packet to test compression
    conn.send(MessageKind::Instant, b"Foo".to_vec());
    conn.send(MessageKind::Instant, b"Bar".to_vec());
    conn.send_packet(&mut socket, &address);
    socket.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0

        ].to_vec())
    ]);

    // Then receive a packet to test for decompression
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 0, 0, 0,
        0, 0,
        1, 2, 3, 4, 128, 96, 7

    ].to_vec());

    let events: Vec<ConnectionEvent> = conn.events().collect();
    assert_eq!(events, vec![
        ConnectionEvent::Connected,
        ConnectionEvent::Message(b"Foo".to_vec()),
        ConnectionEvent::Message(b"Bar".to_vec())
    ]);

}


// Helpers --------------------------------------------------------------------
fn create_connection_with_modifier<T: PacketModifier>(config: Option<Config>) -> Connection<BinaryRateLimiter, T> {
    let config = config.unwrap_or_else(Config::default);
    let local_address: SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let peer_address: SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let limiter = BinaryRateLimiter::new(config);
    let modifier = T::new(config);
    Connection::new(config, local_address, peer_address, limiter, modifier)
}

fn create_connection(config: Option<Config>) -> Connection<BinaryRateLimiter, NoopPacketModifier> {
    create_connection_with_modifier::<NoopPacketModifier>(config)
}

