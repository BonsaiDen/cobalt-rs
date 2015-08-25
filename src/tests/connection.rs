use std::net;
use std::thread;
use std::io::{Error, ErrorKind};

use super::super::traits::socket::SocketReader;
use super::super::{
    BinaryRateLimiter,
    Connection,
    ConnectionState,
    Config,
    MessageKind,
    Handler,
    Socket
};

fn connection() -> Connection {
    let config = Config::default();
    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let limiter = BinaryRateLimiter::new(&config);
    Connection::new(config, address, limiter)
}

#[test]
fn test_create() {
    let conn = connection();
    assert_eq!(conn.open(), true);
    assert_eq!(conn.congested(), false);
    assert!(conn.state() == ConnectionState::Connecting);
    assert_eq!(conn.rtt(), 0);
    assert_eq!(conn.packet_loss(), 0.0);
}

#[test]
fn test_close() {
    let mut conn = connection();
    conn.close();
    assert_eq!(conn.open(), false);
    assert!(conn.state() == ConnectionState::Closed);
}

#[test]
fn test_reset() {
    let mut conn = connection();
    conn.close();
    conn.reset();
    assert_eq!(conn.open(), true);
    assert!(conn.state() == ConnectionState::Connecting);
}

#[test]
fn test_send_sequence_wrap_around() {

    let mut conn = connection();
    let address = conn.peer_addr();
    let mut socket = MockSocket::new(Vec::new());
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    for i in 0..256 {

        let expected = vec![[
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

        ].to_vec()];

        socket.expect(expected);

        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    }

    // Should now have wrapped around
    let expected = vec![[
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

    ].to_vec()];

    socket.expect(expected);

    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

}

#[test]
fn test_send_and_receive_packet() {

    let mut conn = connection();
    let address = conn.peer_addr();
    let mut expected_packets: Vec<Vec<u8>> = Vec::new();

    // Initial packet
    expected_packets.push([
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

    ].to_vec());

    // Write packet
    expected_packets.push([
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,
        1, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec());

    // Empty again
    expected_packets.push([
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        2, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec());


    // Ack Bitfield test
    expected_packets.push([
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        3, // local sequence number
        27, // remove sequence number set by receive_packet)

        // Ack bitfield
        0, 0, 3, 128 // 0000_0000 0000_0000 0000_0011 1000_0000

    ].to_vec());


    // Testing
    let mut socket = MockSocket::new(expected_packets);
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    // Test Initial Packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    // Test sending of written data
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    // Write buffer should get cleared
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    // Test receiving of a packet with acknowledgements for two older packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        17, // local sequence number
        2, // remote sequence number we confirm
        0, 0, 0, 3, // confirm the first two packets

    ].to_vec(), &mut owner, &mut handler);


    // Receive additional packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        18, // local sequence number
        3, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        19, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        27, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Test Receive Ack Bitfield
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

}

#[test]
fn test_send_and_receive_message() {

    let mut conn = connection();
    let address = conn.peer_addr();
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    // Expected packet data
    let mut socket = MockSocket::new(vec![
        [
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

        ].to_vec()
    ]);

    // Test Message Sending
    conn.send(MessageKind::Instant, b"Foo".to_vec());
    conn.send(MessageKind::Instant, b"Bar".to_vec());
    conn.send(MessageKind::Reliable, b"Test".to_vec());
    conn.send(MessageKind::Ordered, b"Hello".to_vec());
    conn.send(MessageKind::Ordered, b"World".to_vec());
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

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

    ].to_vec(), &mut owner, &mut handler);

    // Get received messages
    let mut messages = Vec::new();
    for msg in conn.received() {
        messages.push(msg);
    }

    assert_eq!(messages, vec![
        b"Foo".to_vec(),
        b"Bar".to_vec(),
        b"Test".to_vec(),
        b"Hello".to_vec(),
        b"World".to_vec()
    ]);

}

#[test]
fn test_receive_invalid_packets() {

    let mut conn = connection();
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    // Empty packet
    conn.receive_packet([].to_vec(), &mut owner, &mut handler);

    // Garbage packet
    conn.receive_packet([
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14

    ].to_vec(), &mut owner, &mut handler);

}

#[test]
fn test_rtt() {

    let mut conn = connection();
    let address = conn.peer_addr();
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    let mut socket = MockSocket::new(vec![
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0
        ].to_vec(),
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            0,
            0, 0, 0, 0
        ].to_vec(),
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            2,
            1,
            0, 0, 0, 1
        ].to_vec(),
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            3,
            2,
            0, 0, 0, 3
        ].to_vec(),
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            4,
            3,
            0, 0, 0, 7
        ].to_vec(),
        [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            5,
            4,
            0, 0, 0, 15
        ].to_vec()
    ]);

    assert_eq!(conn.rtt(), 0);

    // First packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    thread::sleep_ms(500);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Expect RTT value to have moved by 10% of the overall roundtrip time
    assert!(conn.rtt() >= 40);

    // Second packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        1,
        1, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Third packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        2,
        2, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Fourth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        3,
        3, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Fifth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        4,
        4, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Sixth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        5,
        5, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Expect RTT to have reduced by 10%
    assert!(conn.rtt() <= 40);

}

#[test]
fn test_packet_loss() {

    struct PacketLossHandler;
    impl Handler<MockOwner> for PacketLossHandler {

        fn connection_packet_lost(
            &mut self, _: &mut MockOwner, _: &mut Connection, packet: &[u8]
        ) {
            assert_eq!([
                0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,
                1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,
                2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100
            ].to_vec(), packet);
        }

    }

    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: 10,
        .. Config::default()
    };

    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let limiter = BinaryRateLimiter::new(&config);
    let mut conn = Connection::new(config, address, limiter);
    let mut owner = MockOwner;
    let mut handler = PacketLossHandler;

    let mut socket = MockSocket::new(vec![
        [
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

        ].to_vec(),
        [
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

        ].to_vec()
    ]);

    conn.send(MessageKind::Instant, b"Packet Instant".to_vec());
    conn.send(MessageKind::Reliable, b"Packet Reliable".to_vec());
    conn.send(MessageKind::Ordered, b"Packet Ordered".to_vec());
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    assert_eq!(conn.packet_loss(), 0.0);

    // Wait a bit so the packets will definitely get dropped
    thread::sleep_ms(20);

    // Now receive a packet and check for the lost packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 2, 0, 0, // Set ack seq to non-0 so we trigger the packet loss
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // RTT should be left untouched the lost packet
    assert_eq!(conn.rtt(), 0);

    // But packet loss should spike up
    assert_eq!(conn.packet_loss(), 100.0);

    // The messages from the lost packet should have been re-inserted into
    // the message_queue and should be send again with the next packet.
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    // Fully receive the next packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 1, 0, 0,
        0, 0

    ].to_vec(), &mut owner, &mut handler);

    // But packet loss should now go down
    assert_eq!(conn.packet_loss(), 50.0);

}

// Owner Mock -----------------------------------------------------------------
pub struct MockOwner;
pub struct MockOwnerHandler;
impl Handler<MockOwner> for MockOwnerHandler {}


// Socket Mock ----------------------------------------------------------------
pub struct MockSocket {
    send_packets: Vec<Vec<u8>>,
    send_index: usize
}

impl MockSocket {

    pub fn new(send_packets: Vec<Vec<u8>>) -> MockSocket {
        MockSocket {
            send_packets: send_packets,
            send_index: 0
        }
    }

    pub fn expect(&mut self, send_packets: Vec<Vec<u8>>) {
        self.send_index = 0;
        self.send_packets = send_packets;
    }

}

impl Socket for MockSocket {

    fn reader(&mut self) -> Option<SocketReader> {
        None
    }

    fn send<T: net::ToSocketAddrs>(
        &mut self, _: T, data: &[u8])
    -> Result<usize, Error> {

        // Don't run out of expected packets
        if self.send_index >= self.send_packets.len() {
            panic!(format!("Expected at most {} packet(s) to be send over socket.", self.send_packets.len()));
        }

        // Verify packet data
        assert_eq!(data, &self.send_packets[self.send_index][..]);

        self.send_index += 1;
        Ok(0)

    }

    fn local_addr(&self) -> Result<net::SocketAddr, Error> {
        Err(Error::new(ErrorKind::AddrNotAvailable, ""))
    }

    fn shutdown(&mut self) {

    }

}

