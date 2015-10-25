use std::net;
use std::fmt;
use std::thread;
use std::io::Error;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use super::super::traits::socket::Socket;

/// Non-blocking abstraction over a UDP socket.
///
/// The non-blocking behavior is implement by using a internal reader thread.
///
/// The implementation guarantees that the internal thread exits cleanly in
/// case of either the sockets shutdown or it getting dropped.
///
/// This will be subject to change when support for socket timeouts lands in
/// rust 1.4. We will most likely switch to a timeout based, synchronous read
/// loop by then.
pub struct UdpSocket {
    socket: net::UdpSocket,
    reader_thread: Option<thread::JoinHandle<()>>,
    udp_receiver: Receiver<(net::SocketAddr, Vec<u8>)>,
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
            udp_receiver: r_udp,
            close_sender: s_close
        })

    }

}

impl Socket for UdpSocket {

    /// Attempts to return a incoming packet on this socket without blocking.
    fn try_recv(&self) -> Result<(net::SocketAddr, Vec<u8>), TryRecvError> {
        self.udp_receiver.try_recv()
    }

    /// Send data on the socket to the given address. On success, returns the
    /// number of bytes written.
    fn send_to(
        &mut self, data: &[u8], addr: net::SocketAddr)

    -> Result<usize, Error> {
        self.socket.send_to(data, addr)
    }

    /// Returns the socket address of the underlying `net::UdpSocket`.
    fn local_addr(&self) -> Result<net::SocketAddr, Error> {
        self.socket.local_addr()
    }

    /// Shuts down the socket by stopping its internal reader thread.
    fn shutdown(&mut self) {

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

impl Drop for UdpSocket {
    fn drop(&mut self) {
        // Make sure to exit the internal thread cleanly
        self.shutdown();
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UdpSocket({:?})", self.socket)
    }
}

