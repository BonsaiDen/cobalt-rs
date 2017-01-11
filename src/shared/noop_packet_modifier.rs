// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// Internal Dependencies ------------------------------------------------------
use ::{Config, PacketModifier};


/// Implementation of a noop packet modifier.
#[derive(Debug, Copy, Clone)]
pub struct NoopPacketModifier;

impl PacketModifier for NoopPacketModifier {

    fn new(_: Config) -> NoopPacketModifier {
        NoopPacketModifier
    }

}

