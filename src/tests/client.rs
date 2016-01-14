// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate clock_ticks;

use std::thread;
use std::net::SocketAddr;
use super::super::{Client, Connection, Config, Handler, MessageKind, Stats};

struct MockClientHandler {
    pub last_tick_time: u32,
    pub tick_count: u32,
    pub accumulated: u32
}

impl Handler<Client> for MockClientHandler {

    fn connect(&mut self, _: &mut Client) {
        self.last_tick_time = precise_time_ms();
    }

    fn tick_connection(
        &mut self, client: &mut Client,
        _: &mut Connection
    ) {

        // Accumulate time so we can check that the artificial delay
        // was correct for by the servers tick loop
        if self.tick_count > 1 {
            self.accumulated += precise_time_ms() - self.last_tick_time;
        }

        self.last_tick_time = precise_time_ms();
        self.tick_count += 1;

        if self.tick_count == 5 {
            client.close().unwrap();
        }

        // Fake some load inside of the tick handler
        thread::sleep_ms(75);

    }

}

struct MockSyncClientHandler {
    pub connect_count: u32,
    pub tick_count: u32,
    pub close_count: u32
}

impl Handler<Client> for MockSyncClientHandler {

    fn connect(&mut self, _: &mut Client) {
        self.connect_count += 1;
    }

    fn tick_connection(&mut self, _: &mut Client, _: &mut Connection) {
        self.tick_count += 1;
    }

    fn close(&mut self, _: &mut Client) {
        self.close_count += 1;
    }

}

struct MockClientStatsHandler {
    pub tick_count: u32,
}

impl Handler<Client> for MockClientStatsHandler {

    fn connect(&mut self, _: &mut Client) {
    }

    fn tick_connection(
        &mut self, client: &mut Client,
        conn: &mut Connection
    ) {

        conn.send(MessageKind::Instant, b"Hello World".to_vec());
        self.tick_count += 1;

        if self.tick_count == 20 {
            client.close().unwrap();
        }

    }

}

fn precise_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}

#[test]
fn test_client_tick_delay() {

    let config = Config {
        send_rate: 10,
        .. Config::default()
    };

    let mut handler = MockClientHandler {
        last_tick_time: 0,
        tick_count: 0,
        accumulated: 0
    };

    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:12345").unwrap();

    assert!(handler.accumulated <= 350);

}

#[test]
fn test_client_sync() {

    let config = Config {
        send_rate: 10,
        .. Config::default()
    };

    let mut handler = MockSyncClientHandler {
        connect_count: 0,
        tick_count: 0,
        close_count: 0
    };

    // TODO improve sync client tests
    let mut client = Client::new(config);
    let mut state = client.connect_sync(&mut handler, "127.0.0.1:12345").unwrap();
    assert_eq!(handler.tick_count, 0);
    assert_eq!(handler.connect_count, 1);

    assert_eq!(state.rtt(), 0);
    assert_eq!(state.packet_loss(), 0.0);

    let peer_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    assert_eq!(state.peer_addr(), peer_addr);

    client.receive_sync(&mut handler, &mut state, 0);
    client.tick_sync(&mut handler, &mut state);
    assert_eq!(handler.tick_count, 1);
    client.send_sync(&mut handler, &mut state);

    client.receive_sync(&mut handler, &mut state, 0);
    client.tick_sync(&mut handler, &mut state);
    assert_eq!(handler.tick_count, 2);
    client.send_sync(&mut handler, &mut state);

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

    // TODO improve sync client tests
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
        connection_drop_threshold: 2500,
        .. Config::default()
    };

    let mut handler = MockClientStatsHandler {
        tick_count: 0
    };

    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:12346").unwrap();

    assert_eq!(client.stats(), Stats {
        bytes_sent: 300,
        bytes_received: 0
    });

}

