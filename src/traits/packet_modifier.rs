// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// STD Dependencies -----------------------------------------------------------
use std::fmt;


// Internal Dependencies ------------------------------------------------------
use super::super::Config;


/// Trait describing optional per-packet payload modification logic.
pub trait PacketModifier {

    /// Method that constructs a new packet modifier using the provided configuration.
    fn new(Config) -> Self where Self: Sized;

    /// Method that is called for payload modification before a packet is send
    /// over a connection's underlying socket.
    ///
    /// The default implementation does not actually perform any kind of
    /// modification and leaves the payload to be send untouched.
    fn outgoing(&mut self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Method that is called for payload modification purposes after a packet
    /// is received over a connection's underlying socket.
    ///
    /// The default implementation does not actually perform any kind of
    /// modification and returns leaves received payload untouched.
    fn incoming(&mut self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }

}

impl fmt::Debug for PacketModifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PacketModifier")
    }
}

