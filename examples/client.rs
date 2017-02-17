// Crates ---------------------------------------------------------------------
extern crate cobalt;


// Dependencies ---------------------------------------------------------------
use cobalt::{
    BinaryRateLimiter, Config, NoopPacketModifier, MessageKind, UdpSocket,
    Client, ClientEvent
};

fn main() {

    // Create a new client that communicates over a udp socket
    let mut client = Client::<
        UdpSocket,
        BinaryRateLimiter,
        NoopPacketModifier

    >::new(Config::default());

    // Make the client connect to port `1234` on `localhost`
    println!("[Client] Connecting...");
    client.connect("127.0.0.1:1234").expect("Failed to bind to socket.");

    'main: loop {

        // Accept incoming connections and fetch their events
        while let Ok(event) = client.receive() {
            // Handle events (e.g. Connection, Messages, etc.)
            match event {
                ClientEvent::Connection => {
                    let conn = client.connection().unwrap();
                    println!(
                        "[Client] Connection established ({}, {}ms rtt).",
                        conn.peer_addr(),
                        conn.rtt()
                    );

                },
                ClientEvent::Message(message) => {
                    let conn = client.connection().unwrap();
                    println!(
                        "[Client] Message from server ({}, {}ms rtt): {:?}",
                        conn.peer_addr(),
                        conn.rtt(),
                        message
                    );

                },
                ClientEvent::ConnectionClosed(_) | ClientEvent::ConnectionLost(_) => {
                    let conn = client.connection().unwrap();
                    println!(
                        "[Client] ({}, {}ms rtt) disconnected.",
                        conn.peer_addr(),
                        conn.rtt()
                    );
                    break 'main;
                },
                _ => {}
            }
        }

        // Send a message to all connected clients
        if let Ok(conn) = client.connection() {
            conn.send(MessageKind::Instant, b"Hello from Client".to_vec());
        }

        // Send all outgoing messages.
        //
        // Also auto delay the current thread to achieve the configured tick rate.
        client.send(true).is_ok();

    }

    println!("[Client] Disconnecting...");

    // Shutdown the server (freeing its socket and closing all its connections)
    client.disconnect().ok();

}


