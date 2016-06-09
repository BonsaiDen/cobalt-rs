// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::net::SocketAddr;

use super::mock::{
    MockTickDelayClientHandler,
    MockSyncClientHandler,
    MockClientStatsHandler,
    MockTickRecorder
};
use super::super::{Client, Config, MessageKind, Stats};

#[test]
fn test_client_tick_delay_no_overflow() {

    let mut handler = MockTickDelayClientHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 625)
    };

    let mut server = Client::new(Config {
        send_rate: 30,
        connection_init_threshold: 1000,
        tick_overflow_recovery: false,
        tick_overflow_recovery_rate: 0.0,
        .. Config::default()
    });
    server.connect(&mut handler, "127.0.0.1:12345").unwrap();

}

#[test]
fn test_client_tick_delay_overflow_half() {

    let mut handler = MockTickDelayClientHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 495)
    };

    let mut server = Client::new(Config {
        send_rate: 30,
        connection_init_threshold: 1000,
        tick_overflow_recovery_rate: 0.5,
        .. Config::default()
    });
    server.connect(&mut handler, "127.0.0.1:12345").unwrap();

}

#[test]
fn test_client_tick_delay_overflow_one() {

    let mut handler = MockTickDelayClientHandler {
        tick_recorder: MockTickRecorder::new(15, 4, 30, 495)
    };

    let mut server = Client::new(Config {
        send_rate: 30,
        connection_init_threshold: 1000,
        tick_overflow_recovery_rate: 1.0,
        .. Config::default()
    });
    server.connect(&mut handler, "127.0.0.1:12345").unwrap();

}

#[test]
fn test_client_sync() {

    let config = Config {
        send_rate: 10,
        .. Default::default()
    };

    let mut handler = MockSyncClientHandler {
        connect_count: 0,
        tick_count: 0,
        close_count: 0
    };

    let mut client = Client::new(config);
    let mut state = client.connect_sync(&mut handler, "127.0.0.1:12345").unwrap();
    assert_eq!(handler.tick_count, 0);
    assert_eq!(handler.connect_count, 1);

    assert_eq!(state.rtt(), 0);
    assert_eq!(state.packet_loss(), 0.0);
    assert_eq!(state.stats(), Stats {
        bytes_sent: 0,
        bytes_received: 0
    });

    let peer_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    assert_eq!(state.peer_addr(), peer_addr);

    client.receive_sync(&mut handler, &mut state, 0);
    client.tick_sync(&mut handler, &mut state);
    assert_eq!(handler.tick_count, 1);

    client.send_sync(&mut handler, &mut state);
    assert_eq!(state.stats(), Stats {
        bytes_sent: 14,
        bytes_received: 0
    });

    client.receive_sync(&mut handler, &mut state, 0);
    client.tick_sync(&mut handler, &mut state);
    assert_eq!(handler.tick_count, 2);

    client.send_sync(&mut handler, &mut state);
    assert_eq!(state.stats(), Stats {
        bytes_sent: 28,
        bytes_received: 0
    });

    state.send(MessageKind::Instant, b"Hello World".to_vec());
    client.send_sync(&mut handler, &mut state);
    assert_eq!(state.stats(), Stats {
        bytes_sent: 57,
        bytes_received: 0
    });

    state.reset();

    client.close_sync(&mut handler, &mut state).unwrap();
    assert_eq!(handler.close_count, 1);

}

#[test]
fn test_client_sync_set_config() {

    let config = Config {
        send_rate: 10,
        .. Config::default()
    };

    let mut handler = MockSyncClientHandler {
        connect_count: 0,
        tick_count: 0,
        close_count: 0
    };

    let mut client = Client::new(config);
    let mut state = client.connect_sync(&mut handler, "127.0.0.1:12345").unwrap();

    client.set_config(Config {
        send_rate: 30,
        .. Config::default()

    }, &mut state);

}

#[test]
fn test_client_stats() {

    let config = Config {
        send_rate: 20,
        connection_init_threshold: 1500,
        connection_drop_threshold: 1500,
        .. Config::default()
    };

    let mut handler = MockClientStatsHandler {
        tick_count: 0
    };

    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:12346").unwrap();

    assert_eq!(client.stats(), Stats {
        bytes_sent: 580,
        bytes_received: 0
    });

}

