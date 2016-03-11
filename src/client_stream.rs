// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::VecDeque;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use shared::udp_socket::UdpSocket;
use super::{
    Config, Client, ClientState, Connection, ConnectionID, Handler,
    MessageKind, Stats
};

/// Implementation of a stream based `Client` interface more suitable for
/// classical event polling architectures.
#[derive(Debug)]
pub struct ClientStream {
    handler: Option<StreamHandler>,
    client: Client,
    state: Option<ClientState<UdpSocket>>,
    tick_rate: u32
}

impl ClientStream {

    pub fn new(client: Client) -> ClientStream {
        ClientStream {
            handler: None,
            client: client,
            state: None,
            tick_rate: 0
        }
    }

    // Getter

    /// Returns the address of the server the stream is currently connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr, Error> {
        self.client.peer_addr()
    }

    /// Returns the local address that the stream is sending from.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.client.local_addr()
    }

    /// Returns the average roundtrip time for this stream's underlying
    /// connection.
    pub fn rtt(&self) -> u32 {
        self.state.rtt()
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across this
    /// stream.
    pub fn packet_loss(&self) -> f32 {
        self.state.packet_loss()
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&self) -> Stats {
        self.state.stats()
    }

    /// Returns the number of bytes sent over the last second.
    pub fn bytes_sent(&self) -> u32 {
        self.state.stats().bytes_sent
    }

    /// Returns the number of bytes received over the last second.
    pub fn bytes_received(&self) -> u32 {
        self.state.stats().bytes_received
    }

    /// Returns a copy of the stream's current configuration.
    pub fn config(&self) -> Config {
        self.client.config()
    }

    // Setter

    /// Overrides the streams's existing configuration with the one provided.
    ///
    /// Returns a error in case the stream is currently not connected.
    pub fn set_config(&mut self, config: Config) -> Result<(), Error> {
        if let Some(state) = self.state {
            state.set_config(config);
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    // Methods

    pub fn connect<A: ToSocketAddrs>(&mut self, addr: A) {
        self.tick_rate = self.config().tick_rate;
        self.state = None;
    }

    pub fn receive(&mut self) {
        self.client.receive_sync(
            &mut self.handler, &mut self.state, 1000 / self.tick_rate
        );
        self.client.tick_sync(&mut self.handler, &mut self.state);
    }

    pub fn recv_message(&mut self) -> Option<StreamEvent> {
        self.handler.try_recv()
    }

    pub fn send(&mut self) {
        self.client.send_sync(&mut self.handler, &mut self.state);
    }

    pub fn send_message(&mut self, kind: MessageKind, data: &Vec<u8>) {
        self.state.send(kind, data);
    }

    pub fn close(&mut self) {

    }

    pub fn reset(&mut self) -> Result<(), Error> {
        self.state.and_then(|s| {
            s.reset();
            Ok(())

        }).or_else(|| Err(Error::new(ErrorKind::NotConnected, ""))).unwrap()
    }

    pub fn destroy(&mut self) -> Result<(), Error> {
        if let Some(state) = self.state.take() {
            self.client.close_sync(&mut self.handler, &mut state)

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

}

// TODO must be public hm
#[derive(Debug)]
pub enum StreamEvent {
    Connect,
    Tick,
    Close,
    Message(ConnectionID, Vec<u8>),
    Connection(ConnectionID),
    ConnectionFailed(ConnectionID),
    ConnectionCongestionState(ConnectionID, bool),
    ConnectionLost(ConnectionID),
    PacketLost(ConnectionID, Vec<u8>)
}


struct StreamHandler {
    events: VecDeque<StreamEvent>
}

impl StreamHandler {

    pub fn new() -> StreamHandler {
        StreamHandler {
            events: VecDeque::new()
        }
    }

    pub fn try_recv(&mut self) -> Option<StreamEvent> {
        self.events.pop_front()
    }

}

impl Handler<Client> for StreamHandler {

    fn connect(&mut self, _: &mut Client) {
        self.events.push_back(StreamEvent::Connect);
    }

    fn tick_connection(
        &mut self, _: &mut Client, conn: &mut Connection
    ) {

        let id = conn.id();
        for msg in conn.received() {
            self.events.push_back(StreamEvent::Message(id, msg));
        }

        self.events.push_back(StreamEvent::Tick);

    }

    fn close(&mut self, _: &mut Client) {
        self.events.push_back(StreamEvent::Close);
    }

    fn connection(&mut self, _: &mut Client, conn: &mut Connection) {
        self.events.push_back(StreamEvent::Connection(conn.id()));
    }

    fn connection_failed(&mut self, _: &mut Client, conn: &mut Connection) {
        self.events.push_back(StreamEvent::ConnectionFailed(conn.id()));
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, conn: &mut Connection, data: &[u8]
    ) {
        self.events.push_back(StreamEvent::PacketLost(conn.id(), data.to_vec()));
    }

    fn connection_congestion_state(&mut self, _: &mut Client, conn: &mut Connection, state: bool) {
        self.events.push_back(StreamEvent::ConnectionCongestionState(conn.id(), state));
    }

    fn connection_lost(&mut self, _: &mut Client, conn: &mut Connection) {
        self.events.push_back(StreamEvent::ConnectionLost(conn.id()));
    }

}

