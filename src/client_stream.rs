// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::VecDeque;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::TryRecvError;
use shared::udp_socket::UdpSocket;
use super::{
    Config, Client, ClientState, Connection, Handler, MessageKind, Stats
};


/// Enum of stream related network events.
#[derive(Debug, PartialEq)]
pub enum ClientEvent {

    /// Event emitted when a initial connection attempt to a server is made.
    Connect,

    /// Event emitted each time the stream's underlying handler's `tick` method
    /// would have been invoked.
    Tick,

    /// Event emitted once the connection to the server has been closed by the
    /// stream's `close` method.
    Close,

    /// Event emitted once a connection to the server has been established.
    Connection,

    /// Event emitted when a initial connection attempt to a server failed.
    ConnectionFailed,

    /// Event emitted when a existing connection to a server is lost.
    ConnectionLost,

    /// Event emitted for each message received from the server.
    Message(Vec<u8>),

    /// Event emitted each time the stream's congestion state changes.
    ConnectionCongestionState(bool),

    /// Event emitted each time a packet is lost.
    PacketLost(Vec<u8>)

}

/// Implementation of a stream based `Client` interface suitable for event
/// polling.
#[derive(Debug)]
pub struct ClientStream {
    handler: StreamHandler,
    config: Config,
    client: Client,
    state: Option<ClientState<UdpSocket>>,
    tick_rate: u32,
    should_receive: bool
}

impl ClientStream {

    /// Creates a new client stream with the given configuration.
    pub fn new(config: Config) -> ClientStream {
        ClientStream {
            handler: StreamHandler::new(),
            config: config,
            client: Client::new(config),
            state: None,
            tick_rate: 0,
            should_receive: false
        }
    }

    /// Creates an new client stream from an existing client instance.
    pub fn from_client(client: Client) -> ClientStream {
        ClientStream {
            handler: StreamHandler::new(),
            config: client.config(),
            client: client,
            state: None,
            tick_rate: 0,
            should_receive: false
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
        self.state.as_ref().map_or(0, |s| s.rtt())
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across this
    /// stream.
    pub fn packet_loss(&self) -> f32 {
        self.state.as_ref().map_or(0.0, |s| s.packet_loss())
    }

    /// Returns statistics (i.e. bandwidth usage) for the last second.
    pub fn stats(&self) -> Stats {
        self.state.as_ref().map_or(Default::default(), |s| s.stats())
    }

    /// Returns the number of bytes sent over the last second.
    pub fn bytes_sent(&self) -> u32 {
        self.state.as_ref().map_or(0, |s| s.stats().bytes_sent)
    }

    /// Returns the number of bytes received over the last second.
    pub fn bytes_received(&self) -> u32 {
        self.state.as_ref().map_or(0, |s| s.stats().bytes_received)
    }

    /// Returns a copy of the stream's current configuration.
    pub fn config(&self) -> Config {
        self.config
    }

    // Setter

    /// Overrides the stream's current configuration with the one provided.
    pub fn set_config(&mut self, config: Config) {

        self.config = config;

        if let Some(state) = self.state.as_mut() {
            state.set_config(config);
        }

    }

    // Methods

    /// Establishes a connection with the server at the specified address.
    pub fn connect<A: ToSocketAddrs>(&mut self, addr: A) -> Result<(), Error> {

        if self.state.is_none() {
            let mut state = self.client.connect_sync(&mut self.handler, addr).unwrap();
            state.set_config(self.config);
            self.tick_rate = self.config.send_rate;
            self.should_receive = true;
            self.state = Some(state);
            Ok(())

        } else {
            Err(Error::new(ErrorKind::AlreadyExists, ""))
        }

    }

    /// Receives the next incoming message from the stream's underlying
    /// connection.
    pub fn receive(&mut self) -> Result<ClientEvent, TryRecvError> {

        if self.state.is_none() {
            Err(TryRecvError::Disconnected)

        } else {

            if self.should_receive {

                self.should_receive = false;

                let mut state = self.state.as_mut().unwrap();
                self.client.receive_sync(
                    &mut self.handler, &mut state, 1000 / self.tick_rate
                );
                self.client.tick_sync(&mut self.handler, &mut state);

            }

            self.handler.try_recv().ok_or_else(|| TryRecvError::Empty)

        }

    }

    /// Queues a message of the specified `kind` along with its `payload` to
    /// be send with the next `flush` call.
    pub fn send(&mut self, kind: MessageKind, payload: Vec<u8>) -> Result<(), Error> {
        if let Some(ref mut state) = self.state {
            Ok(state.send(kind, payload))

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Sends all queued messages over the stream's underlying connection.
    pub fn flush(&mut self) -> Result<(), Error> {
        if let Some(mut state) = self.state.as_mut() {
            self.should_receive = true;
            Ok(self.client.send_sync(&mut self.handler, &mut state))

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Resets the stream's underlying connection to the server.
    pub fn reset(&mut self) -> Result<(), Error> {
        if let Some(mut state) = self.state.as_mut() {
            Ok(state.reset())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Closes the stream's underlying connection to the server.
    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut state) = self.state.take() {
            self.client.close_sync(&mut self.handler, &mut state).unwrap();
            self.state = None;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

}


#[derive(Debug, Default)]
struct StreamHandler {
    events: VecDeque<ClientEvent>
}

impl StreamHandler {

    pub fn new() -> StreamHandler {
        StreamHandler {
            events: VecDeque::new()
        }
    }

    pub fn try_recv(&mut self) -> Option<ClientEvent> {
        self.events.pop_front()
    }

}

impl Handler<Client> for StreamHandler {

    fn connect(&mut self, _: &mut Client) {
        self.events.push_back(ClientEvent::Connect);
    }

    fn tick_connection(
        &mut self, _: &mut Client, conn: &mut Connection
    ) {

        for msg in conn.received() {
            self.events.push_back(ClientEvent::Message(msg));
        }

        self.events.push_back(ClientEvent::Tick);

    }

    fn close(&mut self, _: &mut Client) {
        self.events.push_back(ClientEvent::Close);
    }

    fn connection(&mut self, _: &mut Client, _: &mut Connection) {
        self.events.push_back(ClientEvent::Connection);
    }

    fn connection_failed(&mut self, _: &mut Client, _: &mut Connection) {
        self.events.push_back(ClientEvent::ConnectionFailed);
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, _: &mut Connection, data: &[u8]
    ) {
        self.events.push_back(ClientEvent::PacketLost(data.to_vec()));
    }

    fn connection_congestion_state(&mut self, _: &mut Client, _: &mut Connection, state: bool) {
        self.events.push_back(ClientEvent::ConnectionCongestionState(state));
    }

    fn connection_lost(&mut self, _: &mut Client, _: &mut Connection) {
        self.events.push_back(ClientEvent::ConnectionLost);
    }

}

