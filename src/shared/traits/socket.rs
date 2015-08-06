use std::io::Error;
use std::net;
use std::sync::mpsc::Receiver;

/// A channel receiver for UDP packets.
pub type SocketReader = Receiver<(net::SocketAddr, Vec<u8>)>;

/// Trait that defines a non-blocking abstraction over a UDP socket.
pub trait Socket {

    /// Tries to create a new UDP socket by binding to the specified address.
    fn new<T: net::ToSocketAddrs>(address: T,
                                  max_packet_size: usize)
    -> Result<Socket, Error>;

    /// Returns the socket address of the underlying UdpSocket.
    fn local_addr(&self) -> Result<net::SocketAddr, Error>;

    /// Returns the channel receiver for incoming UDP packets.
    ///
    /// The `SocketReader` will be moved out; thus, the method will return
    /// `None` on all subsequent calls.
    fn reader(&mut self) -> Option<SocketReader>;

    /// Sends `data` to the specified remote address.
    fn send<T: net::ToSocketAddrs>(&self,
                                   addr: T, data: &[u8])
    -> Result<usize, Error>;

    /// Shuts down the socket by stopping its internal reader thread.
    fn shutdown(&mut self);

}

