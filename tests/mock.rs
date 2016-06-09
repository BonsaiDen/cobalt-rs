extern crate cobalt;
use std::collections::HashMap;
use cobalt::{Client, Connection, ConnectionID, Handler, Server};


// Client Mock ----------------------------------------------------------------
pub struct MockClientHandler {
    pub connect_calls: u32,
    pub tick_connection_calls: u32,
    pub close_calls: u32,

    pub connection_calls: u32,
    pub connection_failed_calls: u32,
    pub connection_congestion_state_calls: u32,
    pub connection_packet_lost_calls: u32,
    pub connection_lost_calls: u32,
    pub connection_closed_calls: u32,
    pub closed_by_remote: bool
}

impl MockClientHandler {
    pub fn new() -> MockClientHandler {
        MockClientHandler {
            connect_calls: 0,
            tick_connection_calls: 0,
            close_calls: 0,

            connection_calls: 0,
            connection_failed_calls: 0,
            connection_congestion_state_calls: 0,
            connection_packet_lost_calls: 0,
            connection_lost_calls: 0,
            connection_closed_calls: 0,
            closed_by_remote: false
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
        client.close().unwrap();
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, _: &mut Connection, _: &[u8]
    ) {
        self.connection_packet_lost_calls += 1;
    }

    fn connection_congestion_state(&mut self, _: &mut Client, _: &mut Connection, _: bool) {
        self.connection_congestion_state_calls += 1;
    }

    fn connection_lost(&mut self, client: &mut Client, _: &mut Connection) {
        self.connection_lost_calls += 1;
        client.close().unwrap();
    }

    fn connection_closed(&mut self, client: &mut Client, _: &mut Connection, by_remote: bool) {
        self.connection_closed_calls += 1;
        self.closed_by_remote = by_remote;
        client.close().unwrap();
    }

}


// Server Mock ----------------------------------------------------------------
pub struct MockServerHandler {

    shutdown_ticks: u32,
    close_connection: bool,

    pub shutdown_calls: u32,
    pub tick_connections_calls: u32,
    pub bind_calls: u32,

    pub connection_calls: u32,
    pub connection_failed_calls: u32,
    pub connection_congestion_state_calls: u32,
    pub connection_packet_lost_calls: u32,
    pub connection_lost_calls: u32,
    pub connection_closed_calls: u32,
    pub closed_by_remote: bool
}

impl MockServerHandler {
    pub fn new(shutdown_ticks: u32, close_connection: bool) -> MockServerHandler {
        MockServerHandler {
            shutdown_ticks: shutdown_ticks,
            close_connection: close_connection,

            shutdown_calls: 0,
            tick_connections_calls: 0,
            bind_calls: 0,

            connection_calls: 0,
            connection_failed_calls: 0,
            connection_congestion_state_calls: 0,
            connection_packet_lost_calls: 0,
            connection_lost_calls: 0,
            connection_closed_calls: 0,
            closed_by_remote: false
        }
    }
}

impl Handler<Server> for MockServerHandler {

    fn bind(&mut self, _: &mut Server) {
        self.bind_calls += 1;
    }

    fn tick_connections(
        &mut self, server: &mut Server,
        connections: &mut HashMap<ConnectionID, Connection>
    ) {

        // Ensure hashmap and connection object have the same id
        for (id, conn) in connections.iter() {
            assert_eq!(*id, conn.id());
        }

        // only advance until initial connection close
        // when there is a connection
        if connections.len() == 0 && self.close_connection {
            self.tick_connections_calls = 0;
        }

        self.tick_connections_calls += 1;

        if self.tick_connections_calls == self.shutdown_ticks {

            // close the client connection the first time around
            if self.close_connection {
                for (_, conn) in connections.iter_mut() {
                    conn.close();
                }

            // shutdown the server
            } else {
                server.shutdown().unwrap();
            }
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

    fn connection_congestion_state(&mut self, _: &mut Server, _: &mut Connection, _: bool) {
        self.connection_congestion_state_calls += 1;
    }

    fn connection_lost(&mut self, _: &mut Server, _: &mut Connection) {
        self.connection_lost_calls += 1;
    }

    fn connection_closed(&mut self, _: &mut Server, _: &mut Connection, by_remote: bool) {
        self.connection_closed_calls += 1;
        self.closed_by_remote = by_remote;
        self.tick_connections_calls = 0;
        self.shutdown_ticks = 5;
        self.close_connection = false;
    }

}

