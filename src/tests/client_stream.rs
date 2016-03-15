// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::net;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::io::ErrorKind;
use super::super::{
    Client, ClientStream, ClientEvent, Config, Connection, ConnectionID,
    Handler, MessageKind, Stats, Server
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
    let server_address = address.clone();
    let server_thread = thread::spawn(move|| {
        let config = Config::default();
        let mut server_handler = MockServerHandler::new();
        let mut server = Server::new(config);
        server.bind(&mut server_handler, server_address.unwrap()).unwrap();
        assert_eq!(server_handler.received, [b"Hello World".to_vec()].to_vec());
    });

    // Connect
    let mut stream = ClientStream::new(Config {
        send_rate: 5,
        .. Default::default()
    });

    stream.connect(server_address.unwrap()).expect("ClientStream address already in use!");

    // Wait and receive messages from server
    let mut received = Vec::new();
    let mut send = false;
    'wait: loop {

        while let Ok(event) = stream.receive() {

            if received.len() == 0 || received[received.len() - 1] != event{
                if event == ClientEvent::ConnectionLost {
                    break 'wait;
                } else {
                    received.push(event);
                }
            }

        }

        if !send  {
            stream.send(MessageKind::Instant, b"Hello World".to_vec()).unwrap();
            send = true;
        }

        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(50));

    }

    server_thread.join().unwrap();

    assert_eq!(received, vec![
        ClientEvent::Connect,
        ClientEvent::Tick,
        ClientEvent::Connection,
        ClientEvent::Message([1].to_vec()),
        ClientEvent::Tick,
        ClientEvent::Message([2].to_vec()),
        ClientEvent::Tick

    ]);

    stream.close().unwrap();

}


pub struct MockServerHandler {
    send_count: u8,
    pub received: Vec<Vec<u8>>
}

impl MockServerHandler {
    pub fn new() -> MockServerHandler {
        MockServerHandler {
            send_count: 0,
            received: Vec::new()
        }
    }
}

impl Handler<Server> for MockServerHandler {

    fn tick_connections(
        &mut self, server: &mut Server,
        connections: &mut HashMap<ConnectionID, Connection>
    ) {

        // Ensure hashmap and connection object have the same id
        for (_, conn) in connections.iter_mut() {

            if self.send_count < 3 {
                conn.send(MessageKind::Instant, [self.send_count].to_vec())
            }

            for msg in conn.received() {
                self.received.push(msg);
            }

        }

        if self.send_count < 3 {
            self.send_count += 1;
        }

        if self.received.len() > 0 && self.send_count == 3 {
            server.shutdown().unwrap();
        }

    }

}

