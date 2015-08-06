use std::thread;
use std::io::Error;
use std::net;
use std::sync::mpsc::{channel, Sender};
use shared::traits::{Socket, SocketReader};

/// A Non-blocking abstraction over a UDP socket.
///
/// The non-blocking behavior is implement using a thread internally.
///
/// The implementation guarantees that the internal thread exits cleanly in
/// case of either the sockets shutdown or it getting dropped.
pub struct UdpSocket {
    socket: net::UdpSocket,
    reader_thread: Option<thread::JoinHandle<()>>,
    udp_receiver: Option<SocketReader>,
    close_sender: Sender<()>
}

impl UdpSocket {

    /// Tries to create a new UDP socket by binding to the specified address.
    pub fn new<T: net::ToSocketAddrs>(
        address: T, max_packet_size: usize
    ) -> Result<Self, Error> {

        // Create the send socket
        let sender = try!(net::UdpSocket::bind(address));

        // Clone the socket handle for use inside the reader thread
        let reader = try!(sender.try_clone());

        // Create communication channels
        let (s_udp, r_udp) = channel::<(net::SocketAddr, Vec<u8>)>();
        let (s_close, r_close) = channel::<()>();

        // Create Reader Thread
        let reader_thread = thread::spawn(move|| {

            // Allocate buffer for the maximum packet size
            let mut buffer = Vec::with_capacity(max_packet_size);
            for _ in 0..max_packet_size {
                buffer.push(0);
            }

            loop  {

                // Receive packets...
                if let Ok((len, src)) = reader.recv_from(&mut buffer) {

                    // ...until shutdown is received
                    if let Ok(_) = r_close.try_recv() {
                        break;

                    // Copy only the actual number of bytes and send them
                    // along with the source address
                    } else {
                        s_udp.send((
                            src,
                            buffer[..len].iter().cloned().collect()

                        )).unwrap()
                    }

                }

            }

        });

        // Return the combined structure
        Ok(UdpSocket {
            socket: sender,
            reader_thread: Some(reader_thread),
            udp_receiver: Some(r_udp),
            close_sender: s_close
        })

    }

    /// Returns the socket address of the underlying UdpSocket.
    pub fn local_addr(&self) -> Result<net::SocketAddr, Error> {
        self.socket.local_addr()
    }


    /// Shuts down the socket by stopping its internal reader thread.
    pub fn shutdown(&mut self) {

        // Only shutdown if we still got a reader thread
        if let Some(reader_thread) = self.reader_thread.take() {

            // Notify the reader thread to exit
            self.close_sender.send(()).unwrap();

            // Then send a empty packet to the reader socket
            // so its recv_from() unblocks
            let socket = net::UdpSocket::bind("0.0.0.0:0").unwrap();
            let empty_packet: [u8; 0] = [0; 0];
            socket.send_to(&empty_packet, self.local_addr().unwrap()).unwrap();

            // Finally wait for the reader thread to exit cleanly
            reader_thread.join().unwrap();

        }

    }

}

impl Socket for UdpSocket {

    /// Returns the channel receiver for incoming UDP packets.
    ///
    /// The `SocketReader` will be moved out; thus, the method will return
    /// `None` on all subsequent calls.
    fn reader(&mut self) -> Option<SocketReader> {
        self.udp_receiver.take()
    }

    /// Sends `data` to the specified remote address.
    fn send<T: net::ToSocketAddrs>(
        &self, addr: T, data: &[u8])
    -> Result<usize, Error> {

        self.socket.send_to(data, addr)
    }

}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        // Make sure to exit the internal thread cleanly
        self.shutdown();
    }
}

