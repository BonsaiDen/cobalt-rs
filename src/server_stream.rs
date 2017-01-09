// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::{HashMap, VecDeque};
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::TryRecvError;
use shared::udp_socket::UdpSocket;
use super::{
    Config, Server, ServerState, ConnectionID, Connection, Handler, MessageKind, Stats
};


/// Enum of server network events.
#[derive(Debug, PartialEq)]
pub enum ServerEvent {

    /// Event emitted when the server is bound to its listening address.
    Bind,

    /// Event emitted each time the stream's underlying handler's `tick` method
    /// would have been invoked.
    Tick,

    /// Event emitted once the server has been shut down by the stream's
    /// `shutdown` method.
    Shutdown,

    /// Event emitted once a new client connection has been established.
    Connection(ConnectionID),

    /// Event emitted when a existing client connection is lost.
    ConnectionLost(ConnectionID),

    /// Event emitted when a client connection is closed programmatically.
    ConnectionClosed(ConnectionID, bool),

    /// Event emitted for each message received from a client connection.
    Message(ConnectionID, Vec<u8>),

    /// Event emitted each time a client's connection congestion state changes.
    ConnectionCongestionState(ConnectionID, bool),

    /// Event emitted each time a client connection packet is lost.
    PacketLost(ConnectionID, Vec<u8>)

}


/// Implementation of a stream based `Server` interface suitable for event
/// polling.
#[derive(Debug)]
pub struct ServerStream {
    handler: StreamHandler,
    config: Config,
    server: Server,
    state: Option<ServerState<UdpSocket>>,
    tick_rate: u32,
    should_receive: bool
}

impl ServerStream {

    /// Creates a new server stream with the given configuration.
    pub fn new(config: Config) -> ServerStream {
        ServerStream {
            handler: StreamHandler::new(),
            config: config,
            server: Server::new(config),
            state: None,
            tick_rate: 0,
            should_receive: false
        }
    }

    /// Consumes the passed in `Server` instance converting it into a
    /// `ServerStream`.
    pub fn from_client(server: Server) -> ServerStream {
        ServerStream {
            handler: StreamHandler::new(),
            config: server.config(),
            server: server,
            state: None,
            tick_rate: 0,
            should_receive: false
        }
    }

    // Getter

    /// Returns a mutable reference for the specified client connection.
    pub fn connection_mut(&mut self, id: &ConnectionID) -> Option<&mut Connection> {
        if let Some(state) = self.state.as_mut() {
            state.connection_mut(id)

        } else {
            None
        }
    }

    /// Returns a mutable reference for the servers client connections.
    pub fn connections_mut(&mut self) -> Option<&mut HashMap<ConnectionID, Connection>> {
        if let Some(state) = self.state.as_mut() {
            Some(state.connections_mut())

        } else {
            None
        }
    }

    // Setter

    /// Overrides the stream's current configuration with the one provided.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.server.set_config(config);
    }

    // Methods

    /// Binds the server to the specified address.
    pub fn bind<A: ToSocketAddrs>(&mut self, addr: A) -> Result<(), Error> {

        if self.state.is_none() {

            // Clear any previous stream events
            self.handler.clear();

            let mut state = self.server.bind_sync(&mut self.handler, addr).unwrap();
            state.set_config(self.config);

            self.tick_rate = self.config.send_rate;
            self.should_receive = true;
            self.state = Some(state);

            Ok(())

        } else {
            Err(Error::new(ErrorKind::AlreadyExists, ""))
        }

    }

    /// Accepts new incoming client connections from the stream's underlying
    /// server and receives and returns messages from them.
    pub fn accept_receive(&mut self) -> Result<ServerEvent, TryRecvError> {

        if self.state.is_none() {
            Err(TryRecvError::Disconnected)

        } else {

            if self.should_receive {

                self.should_receive = false;

                let mut state = self.state.as_mut().unwrap();
                self.server.accept_receive_sync(
                    &mut self.handler, &mut state, 1000000000 / self.tick_rate
                );
                self.server.tick_sync(&mut self.handler, &mut state);

            }

            self.handler.try_recv().ok_or_else(|| TryRecvError::Empty)

        }

    }

    /// Queues a message of the specified `kind` along with its `payload` to
    /// be send to the specified client connection with the next `flush` call.
    pub fn send(&mut self, id: &ConnectionID, kind: MessageKind, payload: Vec<u8>) -> Result<(), Error> {
        if let Some(ref mut state) = self.state {
            state.send(id, kind, payload)

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Sends all queued messages to the stream's underlying client connections.
    pub fn flush(&mut self, delay: bool) -> Result<(), Error> {
        if let Some(mut state) = self.state.as_mut() {
            self.should_receive = true;
            Ok(self.server.send_sync(
                &mut self.handler,
                &mut state,
                1000000000 / self.tick_rate,
                delay
            ))

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

    /// Shuts down the stream's underlying server, resetting all connections.
    pub fn shutdown(&mut self) -> Result<(), Error> {
        if let Some(mut state) = self.state.take() {
            self.server.shutdown_sync(&mut self.handler, &mut state);
            self.state = None;
            Ok(())

        } else {
            Err(Error::new(ErrorKind::NotConnected, ""))
        }
    }

}

#[derive(Debug, Default)]
struct StreamHandler {
    events: VecDeque<ServerEvent>
}

impl StreamHandler {

    fn new() -> StreamHandler {
        StreamHandler {
            events: VecDeque::new()
        }
    }

    fn try_recv(&mut self) -> Option<ServerEvent> {
        self.events.pop_front()
    }

    fn clear(&mut self) {
        self.events.clear();
    }

}

impl Handler<Server> for StreamHandler {

    fn bind(&mut self, _: &mut Server) {
        self.events.push_back(ServerEvent::Bind);
    }

    fn tick_connections(
        &mut self, _: &mut Server, conns: &mut HashMap<ConnectionID, Connection>
    ) {

        for (id, conn) in conns.iter_mut() {
            for msg in conn.received() {
                self.events.push_back(ServerEvent::Message(*id, msg));
            }
        }

        self.events.push_back(ServerEvent::Tick);

    }

    fn shutdown(&mut self, _: &mut Server) {
        self.events.push_back(ServerEvent::Shutdown);
    }

    fn connection(&mut self, _: &mut Server, conn: &mut Connection) {
        self.events.push_back(ServerEvent::Connection(conn.id()));
    }

    fn connection_packet_lost(
        &mut self, _: &mut Server, conn: &mut Connection, data: &[u8]
    ) {
        self.events.push_back(ServerEvent::PacketLost(conn.id(), data.to_vec()));
    }

    fn connection_congestion_state(&mut self, _: &mut Server, conn: &mut Connection, state: bool) {
        self.events.push_back(ServerEvent::ConnectionCongestionState(conn.id(), state));
    }

    fn connection_lost(&mut self, _: &mut Server, conn: &mut Connection) {
        self.events.push_back(ServerEvent::ConnectionLost(conn.id()));
    }

    fn connection_closed(&mut self, _: &mut Server, conn: &mut Connection, by_remote: bool) {
        self.events.push_back(ServerEvent::ConnectionClosed(conn.id(), by_remote));
    }

}

