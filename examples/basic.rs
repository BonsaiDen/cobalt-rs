extern crate cobalt;

use std::env;
use std::collections::HashMap;
use cobalt::client::Client;
use cobalt::server::Server;
use cobalt::shared::{Config, Connection, ConnectionID, Handler};

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
            conn.write(b"Hello World");
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

    fn connection_congested(&mut self, _: &mut Server, _: &mut Connection) {
        println!("Server::connection_congested");
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
        let data = conn.read();
        println!("Received {} bytes of data", data.len());
    }

    fn close(&mut self, _: &mut Client) {
        println!("Client::close");
    }

    fn connection(&mut self, _: &mut Client, _: &mut Connection) {
        println!("Client::connection");
    }

    fn connection_failed(&mut self, client: &mut Client, _: &mut Connection) {
        println!("Client::connection_failed");
        client.close();
    }

    fn connection_packet_lost(
        &mut self, _: &mut Client, _: &mut Connection, _: &[u8]
    ) {
        println!("Client::connection_packet_loss");
    }

    fn connection_congested(&mut self, _: &mut Client, _: &mut Connection) {
        println!("Client::connection_congested");
    }

    fn connection_lost(&mut self, client: &mut Client, _: &mut Connection) {
        println!("Client::connection_lost");
        client.close();
    }

}

fn main() {

    for arg in env::args().skip(1) {
        match &arg[..]  {
            "client" => {
                let config = Config::default();
                let mut handler = ClientHandler;
                let mut client = Client::new(config);
                client.connect(&mut handler, "127.0.0.1:7156");
            },
            "server" => {
                let config = Config::default();
                let mut handler = ServerHandler;
                let mut server = Server::new(config);
                server.bind(&mut handler, "127.0.0.1:7156");
            },
            _ => {

            }
        }
    }

}

