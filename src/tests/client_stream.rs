// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::net;
use std::thread;
use std::time::Duration;
use std::io::ErrorKind;

use super::mock::MockServerHandler;
use super::super::{
    Client, ClientStream, ClientEvent, Config, MessageKind, Stats, Server
};

#[test]
fn test_client_stream() {

    let config = Config {
        send_rate: 5,
        .. Default::default()
    };

    let mut stream = ClientStream::new(config);

    // Check unconnected defaults and errors
    assert_eq!(stream.peer_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(stream.local_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(stream.rtt(), 0);
    assert_eq!(stream.packet_loss(), 0.0);
    assert_eq!(stream.stats(), Stats {
        bytes_sent: 0,
        bytes_received: 0
    });

    assert_eq!(stream.bytes_sent(), 0);
    assert_eq!(stream.bytes_received(), 0);

    // Configuration
    assert_eq!(stream.config(), config);
    let config = Config {
        send_rate: 10,
        .. Default::default()
    };
    stream.set_config(config);
    assert_eq!(stream.config(), config);

}

#[test]
fn test_client_stream_from_client() {

    let config = Config {
        send_rate: 5,
        .. Default::default()
    };

    let client = Client::new(config);
    let stream = ClientStream::from_client(client);
    assert_eq!(stream.config(), config);

}

#[test]
fn test_client_stream_connect() {

    // Get a free local socket and then drop it for quick re-use
    // this is not 100% safe but we cannot easily get the locally bound server
    // address after bind() has been called
    let address: Option<net::SocketAddr> = {
        Some(net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap())
    };

    // Setup Test Server
    let server_thread = thread::spawn(move|| {
        let config = Config::default();
        let mut server_handler = MockServerHandler::new();
        let mut server = Server::new(config);
        server.bind(&mut server_handler, address.unwrap()).unwrap();
        assert_eq!(server_handler.received, [b"Hello World".to_vec()].to_vec());
    });

    // Connect
    let mut stream = ClientStream::new(Config {
        send_rate: 5,
        connection_init_threshold: 200,
        connection_drop_threshold: 200,
        .. Default::default()
    });

    stream.connect(address.unwrap()).expect("ClientStream address already in use!");

    // Wait and receive messages from server
    let mut received = Vec::new();
    'wait: loop {

        while let Ok(event) = stream.receive() {

            if received.is_empty() || received[received.len() - 1] != event {
                if event == ClientEvent::ConnectionLost || event == ClientEvent::ConnectionFailed {
                    break 'wait;

                } else {
                    if event == ClientEvent::Connection  {
                        stream.send(MessageKind::Instant, b"Hello World".to_vec()).unwrap();
                        received.push(event);

                    } else if event != ClientEvent::Tick {
                        received.push(event);
                    }
                }
            }

        }

        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(20));

    }

    server_thread.join().unwrap();

    assert_eq!(received, vec![
        ClientEvent::Connect,
        ClientEvent::Connection,
        ClientEvent::Message([0].to_vec()),
        ClientEvent::Message([1].to_vec()),
        ClientEvent::Message([2].to_vec())
    ]);

    stream.close().unwrap();

}

#[test]
fn test_client_stream_reset() {

    let mut stream = ClientStream::new(Config {
        send_rate: 5,
        connection_init_threshold: 100,
        connection_drop_threshold: 100,
        .. Default::default()
    });

    stream.connect("127.0.0.1:9999").expect("ClientStream address already in use!");
    stream.reset().ok();

    // Should not reset the stream's peer_addr
    assert_eq!(
        stream.peer_addr().ok(),
        Some(net::SocketAddr::V4(
            net::SocketAddrV4::new(net::Ipv4Addr::new(127, 0, 0, 1), 9999)
        ))
    );

}

#[test]
fn test_client_stream_reconnect() {

    let mut stream = ClientStream::new(Config {
        send_rate: 5,
        connection_init_threshold: 100,
        connection_drop_threshold: 100,
        .. Default::default()
    });

    stream.connect("127.0.0.1:9999").expect("ClientStream address already in use!");

    // Wait for events
    let mut tick = 0;
    loop {
        stream.flush().unwrap();
        tick += 1;
        if tick > 10 {
            break;
        }
    }

    // Close stream
    stream.close().unwrap();

    // Reconnect
    stream.connect("127.0.0.1:9999").expect("ClientStream address already in use!");

    // Expect previous stream events to be cleared (Connect, Close)
    let mut events = Vec::new();
    while let Ok(event) = stream.receive() {
        events.push(event);
    }

    assert_eq!(events, vec![
        ClientEvent::Connect,
        ClientEvent::Tick,
    ]);

}

