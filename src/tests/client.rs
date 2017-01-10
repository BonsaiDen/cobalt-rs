// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io::{Error, ErrorKind};
use std::sync::mpsc::TryRecvError;

use super::super::{
    BinaryRateLimiter, Client, Config, MessageKind, Stats, UdpSocket
};

#[test]
fn test_client_disconnected() {

    let mut client = Client::<UdpSocket, BinaryRateLimiter>::new(Config {
        send_rate: 30,
        .. Config::default()
    });

    assert_eq!(client.bytes_sent(), 0);
    assert_eq!(client.bytes_received(), 0);

    assert_eq!(client.peer_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(client.local_addr().unwrap_err().kind(), ErrorKind::AddrNotAvailable);
    assert_eq!(client.connection().unwrap_err().kind(), ErrorKind::NotConnected);

    assert_eq!(client.receive(), Err(TryRecvError::Disconnected));
    assert_eq!(client.send(MessageKind::Instant, Vec::new()).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.flush(false).unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.reset().unwrap_err().kind(), ErrorKind::NotConnected);
    assert_eq!(client.close().unwrap_err().kind(), ErrorKind::NotConnected);

}

#[test]
fn test_client_configuration() {

    let config = Config {
        send_rate: 30,
        .. Config::default()
    };

    let mut client = Client::<UdpSocket, BinaryRateLimiter>::new(config);
    assert_eq!(client.config(), config);

    let config = Config {
        send_rate: 60,
        .. Config::default()
    };

    client.set_config(config);
    assert_eq!(client.config(), config);

}

