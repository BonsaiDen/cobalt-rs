use std::io::Error;
use std::net;
use std::sync::mpsc::Receiver;

/// A channel receiver for UDP packets.
pub type SocketReader = Receiver<(net::SocketAddr, Vec<u8>)>;

/// Trait that defines a non-blocking abstraction over a UDP socket.
pub trait Socket {

    /// Returns the channel receiver for incoming UDP packets.
    ///
    /// The `SocketReader` will be moved out; thus, the method will return
    /// `None` on all subsequent calls.
    fn reader(&mut self) -> Option<SocketReader>;

    /// Sends `data` to the specified remote address.
    fn send<T: net::ToSocketAddrs>(
        &mut self, addr: T, data: &[u8])
    -> Result<usize, Error>;

}

