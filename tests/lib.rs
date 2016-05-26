extern crate cobalt;

use std::net;
use std::thread;
use std::time::Duration;
use cobalt::{Client, Config, Server};

mod mock;
use mock::{MockClientHandler, MockServerHandler};

#[test]
fn test_client_connection_failure() {

    let config = Config::default();
    let mut handler = MockClientHandler::new();
    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:12345").unwrap();

    assert_eq!(handler.connect_calls, 1);
    assert!(handler.tick_connection_calls > 0);
    assert_eq!(handler.close_calls, 1);

    assert_eq!(handler.connection_calls, 0);
    assert_eq!(handler.connection_failed_calls, 1);
    assert_eq!(handler.connection_congestion_state_calls, 0);
    assert_eq!(handler.connection_packet_lost_calls, 0);
    assert_eq!(handler.connection_lost_calls, 0);

}

#[test]
fn test_server_bind_and_shutdown() {

    let config = Config::default();
    let mut handler = MockServerHandler::new(1, false);
    let mut server = Server::new(config);
    server.bind(&mut handler, "127.0.0.1:0").unwrap();

    assert_eq!(handler.bind_calls, 1);
    assert!(handler.tick_connections_calls > 0);
    assert_eq!(handler.shutdown_calls, 1);

    assert_eq!(handler.connection_calls, 0);
    assert_eq!(handler.connection_failed_calls, 0);
    assert_eq!(handler.connection_congestion_state_calls, 0);
    assert_eq!(handler.connection_packet_lost_calls, 0);
    assert_eq!(handler.connection_lost_calls, 0);

}

#[test]
fn test_server_client_connection_lost() {

    // Get a free local socket and then drop it for quick re-use
    // this is not 100% safe but we cannot easily get the locally bound server
    // address after bind() has been called
    let address: Option<net::SocketAddr> = {
        Some(net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap())
    };

    // Setup Test Server
    let server_address = address.clone();
    let server_thread = thread::spawn(move|| {

        let config = Config::default();
        let mut server_handler = MockServerHandler::new(15, false);
        let mut server = Server::new(config);
        server.bind(&mut server_handler, server_address.unwrap()).unwrap();

        assert_eq!(server_handler.bind_calls, 1);
        assert!(server_handler.tick_connections_calls > 0);
        assert_eq!(server_handler.shutdown_calls, 1);

        assert_eq!(server_handler.connection_calls, 1);
        assert_eq!(server_handler.connection_failed_calls, 0);
        assert_eq!(server_handler.connection_congestion_state_calls, 0);
        assert_eq!(server_handler.connection_packet_lost_calls, 0);
        assert_eq!(server_handler.connection_lost_calls, 0);
        assert_eq!(server_handler.connection_closed_calls, 0);

    });

    let config = Config::default();
    let mut client_handler = MockClientHandler::new();
    let mut client = Client::new(config);
    client.connect(&mut client_handler, address.unwrap()).unwrap();

    server_thread.join().unwrap();
    assert_eq!(client_handler.connect_calls, 1);
    assert!(client_handler.tick_connection_calls > 0);
    assert_eq!(client_handler.close_calls, 1);

    assert_eq!(client_handler.connection_calls, 1);
    assert_eq!(client_handler.connection_failed_calls, 0);
    assert_eq!(client_handler.connection_congestion_state_calls, 0);
    // This is somewhat random and depends on how excatly the two threads
    // interact
    // assert_eq!(client_handler.connection_packet_lost_calls, 0);
    assert_eq!(client_handler.connection_lost_calls, 1);
    assert_eq!(client_handler.connection_closed_calls, 0);

}

#[test]
fn test_server_client_connection_close() {

    // Get a free local socket and then drop it for quick re-use
    // this is not 100% safe but we cannot easily get the locally bound server
    // address after bind() has been called
    let address: Option<net::SocketAddr> = {
        Some(net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap())
    };

    // Setup Test Server
    let server_address = address.clone();
    let server_thread = thread::spawn(move|| {

        let config = Config {
            connection_init_threshold: 1000,
            .. Default::default()
        };

        let mut server_handler = MockServerHandler::new(5, true);
        let mut server = Server::new(config);
        server.bind(&mut server_handler, server_address.unwrap()).unwrap();

        assert_eq!(server_handler.connection_lost_calls, 0);
        assert_eq!(server_handler.connection_calls, 1);
        assert_eq!(server_handler.connection_closed_calls, 1);
        assert_eq!(server_handler.closed_by_remote, false);

    });

    let config = Config {
        connection_init_threshold: 1000,
        .. Default::default()
    };

    // TODO cause might be connection marked as dropped by client and
    // an attempted reconnect?
    // client might not receive the closure frame in time?
    //
    // TODO another cause might be that the server thinks the connection is
    // already lost before having send the closure packets? this might explain
    // why the client reconnects afterwards

    let mut client_handler = MockClientHandler::new();
    let mut client = Client::new(config);
    thread::sleep(Duration::from_millis(50));
    client.connect(&mut client_handler, address.unwrap()).unwrap();

    server_thread.join().unwrap();
    assert_eq!(client_handler.connection_lost_calls, 0);
    assert_eq!(client_handler.connection_calls, 1);
    assert_eq!(client_handler.connection_closed_calls, 1);
    assert_eq!(client_handler.closed_by_remote, true);

}

