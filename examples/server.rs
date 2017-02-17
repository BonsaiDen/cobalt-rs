// Crates ---------------------------------------------------------------------
extern crate cobalt;


// Dependencies ---------------------------------------------------------------
use cobalt::{
    BinaryRateLimiter, Config, NoopPacketModifier, MessageKind, UdpSocket,
    Server, ServerEvent
};

fn main() {

    // Create a new server that communicates over a udp socket
    let mut server = Server::<
        UdpSocket,
        BinaryRateLimiter,
        NoopPacketModifier

    >::new(Config::default());

    // Make the server listen on port `1234` on all interfaces.
    println!("[Server] Listening...");
    server.listen("0.0.0.0:1234").expect("Failed to bind to socket.");

    'main: loop {

        // Accept incoming connections and fetch their events
        while let Ok(event) = server.accept_receive() {
            // Handle events (e.g. Connection, Messages, etc.)
            match event {
                ServerEvent::Connection(id) => {
                    let conn = server.connection(&id).unwrap();
                    println!(
                        "[Server] Client {} ({}, {}ms rtt) connected.",
                        id.0,
                        conn.peer_addr(),
                        conn.rtt()
                    );

                },
                ServerEvent::Message(id, message) => {
                    let conn = server.connection(&id).unwrap();
                    println!(
                        "[Server] Message from client {} ({}, {}ms rtt): {:?}",
                        id.0,
                        conn.peer_addr(),
                        conn.rtt(),
                        message
                    );

                },
                ServerEvent::ConnectionClosed(id, _) | ServerEvent::ConnectionLost(id, _) => {
                    let conn = server.connection(&id).unwrap();
                    println!(
                        "[Server] Client {} ({}, {}ms rtt) disconnected.",
                        id.0,
                        conn.peer_addr(),
                        conn.rtt()
                    );
                    break 'main;
                },
                _ => {}
            }
        }

        // Send a message to all connected clients
        for (_, conn) in server.connections() {
            conn.send(MessageKind::Instant, b"Hello from Server".to_vec());
        }

        // Send all outgoing messages.
        //
        // Also auto delay the current thread to achieve the configured tick rate.
        server.send(true).is_ok();

    }

    println!("[Server] Shutting down...");

    // Shutdown the server (freeing its socket and closing all its connections)
    server.shutdown().ok();

}

