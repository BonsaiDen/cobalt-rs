// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// STD Dependencies -----------------------------------------------------------
use std::net;
use std::fmt;
use std::iter;
use std::io::Error;
use std::sync::mpsc::TryRecvError;


// Internal Dependencies ------------------------------------------------------
use ::Socket;

/// Non-blocking abstraction over a UDP socket.
pub struct UdpSocket {
    socket: net::UdpSocket,
    buffer: Vec<u8>
}

impl Socket for UdpSocket {

    /// Tries to create a new UDP socket by binding to the specified address.
    fn new<T: net::ToSocketAddrs>(
        address: T, max_packet_size: usize

    ) -> Result<Self, Error> {

        // Create the send socket
        let socket = try!(net::UdpSocket::bind(address));

        // Switch into non-blocking mode
        try!(socket.set_nonblocking(true));

        // Allocate receival buffer
        let buffer: Vec<u8> = iter::repeat(0).take(max_packet_size).collect();

        Ok(UdpSocket {
            socket: socket,
            buffer: buffer
        })

    }

    /// Attempts to return a incoming packet on this socket without blocking.
    fn try_recv(&mut self) -> Result<(net::SocketAddr, Vec<u8>), TryRecvError> {

        if let Ok((len, src)) = self.socket.recv_from(&mut self.buffer) {
            Ok((src, self.buffer[..len].to_vec()))

        } else {
            Err(TryRecvError::Empty)
        }
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

}

impl UdpSocket {

    /// Returns the underlying net::UdpSocket
    pub fn as_raw_udp_socket(&self) -> &net::UdpSocket {
        &self.socket
    }

}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "UdpSocket({:?})", self.socket)
    }
}

