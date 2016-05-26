// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::net;
use std::thread;
use std::io::Error;
use std::time::Duration;
use std::net::ToSocketAddrs;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use super::super::{
    Connection, ConnectionID, Handler, MessageKind, Socket, Server, Client
};


// Owner Mocks ----------------------------------------------------------------
pub struct MockOwner;
pub struct MockOwnerHandler;
impl Handler<MockOwner> for MockOwnerHandler {}


// Socket Mock ----------------------------------------------------------------
pub struct MockSocket {
    send_packets: Vec<(&'static str, Vec<u8>)>,
    receiver: Receiver<(net::SocketAddr, Vec<u8>)>,
    send_count: usize
}

impl MockSocket {

    pub fn new(messages: Vec<(&'static str, Vec<u8>)>) -> MockSocket {

        let (sender, receiver) = channel::<(net::SocketAddr, Vec<u8>)>();
        for (addr, data) in messages.into_iter() {
            let src: net::SocketAddr = addr.parse().ok().unwrap();
            sender.send((src, data.clone())).ok();
        }

        MockSocket {
            send_count: 0,
            send_packets: Vec::new(),
            receiver: receiver
        }

    }

    pub fn expect(&mut self, send_packets: Vec<(&'static str, Vec<u8>)>) {
        self.send_count = send_packets.len();
        self.send_packets = send_packets;
    }

}

impl Socket for MockSocket {

    fn try_recv(&mut self) -> Result<(net::SocketAddr, Vec<u8>), TryRecvError> {
        self.receiver.try_recv()
    }

    fn send_to(
        &mut self, data: &[u8], addr: net::SocketAddr)

    -> Result<usize, Error> {

        // Don't run out of expected packets
        if self.send_packets.is_empty() {
            panic!(format!("Expected at most {} packet(s) to be send over socket.", self.send_count));
        }

        let addr_to: net::SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();

        // Search for the next packet with the matching address
        let mut index: i32 = -1;
        for (i, p) in self.send_packets.iter().enumerate() {

            // Verify receiver address
            let to: net::SocketAddr = p.0.parse().ok().unwrap();
            if to == addr_to {
                index = i as i32;
                break;
            }

        }

        if index == -1 {
            panic!(format!("Expected no more packet(s) to be send over socket to address {}", addr_to));
        }

        let expected = self.send_packets.remove(index as usize);

        // Verify packet data we ignore the connection ID here since it is
        // random and cannot be accessed by the mocks
        assert_eq!(&data[0..4], &expected.1[0..4]);
        assert_eq!(&data[8..], &expected.1[8..]);

        Ok(0)

    }

    fn local_addr(&self) -> Result<net::SocketAddr, Error> {
        let ip = net::Ipv4Addr::new(0, 0, 0, 0);
        Ok(net::SocketAddr::V4(net::SocketAddrV4::new(ip, 12345)))
    }

}


// Client Mocks ---------------------------------------------------------------
pub struct MockClientHandler {
    pub last_tick_time: u32,
    pub tick_count: u32,
    pub accumulated: i32
}

impl Handler<Client> for MockClientHandler {

    fn connect(&mut self, _: &mut Client) {
        self.last_tick_time = precise_time_ms();
    }

    fn tick_connection(&mut self, client: &mut Client, _: &mut Connection) {

        // Accumulate time so we can check that the artificial delay
        // was correct for by the servers tick loop
        if self.tick_count > 1 {
            self.accumulated += (precise_time_ms() - self.last_tick_time) as i32;
        }

        self.last_tick_time = precise_time_ms();
        self.tick_count += 1;

        if self.tick_count == 5 {
            client.close().unwrap();
        }

        // Fake some load inside of the tick handler
        let before = precise_time_ms();
        thread::sleep(Duration::from_millis(75));

        // Compensate for slow timers
        let spend = (precise_time_ms() - before) as i32;
        self.accumulated -= spend - 75;

    }

}

pub struct MockSyncClientHandler {
    pub connect_count: u32,
    pub tick_count: u32,
    pub close_count: u32
}

impl Handler<Client> for MockSyncClientHandler {

    fn connect(&mut self, _: &mut Client) {
        self.connect_count += 1;
    }

    fn tick_connection(&mut self, _: &mut Client, _: &mut Connection) {
        self.tick_count += 1;
    }

    fn close(&mut self, _: &mut Client) {
        self.close_count += 1;
    }

}

pub struct MockClientStatsHandler {
    pub tick_count: u32,
}

impl Handler<Client> for MockClientStatsHandler {

    fn connect(&mut self, _: &mut Client) {
    }

    fn tick_connection(
        &mut self, client: &mut Client,
        conn: &mut Connection
    ) {

        conn.send(MessageKind::Instant, b"Hello World".to_vec());
        self.tick_count += 1;

        if self.tick_count == 20 {
            client.close().unwrap();
        }

    }

}


// Server Mocks ----------------------------------------------------------------------
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
                conn.send(MessageKind::Instant, [self.send_count].to_vec());
                self.send_count += 1;
            }

            for msg in conn.received() {
                self.received.push(msg);
            }

        }

        if !self.received.is_empty() && self.send_count == 3 {
            server.shutdown().unwrap();
        }

    }

}


// Helpers --------------------------------------------------------------------
pub fn precise_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}

