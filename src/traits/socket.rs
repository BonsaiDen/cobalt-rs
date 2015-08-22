use std::io::Error;
use std::net;
use std::sync::mpsc::Receiver;

/// Asynchronous `Receiver` for UDP packets.
pub type SocketReader = Receiver<(net::SocketAddr, Vec<u8>)>;

/// Trait for implementation a non-blocking UDP socket.
pub trait Socket {

    /// Method returning a channel receiver for incoming packets.
    fn reader(&mut self) -> Option<SocketReader>;

    /// Method for sending `data` to the specified remote address.
    fn send<T: net::ToSocketAddrs>(
        &mut self, addr: T, data: &[u8])
    -> Result<usize, Error>;

    /// Method returning the address of the actual, underlying socket.
    fn local_addr(&self) -> Result<net::SocketAddr, Error>;

    /// Method for shutting down the socket.
    fn shutdown(&mut self);

}

