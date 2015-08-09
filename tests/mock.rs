extern crate cobalt;

use std::net;
use std::io::Error;
use std::collections::HashMap;

use cobalt::client::Client;
use cobalt::server::Server;
use cobalt::shared::{Connection, ConnectionID};
use cobalt::shared::traits::{Handler, Socket, SocketReader};
use std::sync::mpsc::{channel, Receiver, Sender};


// Owner Mock -----------------------------------------------------------------
pub struct MockOwner;
pub struct MockOwnerHandler;
impl Handler<MockOwner> for MockOwnerHandler {}


// Socket Mock ----------------------------------------------------------------
pub struct MockSocket {
    send_packets: Vec<Vec<u8>>,
    send_index: usize,
    sender: Sender<(net::SocketAddr, Vec<u8>)>,
    receiver: Option<SocketReader>
}

impl MockSocket {

    pub fn new(send_packets: Vec<Vec<u8>>) -> MockSocket {

        let (sender, receiver) = channel::<(net::SocketAddr, Vec<u8>)>();

        MockSocket {
            send_packets: send_packets,
            send_index: 0,
            sender: sender,
            receiver: Some(receiver)
        }

    }

    pub fn expect(&mut self, send_packets: Vec<Vec<u8>>) {
        self.send_index = 0;
        self.send_packets = send_packets;
    }

}

impl Socket for MockSocket {

    fn reader(&mut self) -> Option<SocketReader> {
        self.receiver.take()
    }

    fn send<T: net::ToSocketAddrs>(
        &mut self, _: T, data: &[u8])
    -> Result<usize, Error> {

        // Don't run out of expected packets
        assert!(self.send_index < self.send_packets.len());

        // Verify packet data
        assert_eq!(data, &self.send_packets[self.send_index][..]);

        self.send_index += 1;
        Ok(0)

    }

}


// Client Mock ----------------------------------------------------------------
pub struct MockClientHandler {
    pub connect_calls: u32,
    pub tick_connection_calls: u32,
    pub close_calls: u32,

    pub connection_calls: u32,
    pub connection_failed_calls: u32,
    pub connection_congested_calls: u32,
    pub connection_packet_lost_calls: u32,
    pub connection_lost_calls: u32
}

impl MockClientHandler {
    pub fn new() -> MockClientHandler {
        MockClientHandler {
            connect_calls: 0,
            tick_connection_calls: 0,
            close_calls: 0,

            connection_calls: 0,
            connection_failed_calls: 0,
            connection_congested_calls: 0,
            connection_packet_lost_calls: 0,
            connection_lost_calls: 0
        }
    }
}

impl Handler<Client> for MockClientHandler {

    fn connect(&mut self, _: &mut Client) {
        self.connect_calls += 1;
    }

    fn tick_connection(&mut self, _: &mut Client, _: &mut Connection) {
        self.tick_connection_calls += 1;
    }

    fn close(&mut self, _: &mut Client) {
        self.close_calls += 1;
    }

    fn connection(&mut self, _: &mut Client, _: &mut Connection) {
        self.connection_calls += 1;
    }

    fn connection_failed(&mut self, client: &mut Client, _: &mut Connection) {
        self.connection_failed_calls += 1;
        client.close();
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, _: &mut Connection, _: &[u8]
    ) {
        self.connection_packet_lost_calls += 1;
    }

    fn connection_congested(&mut self, _: &mut Client, _: &mut Connection) {
        self.connection_congested_calls += 1;
    }

    fn connection_lost(&mut self, client: &mut Client, _: &mut Connection) {
        self.connection_lost_calls += 1;
        client.close();
    }

}


// Server Mock ----------------------------------------------------------------
pub struct MockServerHandler {

    shutdown_ticks: u32,

    pub shutdown_calls: u32,
    pub tick_connections_calls: u32,
    pub bind_calls: u32,

    pub connection_calls: u32,
    pub connection_failed_calls: u32,
    pub connection_congested_calls: u32,
    pub connection_packet_lost_calls: u32,
    pub connection_lost_calls: u32
}

impl MockServerHandler {
    pub fn new(shutdown_ticks: u32) -> MockServerHandler {
        MockServerHandler {
            shutdown_ticks: shutdown_ticks,

            shutdown_calls: 0,
            tick_connections_calls: 0,
            bind_calls: 0,

            connection_calls: 0,
            connection_failed_calls: 0,
            connection_congested_calls: 0,
            connection_packet_lost_calls: 0,
            connection_lost_calls: 0
        }
    }
}

impl Handler<Server> for MockServerHandler {

    fn bind(&mut self, _: &mut Server) {
        self.bind_calls += 1;
    }

    fn tick_connections(
        &mut self, server: &mut Server,
        _: &mut HashMap<ConnectionID, Connection>
    ) {

        self.tick_connections_calls += 1;

        if self.tick_connections_calls > self.shutdown_ticks {
            server.shutdown();
        }

    }

    fn shutdown(&mut self, _: &mut Server) {
        self.shutdown_calls += 1;
    }

    fn connection(&mut self, _: &mut Server, _: &mut Connection) {
        self.connection_calls += 1;
    }

    fn connection_failed(&mut self, _: &mut Server, _: &mut Connection) {
        self.connection_failed_calls += 1;
    }

    fn connection_packet_lost(
        &mut self, _: &mut Server, _: &mut Connection, _: &[u8]
    ) {
        self.connection_packet_lost_calls += 1;
    }

    fn connection_congested(&mut self, _: &mut Server, _: &mut Connection) {
        self.connection_congested_calls += 1;
    }

    fn connection_lost(&mut self, _: &mut Server, _: &mut Connection) {
        self.connection_lost_calls += 1;
    }

}

