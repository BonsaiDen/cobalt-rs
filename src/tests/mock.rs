// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::cmp;
use std::net;
use std::thread;
use std::io::Error;
use std::time::Duration;
use std::net::ToSocketAddrs;
use std::collections::HashMap;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver, TryRecvError};

use super::super::{
    BinaryRateLimiter, Config, Connection, ConnectionID,
    Handler, MessageKind, Socket,
    Server, Client
};


// Owner Mocks ----------------------------------------------------------------
pub struct MockOwner;
pub struct MockOwnerHandler;
impl Handler<MockOwner> for MockOwnerHandler {}


// Mock Packet Data Abstraction -----------------------------------------------
#[derive(Clone, Eq, PartialEq)]
pub struct MockPacket(net::SocketAddr, Vec<u8>);

impl Ord for MockPacket {

    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.port().cmp(&other.0.port())
    }
}

impl PartialOrd for MockPacket {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}


// Client / Server Socket Abstraction -----------------------------------------
pub struct MockSocket {
    addr: net::SocketAddr,
    incoming: Receiver<MockPacket>,
    incoming_sender: Option<Sender<MockPacket>>,
    outgoing: Sender<MockPacket>,
    sent_packets: Arc<Mutex<Vec<MockPacket>>>,
    received_packets: Arc<Mutex<Vec<MockPacket>>>
}

impl MockSocket {

    pub fn new<T: ToSocketAddrs>(
        address: T,
        incoming: Receiver<MockPacket>,
        outgoing: Sender<MockPacket>,
        incoming_sender: Option<Sender<MockPacket>>

    ) -> Self {
        MockSocket {
            addr: to_socket_addr(address),
            incoming: incoming,
            incoming_sender: incoming_sender,
            outgoing: outgoing,
            sent_packets: Arc::new(Mutex::new(Vec::new())),
            received_packets: Arc::new(Mutex::new(Vec::new()))
        }
    }

    pub fn from_address<T: ToSocketAddrs>(addr: T) -> MockSocket {
        let (incoming_sender, incoming) = channel::<MockPacket>();
        let (outgoing, _) = channel::<MockPacket>();
        MockSocket::new(addr, incoming, outgoing, Some(incoming_sender))
    }

    //pub fn from_address_pair<T: ToSocketAddrs>(client_addr: T, server_addr: T) -> (MockSocket, MockSocket) {
    //    let (server_outgoing, client_incoming) = channel::<MockPacket>();
    //    let (client_outgoing, server_incoming) = channel::<MockPacket>();
    //    (
    //        MockSocket::new(client_addr, client_incoming, client_outgoing, None),
    //        MockSocket::new(server_addr, server_incoming, server_outgoing, None)
    //    )
    //}

    pub fn handle(&self) -> MockSocketHandle {
        MockSocketHandle {
            sent_index: 0,
            sent_packets: self.sent_packets.clone(),
            //received_index: 0,
            //received_packets: self.received_packets.clone()
        }
    }

    pub fn receive<T: ToSocketAddrs>(&self, packets: Vec<(T, Vec<u8>)>)  {
        if let Some(ref incoming_sender) = self.incoming_sender {
            for (addr, data) in packets.into_iter() {
                incoming_sender.send(MockPacket(to_socket_addr(addr), data)).ok();
            }
        }
    }

}

impl Socket for MockSocket {

    fn try_recv(&mut self) -> Result<(net::SocketAddr, Vec<u8>), TryRecvError> {
        match self.incoming.try_recv() {
            Ok(packet) => {
                let mut received_packets = self.received_packets.lock().unwrap();
                received_packets.push(packet.clone());
                Ok((packet.0, packet.1))
            },
            Err(err) => Err(err)
        }
    }

    fn send_to(
        &mut self, data: &[u8], addr: net::SocketAddr)

    -> Result<usize, Error> {
        self.outgoing.send(MockPacket(addr, data.to_vec())).ok();
        let mut sent_packets = self.sent_packets.lock().unwrap();
        sent_packets.push(MockPacket(addr, data.to_vec()));
        Ok(data.len())
    }

    fn local_addr(&self) -> Result<net::SocketAddr, Error> {
        Ok(self.addr)
    }

}

pub struct MockSocketHandle {
    sent_index: usize,
    sent_packets: Arc<Mutex<Vec<MockPacket>>>,
    //received_index: usize,
    //received_packets: Arc<Mutex<Vec<MockPacket>>>
}

impl MockSocketHandle {

    pub fn sent(&mut self) -> Vec<MockPacket> {

        let sent_packets = self.sent_packets.lock().unwrap();
        let packets: Vec<MockPacket> = sent_packets.iter().skip(self.sent_index).map(|packet| {
            packet.clone()

        }).collect();

        self.sent_index += packets.len();
        packets

    }

    //pub fn received(&mut self) -> Vec<MockPacket> {

    //    let received_packets = self.received_packets.lock().unwrap();
    //    let packets: Vec<MockPacket> = received_packets.iter().skip(self.received_index).map(|packet| {
    //        packet.clone()

    //    }).collect();

    //    self.received_index += packets.len();
    //    packets

    //}

    pub fn assert_sent_none(&mut self) {
        let sent = self.sent();
        if !sent.is_empty() {
            panic!(format!("Expected no more packet(s) to be sent, but {} additional ones.", sent.len()));
        }
    }

    pub fn assert_sent<T: ToSocketAddrs>(&mut self, expected: Vec<(T, Vec<u8>)>) {
        self.assert_sent_sorted(expected, false);
    }

    pub fn assert_sent_sort_by_addr<T: ToSocketAddrs>(&mut self, expected: Vec<(T, Vec<u8>)>) {
        self.assert_sent_sorted(expected, true);
    }

    fn assert_sent_sorted<T: ToSocketAddrs>(&mut self, expected: Vec<(T, Vec<u8>)>, sort_by_addr: bool) {


        // In some cases we need need a reliable assert order so we sort the sent
        // packets by their address
        let sent = if sort_by_addr {
            let mut sent = self.sent();
            sent.sort();
            sent

        } else {
            self.sent()
        };

        // Totals
        let expected_total = expected.len();
        let sent_total = sent.len();
        let min_total = cmp::min(expected_total, sent_total);

        for (i, (expected, sent)) in expected.into_iter().zip(sent.into_iter()).enumerate() {

            // Compare addresses
            let sent_addr = to_socket_addr(sent.0);
            let expected_addr = to_socket_addr(expected.0);
            if sent_addr != expected_addr {
                panic!(format!(
                    "{}) Packet destination address ({:?}) does not match expected one: {:?}.",
                    i, sent_addr, expected_addr
                ));
            }

            // Verify packet data. We specifically ignore the connection ID here
            // since it is random and cannot be accessed by the mocks
            if sent.1[0..4] != expected.1[0..4] {
                panic!(format!(
                    "{}) Packet protocol header of sent packet ({:?}) does not match expected one: {:?}.",
                    i, sent.1[0..4].to_vec(), expected.1[0..4].to_vec()
                ));
            }

            if sent.1[8..] != expected.1[8..] {
                panic!(format!(
                    "{}) Packet body data of sent packet ({:?}) does not match expected one: {:?}.",
                    i, sent.1[8..].to_vec(), expected.1[8..].to_vec()
                ));
            }

        }

        if expected_total > min_total {
            panic!(format!("Expected at least {} packet(s) to be sent, but only got a total of {}.", expected_total, sent_total));

        } else if sent_total > min_total {
            panic!(format!("Expected no more than {} packet(s) to be sent, but got a total of {}.", expected_total, sent_total));
        }

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
fn to_socket_addr<T: ToSocketAddrs>(address: T) -> net::SocketAddr {
    address.to_socket_addrs().unwrap().nth(0).unwrap()
}

pub fn precise_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}

pub fn create_connection(config: Option<Config>) -> (Connection, MockOwner, MockOwnerHandler) {
    let config = config.unwrap_or_else(||Config::default());
    let local_address: net::SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let peer_address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let limiter = BinaryRateLimiter::new(&config);
    (
        Connection::new(config, local_address, peer_address, limiter),
        MockOwner,
        MockOwnerHandler
    )

}

pub fn create_socket(config: Option<Config>) -> (
    Connection, MockSocket, MockSocketHandle, MockOwner, MockOwnerHandler
) {
    let (conn, owner, handler) = create_connection(config);
    let socket = MockSocket::from_address(conn.local_addr());
    let socket_handle = socket.handle();
    (conn, socket, socket_handle, owner, handler)
}

