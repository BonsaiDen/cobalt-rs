extern crate clock_ticks;

use std::thread;
use super::super::{Connection, Config, Handler, Client};

struct MockServerHandler {
    pub last_tick_time: u32,
    pub tick_count: u32,
    pub accumulated: u32
}

impl Handler<Client> for MockServerHandler {

    fn bind(&mut self, _: &mut Client) {
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

fn precise_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}

#[test]
fn test_client_tick_delay() {

    let config = Config {
        send_rate: 10,
        .. Config::default()
    };

    let mut handler = MockServerHandler {
        last_tick_time: 0,
        tick_count: 0,
        accumulated: 0
    };

    let mut client = Client::new(config);
    client.connect(&mut handler, "127.0.0.1:0").unwrap();

    assert!(handler.accumulated <= 350);

}

