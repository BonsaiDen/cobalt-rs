extern crate cobalt;

use std::env;
use std::str;
use std::collections::HashMap;
use cobalt::{Client, Config, Connection, ConnectionID, MessageKind, Handler, Server};

struct ServerHandler;
impl Handler<Server> for ServerHandler {

    fn bind(&mut self, _: &mut Server) {
        println!("Server::bind");
    }

    fn tick_connections(
        &mut self, _: &mut Server,
        connections: &mut HashMap<ConnectionID, Connection>
    ) {
        for (_, conn) in connections.iter_mut() {
            conn.send(MessageKind::Reliable, b"Hello World".to_vec());
        }
    }

    fn shutdown(&mut self, _: &mut Server) {
        println!("Server::shutdown");
    }

    fn connection(&mut self, _: &mut Server, _: &mut Connection) {
        println!("Server::connection");
    }

    fn connection_failed(&mut self, _: &mut Server, _: &mut Connection) {
        println!("Server::connection_failed");
    }

    fn connection_packet_lost(
        &mut self, _: &mut Server, _: &mut Connection, p: &[u8]
    ) {
        println!("Server::connection_packet_loss {}", p.len());
    }

    fn connection_congestion_state(&mut self, _: &mut Server, _: &mut Connection, state: bool) {
        println!("Server::connection_congestion_state {}", state);
    }

    fn connection_lost(&mut self, _: &mut Server, _: &mut Connection) {
        println!("Server::connection_lost");
    }

}

struct ClientHandler;
impl Handler<Client> for ClientHandler {

    fn connect(&mut self, _: &mut Client) {
        println!("Client::connect");
    }

    fn tick_connection(&mut self, _: &mut Client, conn: &mut Connection) {
        for msg in conn.received() {
            println!("Received Message: {}", str::from_utf8(&msg).unwrap());
        }
    }

    fn close(&mut self, _: &mut Client) {
        println!("Client::close");
    }

    fn connection(&mut self, _: &mut Client, _: &mut Connection) {
        println!("Client::connection");
    }

    fn connection_failed(&mut self, client: &mut Client, _: &mut Connection) {
        println!("Client::connection_failed");
        client.close().unwrap();
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, _: &mut Connection, _: &[u8]
    ) {
        println!("Client::connection_packet_loss");
    }

    fn connection_congestion_state(&mut self, _: &mut Client, _: &mut Connection, state: bool) {
        println!("Client::connection_congestion_state {}", state);
    }

    fn connection_lost(&mut self, client: &mut Client, _: &mut Connection) {
        println!("Client::connection_lost");
        client.close().unwrap();
    }

}

fn main() {

    for arg in env::args().skip(1) {
        match &arg[..]  {
            "client" => {
                let config = Config::default();
                let mut handler = ClientHandler;
                let mut client = Client::new(config);
                client.connect(&mut handler, "127.0.0.1:7156").unwrap();
            },
            "server" => {
                let config = Config::default();
                let mut handler = ServerHandler;
                let mut server = Server::new(config);
                server.bind(&mut handler, "127.0.0.1:7156").unwrap();
            },
            _ => {

            }
        }
    }

}

