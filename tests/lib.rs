extern crate cobalt;

mod mock;

use std::net;
use std::thread;
use mock::*;
use cobalt::client::Client;
use cobalt::server::Server;
use cobalt::shared::{Connection, ConnectionState, Config};

#[test]
fn connection_create() {
    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let conn = Connection::new(Config::default(), address);
    assert_eq!(conn.open(), true);
    assert_eq!(conn.congested(), false);
    assert_eq!(conn.state(), ConnectionState::Connecting);
    assert_eq!(conn.rtt(), 0);
    assert_eq!(conn.packet_loss(), 0.0);
}

#[test]
fn connection_close() {
    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let mut conn = Connection::new(Config::default(), address);
    conn.close();
    assert_eq!(conn.open(), false);
    assert_eq!(conn.state(), ConnectionState::Closed);
}

#[test]
fn connection_reset() {
    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let mut conn = Connection::new(Config::default(), address);
    conn.close();
    conn.reset();
    assert_eq!(conn.open(), true);
    assert_eq!(conn.state(), ConnectionState::Connecting);
}

#[test]
fn connection_send_sequence_wrap_around() {

    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let mut conn = Connection::new(Config::default(), address);

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

        conn.send(&mut socket, &address, &mut owner, &mut handler);

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

    conn.send(&mut socket, &address, &mut owner, &mut handler);

}

#[test]
fn connection_send_and_receive() {

    let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let mut conn = Connection::new(Config::default(), address);
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
        0, 0, 0, 0,

        // "Hello World"
        72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

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
        27, // remove sequence number set by receive()

        // Ack bitfield
        0, 0, 3, 128 // 0000_0000 0000_0000 0000_0011 1000_0000

    ].to_vec());


    // Testing
    let mut socket = MockSocket::new(expected_packets);
    let mut owner = MockOwner;
    let mut handler = MockOwnerHandler;

    // Test Initial Packet
    conn.send(&mut socket, &address, &mut owner, &mut handler);

    // Test sending of written data
    conn.write(b"Hello World");
    conn.send(&mut socket, &address, &mut owner, &mut handler);

    // Write buffer should get cleared
    conn.send(&mut socket, &address, &mut owner, &mut handler);

    // Test receiving of a packet with acknowledgements for two older packets
    conn.receive([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive()
        17, // local sequence number
        2, // remote sequence number we confirm
        0, 0, 0, 3, // confirm the first two packets

        // "Hello World"
        72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

    ].to_vec(), &mut owner, &mut handler);

    {
        let received_data = conn.read();
        assert_eq!(received_data.len(), 11);
        assert_eq!(received_data, b"Hello World");
    }

    // Receive additional packet
    conn.receive([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive()
        18, // local sequence number
        3, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    conn.receive([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive()
        19, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    conn.receive([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive()
        27, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), &mut owner, &mut handler);

    // Test Receive Ack Bitfield
    conn.send(&mut socket, &address, &mut owner, &mut handler);

}

#[test]
fn client_connection_failure() {

    let config = Config::default();
    let mut handler = MockClientHandler::new();
    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:0").unwrap();

    assert_eq!(handler.connect_calls, 1);
    assert!(handler.tick_connection_calls > 0);
    assert_eq!(handler.close_calls, 1);

    assert_eq!(handler.connection_calls, 0);
    assert_eq!(handler.connection_failed_calls, 1);
    assert_eq!(handler.connection_congested_calls, 0);
    assert_eq!(handler.connection_packet_lost_calls, 0);
    assert_eq!(handler.connection_lost_calls, 0);

}

#[test]
fn server_bind_and_shutdown() {

    let config = Config::default();
    let mut handler = MockServerHandler::new(0);
    let mut server = Server::new(config);
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

    assert_eq!(handler.bind_calls, 1);
    assert!(handler.tick_connections_calls > 0);
    assert_eq!(handler.shutdown_calls, 1);

    assert_eq!(handler.connection_calls, 0);
    assert_eq!(handler.connection_failed_calls, 0);
    assert_eq!(handler.connection_congested_calls, 0);
    assert_eq!(handler.connection_packet_lost_calls, 0);
    assert_eq!(handler.connection_lost_calls, 0);

}

#[test]
fn server_client_connection() {

    // Get a free local socket and then drop it for quick re-use
    // this is note 100% safe but we cannot easily get the locally bound server
    // address after bind() has been called
    let address: Option<net::SocketAddr> = {
        Some(net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap())
    };

    let server_address = address.clone();
    thread::spawn(move|| {

        let config = Config::default();
        let mut server_handler = MockServerHandler::new(35);
        let mut server = Server::new(config);
        server.bind(&mut server_handler, server_address.unwrap()).unwrap();

        assert_eq!(server_handler.bind_calls, 1);
        assert!(server_handler.tick_connections_calls > 0);
        assert_eq!(server_handler.shutdown_calls, 1);

        assert_eq!(server_handler.connection_calls, 1);
        assert_eq!(server_handler.connection_failed_calls, 0);
        assert_eq!(server_handler.connection_congested_calls, 0);
        assert_eq!(server_handler.connection_packet_lost_calls, 0);
        assert_eq!(server_handler.connection_lost_calls, 0);

    });

    let config = Config::default();
    let mut client_handler = MockClientHandler::new();
    let mut client = Client::new(config);
    client.connect(&mut client_handler, address.unwrap()).unwrap();

    assert_eq!(client_handler.connect_calls, 1);
    assert!(client_handler.tick_connection_calls > 0);
    assert_eq!(client_handler.close_calls, 1);

    assert_eq!(client_handler.connection_calls, 1);
    assert_eq!(client_handler.connection_failed_calls, 0);
    assert_eq!(client_handler.connection_congested_calls, 0);
    // This is somewhat random and depends on how excatly the two threads
    // interact
    // assert_eq!(client_handler.connection_packet_lost_calls, 0);
    assert_eq!(client_handler.connection_lost_calls, 1);

}

