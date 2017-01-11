// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// STD Dependencies -----------------------------------------------------------
use std::cmp;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::collections::VecDeque;
use std::sync::mpsc::TryRecvError;


// Internal Dependencies ------------------------------------------------------
use super::super::{
    BinaryRateLimiter, NoopPacketModifier,
    Config, Connection, ConnectionEvent,
    Socket, RateLimiter, PacketModifier
};


// Mock Packet Data Abstraction -----------------------------------------------
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MockPacket(SocketAddr, Vec<u8>);

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
#[derive(Debug)]
pub struct MockSocket {
    local_addr: SocketAddr,
    sent_index: usize,
    pub incoming: VecDeque<MockPacket>,
    pub outgoing: Vec<MockPacket>
}

impl Socket for MockSocket {

    fn new<T: ToSocketAddrs>(addr: T, _: usize) -> Result<MockSocket, Error> {
        Ok(MockSocket {
            local_addr: to_socket_addr(addr),
            sent_index: 0,
            incoming: VecDeque::new(),
            outgoing: Vec::new()
        })
    }

    fn try_recv(&mut self) -> Result<(SocketAddr, Vec<u8>), TryRecvError> {
        if let Some(packet) = self.incoming.pop_front() {
            Ok((packet.0, packet.1))

        } else {
            Err(TryRecvError::Empty)
        }
    }

    fn send_to(
        &mut self,
        data: &[u8],
        addr: SocketAddr

    ) -> Result<usize, Error> {
        self.outgoing.push(MockPacket(addr, data.to_vec()));
        Ok(data.len())
    }

    fn local_addr(&self) -> Result<SocketAddr, Error> {
        Ok(self.local_addr)
    }

}

impl MockSocket {

    pub fn sent(&mut self) -> Vec<MockPacket> {

        let packets: Vec<MockPacket> = self.outgoing.iter().skip(self.sent_index).map(|packet| {
            packet.clone()

        }).collect();

        self.sent_index += packets.len();
        packets

    }

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


// Helpers --------------------------------------------------------------------
fn check_server_messages(conn: &mut Connection<BinaryRateLimiter, NoopPacketModifier>) {

    let mut messages = Vec::new();
    for m in conn.events() {
        if let ConnectionEvent::Message(m) = m {
            messages.push(m);
        }
    }

    assert_eq!(messages, [
        [72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100].to_vec()

    ].to_vec())

}

fn to_socket_addr<T: ToSocketAddrs>(address: T) -> SocketAddr {
    address.to_socket_addrs().unwrap().nth(0).unwrap()
}

pub fn create_connection(config: Option<Config>) -> Connection<BinaryRateLimiter, NoopPacketModifier> {
    let config = config.unwrap_or_else(||Config::default());
    let local_address: SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let peer_address: SocketAddr = "255.1.1.2:5678".parse().unwrap();
    let limiter = BinaryRateLimiter::new(config);
    let modifier = NoopPacketModifier::new(config);
    Connection::new(config, local_address, peer_address, limiter, modifier)

}

pub fn create_socket(config: Option<Config>) -> (
    Connection<BinaryRateLimiter, NoopPacketModifier>, MockSocket
) {
    let conn = create_connection(config);
    let socket = MockSocket::new(conn.local_addr(), 0).unwrap();
    (conn, socket)
}

