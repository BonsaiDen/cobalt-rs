// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::net;
use std::io::Error;
use std::sync::mpsc::TryRecvError;

/// Trait for implementation of a non-blocking UDP socket.
pub trait Socket {

    /// Method that attempts to return a incoming packet on this socket without
    /// blocking.
    fn try_recv(&self) -> Result<(net::SocketAddr, Vec<u8>), TryRecvError>;

    /// Method sending data on the socket to the given address. On success,
    /// returns the number of bytes written.
    fn send_to(
        &mut self, data: &[u8], addr: net::SocketAddr)

    -> Result<usize, Error>;

    /// Method returning the address of the actual, underlying socket.
    fn local_addr(&self) -> Result<net::SocketAddr, Error>;

    /// Method for shutting down the socket.
    fn shutdown(&mut self);

}

