extern crate rand;
extern crate clock_ticks;

use std::cmp;
use std::net::SocketAddr;
use std::collections::VecDeque;
use shared::{Config, MessageKind, MessageQueue};
use shared::traits::{Socket, Handler};
use super::message_queue::MessageIterator;

/// Maximum number of acknowledgement bits available in the packet header.
const MAX_ACK_BITS: u32 = 32;

/// Maximum packet sequence number before wrap around happens.
const MAX_SEQ_NUMBER: u32 = 256;

/// Number of bytes used by the protocol header data.
const HEADER_BYTES: usize = 14;

/// A value indicating the current state of a connection.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ConnectionState {

    /// The connection has been opened but has yet to receive the first
    /// incoming packet.
    Connecting,

    /// The remote has responded and at least one incoming packet has been
    /// received.
    Connected,

    /// The remote did not respond with the first packet within the maximum
    /// configured time frame for establishing a connection.
    FailedToConnect,

    /// The remote did not send any packets within the maximum configured time
    /// frame between any two packets.
    Lost,

    /// The connection has been closed programmatically.
    Closed

}

/// Congestion mode of the connection.
#[derive(PartialEq)]
enum CongestionMode {
    Good,
    Bad
}

/// A struct used for packet acknowledgment of sent packets.
struct SentPacketAck {
    seq: u32,
    time: u32,
    acked: bool,
    lost: bool,
    packet: Option<Vec<u8>>
}

/// The Connection ID type.
///
/// Used to uniquely\* identify the reliable connections. The id is send with
/// every packet and is especially helpful in the case of NAT re-assigning
/// local UDP ports which would make a address based identification unreliable.
///
/// > \* The id is a `32` bit random integer; thus, in theory, there is a
/// > potential for two clients using the same id, in which case both will have
/// > their connections dropped very shortly because of data mismatch.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct ConnectionID(pub u32);

/// Implementation of reliable UDP based messaging protocol.
pub struct Connection {

    /// The connection's configuration
    config: Config,

    /// Random Connection ID
    random_id: ConnectionID,

    /// State of the connection
    state: ConnectionState,

    /// The socket address of the remote peer of the connection
    peer_address: SocketAddr,

    /// The most recent received remote sequence number
    remote_seq_number: u32,

    /// The current, local sequence number
    local_seq_number: u32,

    /// Exponentially smoothed moving average of the roundtrip time
    smoothed_rtt: f32,

    /// Last time a packet was received
    last_receive_time: u32,

    /// A queue of recently received packets used for ack bitfield construction
    recv_ack_queue: VecDeque<u32>,

    /// A queue of recently send packets pending acknowledgment
    sent_ack_queue: Vec<SentPacketAck>,

    /// Number of all packets sent over the connection
    sent_packets: u32,

    /// Number of all packets received over the connection
    recv_packets: u32,

    /// Number of all packets sent which were acknowledged
    lost_packets: u32,

    /// Number of all packets sent which were lost
    acked_packets: u32,

    /// Sending mode of the connection, based on congestion
    mode: CongestionMode,

    /// Time in milliseconds until we try out `Good` mode when in `Bad`
    /// congestion mode.
    time_until_good_mode: u32,

    /// Last time the congestion mode switched
    last_mode_switch_time: u32,

    /// The internal message queue of the connection
    message_queue: MessageQueue

}

impl Connection {

    /// Creates a new Virtual Connection over the given `SocketAddr`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::SocketAddr;
    /// use cobalt::shared::{Connection, ConnectionState, Config};
    ///
    /// let address: SocketAddr = "127.0.0.1:0".parse().unwrap();
    /// let conn = Connection::new(Config::default(), address);
    ///
    /// assert_eq!(conn.state(), ConnectionState::Connecting);
    /// assert_eq!(conn.open(), true);
    /// ```
    pub fn new(config: Config, peer_addr: SocketAddr) -> Connection {
        Connection {
            config: config,
            random_id: ConnectionID(rand::random()),
            state: ConnectionState::Connecting,
            peer_address: peer_addr,
            local_seq_number: 0,
            remote_seq_number: 0,
            smoothed_rtt: 0.0,
            last_receive_time: precise_time_ms(),
            recv_ack_queue: VecDeque::new(),
            sent_ack_queue: Vec::new(),
            sent_packets: 0,
            recv_packets: 0,
            acked_packets: 0,
            lost_packets: 0,
            mode: CongestionMode::Good,
            time_until_good_mode: config.congestion_switch_min_delay,
            last_mode_switch_time: 0,
            message_queue: MessageQueue::new(config)
        }
    }

    /// Extracts a `ConnectionID` from packet with a valid protocol header.
    ///
    /// # Examples
    ///
    /// ```
    /// use cobalt::shared::{Connection, ConnectionID, Config};
    ///
    /// let config = Config {
    ///     protocol_header: [11, 22, 33, 44],
    ///     ..Config::default()
    /// };
    ///
    /// let packet = [
    ///     11, 22, 33, 44,
    ///      1,  2,  3,  4,
    ///      0,
    ///      0,
    ///      0,  0, 0,  0
    /// ];
    ///
    /// let conn_id = Connection::id_from_packet(&config, &packet);
    ///
    /// assert_eq!(conn_id, Some(ConnectionID(16909060)));
    /// ```
    pub fn id_from_packet(config: &Config, packet: &[u8]) -> Option<ConnectionID> {
        if &packet[0..4] == &config.protocol_header {
            Some(ConnectionID(
                (packet[4] as u32) << 24 | (packet[5] as u32) << 16 |
                (packet[6] as u32) << 8  |  packet[7] as u32
            ))

        } else {
            None
        }
    }

    /// Returns whether the connection is currently accepting any incoming
    /// packets.
    pub fn open(&self) -> bool {
        match self.state {
            ConnectionState::Connecting => true,
            ConnectionState::Connected => true,
            _ => false
        }
    }

    /// Return whether the connection is currently congested and should be
    /// sending less data.
    pub fn congested(&self) -> bool {
        self.mode == CongestionMode::Bad
    }

    /// Returns the id of the connection.
    pub fn id(&self) -> ConnectionID {
        self.random_id
    }

    /// Returns the current state of the connection.
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the average roundtrip time for the connection.
    pub fn rtt(&self) -> u32 {
        self.smoothed_rtt.ceil() as u32
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across the
    /// connection.
    pub fn packet_loss(&self) -> f32 {
        100.0 / cmp::max(self.sent_packets, 1) as f32
                           * self.lost_packets as f32
    }

    /// Returns the socket address of the remote peer of this connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_address
    }

    /// Sets the socket address of the remote peer of this connection.
    pub fn set_peer_addr(&mut self, peer_addr: SocketAddr) {
        self.peer_address = peer_addr;
    }

    /// Sends a message of the specified `kind` along with its `data` over the
    /// connection.
    ///
    /// How exactly the message is send and whether it is guranteed to be
    /// delivered eventually is determined by its `MessageKind`.
    pub fn send(&mut self, kind: MessageKind, data: Vec<u8>) {
        self.message_queue.send(kind, data);
    }

    /// Returns a consuming iterator over all messages received over this
    /// connections.
    pub fn received(&mut self) -> MessageIterator {
        self.message_queue.received()
    }

    /// Receives a incoming UDP packet.
    pub fn receive_packet<T>(
        &mut self, packet: Vec<u8>, owner: &mut T, handler: &mut Handler<T>
    ) {

        // Update connection state
        if self.update_receive_state(&packet, owner, handler) == false {
            return
        }

        // Update time used for disconnect detection
        self.last_receive_time = precise_time_ms();

        // Read remote sequence number
        self.remote_seq_number = packet[8] as u32;

        // Get latest acknowledge sequence number
        let ack_seq_number = packet[9] as u32;

        // Get acknowledgement bitfield
        let bitfield = (packet[10] as u32) << 24
                     | (packet[11] as u32) << 16
                     | (packet[12] as u32) << 8
                     |  packet[13] as u32;

        // Check most recently send packets for their acknowledgment
        let mut lost_packets = Vec::new();
        for p in self.sent_ack_queue.iter_mut() {

            // Check if we have received a ack for the packet
            let acked = seq_was_acked(p.seq, ack_seq_number, bitfield);

            // Calculate the RTT from acknowledged packets
            if acked {
                // TODO refactor
                self.smoothed_rtt = (self.smoothed_rtt - (self.smoothed_rtt - (self.last_receive_time - p.time) as f32) * 0.10).max(0.0);
                self.acked_packets += 1;
                p.acked = true;

            // Detect any lost packets
            } else if self.last_receive_time - p.time
                    > self.config.packet_drop_threshold {

                lost_packets.push(p.packet.take().unwrap());
                self.lost_packets += 1;
                p.lost = true;
            }

        }

        // Push packet data into message queue
        self.message_queue.receive_packet(&packet[HEADER_BYTES..]);

        // Notify handler of lost packets
        for packet in lost_packets {
            self.message_queue.lost_packet(&packet[HEADER_BYTES..]);
            handler.connection_packet_lost(owner, self, &packet[HEADER_BYTES..]);
        }

        // Remove acked / lost packets from the queue
        self.sent_ack_queue.retain(|p| !(p.lost || p.acked));

        // Insert packet into receive acknowledgment queue
        self.recv_ack_queue.push_front(self.remote_seq_number);

        // Don't keep more entries than we can actually acknowledge per packet
        if self.recv_ack_queue.len() > MAX_ACK_BITS as usize {
            self.recv_ack_queue.pop_back();
        }

        // Update packet statistics
        self.recv_packets += 1;

    }

    /// Creates a new outgoing UDP packet.
    pub fn send_packet<T, S: Socket>(
        &mut self,
        socket: &mut S, address: &SocketAddr,
        owner: &mut T, handler: &mut Handler<T>
    ) {

        // Update connection state
        if self.update_send_state(owner, handler) == false {
            return;
        }

        // Update congestion state
        self.update_congestion_state(owner, handler);

        // Take write buffer out and insert a fresh, empty one in its place
        let mut packet = Vec::<u8>::with_capacity(HEADER_BYTES);

        // Set packet protocol header
        packet.push(self.config.protocol_header[0]);
        packet.push(self.config.protocol_header[1]);
        packet.push(self.config.protocol_header[2]);
        packet.push(self.config.protocol_header[3]);

        // Set connection ID
        packet.push((self.random_id.0 >> 24) as u8);
        packet.push((self.random_id.0 >> 16) as u8);
        packet.push((self.random_id.0 >> 8) as u8);
        packet.push(self.random_id.0 as u8);

        // Set local sequence number
        packet.push(self.local_seq_number as u8);

        // Set packet ack number
        packet.push(self.remote_seq_number as u8);

        // Construct ack bitfield from most recently received packets
        let mut bitfield: u32 = 0;
        for seq in self.recv_ack_queue.iter() {

            // Ignore the remote sequence as it already gets set in the header
            if *seq != self.remote_seq_number {

                // Calculate bitfield index
                let bit = seq_bit_index(*seq, self.remote_seq_number);

                // Set ack bit
                if bit < MAX_ACK_BITS {
                    bitfield |= (1 << bit) as u32;
                }

            }

        }

        // Set ack bitfield
        packet.push((bitfield >> 24) as u8);
        packet.push((bitfield >> 16) as u8);
        packet.push((bitfield >> 8) as u8);
        packet.push(bitfield as u8);

        // Write messages from queue into the packet
        self.message_queue.send_packet(
            &mut packet, self.config.packet_max_size - HEADER_BYTES
        );

        // Send packet to socket
        socket.send(*address, &packet[..]).ok();

        // Insert packet into send acknowledgment queue (but avoid dupes)
        if self.send_ack_required(self.local_seq_number) == true {
            self.sent_ack_queue.push(SentPacketAck {
                seq: self.local_seq_number,
                time: precise_time_ms(),
                acked: false,
                lost: false,
                packet: Some(packet)
            });
        }

        // Increase local sequence number and wrap around
        self.local_seq_number += 1;

        if self.local_seq_number == MAX_SEQ_NUMBER {
            self.local_seq_number = 0;
        }

        // Update packet statistics
        self.sent_packets += 1;

    }

    /// Resets the connection for re-use with another address.
    pub fn reset(&mut self) {
        self.state = ConnectionState::Connecting;
        self.mode = CongestionMode::Good;
        self.time_until_good_mode = self.config.congestion_switch_min_delay;
        self.last_mode_switch_time = 0;
        self.local_seq_number = 0;
        self.remote_seq_number = 0;
        self.smoothed_rtt = 0.0;
        self.last_receive_time = precise_time_ms();
        self.recv_ack_queue.clear();
        self.sent_ack_queue.clear();
        self.sent_packets = 0;
        self.recv_packets = 0;
        self.acked_packets = 0;
        self.lost_packets = 0;
        self.message_queue.reset();
    }

    /// Closes the connection, no further packets will be received or send.
    pub fn close(&mut self) {
        self.state = ConnectionState::Closed;
    }


    // Internal State Handling ------------------------------------------------

    fn update_receive_state<T>(
        &mut self, packet: &[u8], owner: &mut T, handler: &mut Handler<T>
    ) -> bool {

        // Ignore any packets which do not match the desired protocol header
        &packet[0..4] == &self.config.protocol_header && match self.state {

            ConnectionState::Lost => false,

            ConnectionState::Closed => false,

            ConnectionState::FailedToConnect => false,

            ConnectionState::Connecting => {

                // Once we receive the first valid packet we consider the
                // connection as established
                self.state = ConnectionState::Connected;
                handler.connection(owner, self);
                true

            },

            ConnectionState::Connected => {

                // Check if the packet sequence number is more recent,
                // otherwise drop it as a duplicate
                seq_is_more_recent(
                    packet[8] as u32, self.remote_seq_number
                )

            }

        }

    }

    fn update_send_state<T>(
        &mut self, owner: &mut T, handler: &mut Handler<T>
    ) -> bool {

        // Calculate time since last received packet
        let inactive_time = precise_time_ms() - self.last_receive_time;

        match self.state {

            ConnectionState::Lost => false,

            ConnectionState::Closed => false,

            ConnectionState::FailedToConnect => false,

            ConnectionState::Connecting => {

                // Quickly detect initial connection failures
                if inactive_time > self.config.connection_init_threshold {
                    self.state = ConnectionState::FailedToConnect;
                    handler.connection_failed(owner, self);
                    false

                } else {
                    true
                }

            },

            ConnectionState::Connected => {

                // Detect connection timeouts
                if inactive_time > self.config.connection_drop_threshold {
                    self.state = ConnectionState::Lost;
                    handler.connection_lost(owner, self);
                    false

                } else {
                    true
                }

            }
        }

    }

    // Internal Congestion Handling -------------------------------------------

    fn update_congestion_state<T>(
        &mut self, owner: &mut T, handler: &mut Handler<T>
    ) {
        match self.mode {
            CongestionMode::Good => self.update_good_congestion(owner, handler),
            CongestionMode::Bad => self.update_bad_congestion(owner, handler)
        }
    }

    fn update_good_congestion<T>(
        &mut self, owner: &mut T, handler: &mut Handler<T>
    ) {

        if self.rtt() <= self.config.congestion_rtt_threshold {

            // The we stay in good mode for a prolonged period of time,
            // we reduce the wait time before trying out good mode in case we
            // entered bad mode
            if self.mode_switch_delay_exceeded() {
                self.time_until_good_mode = cmp::max(
                    self.time_until_good_mode / 2,
                    self.config.congestion_switch_min_delay
                );
                self.last_mode_switch_time = precise_time_ms();
            }

        } else {

            // Switch to bad mode once we exceed the RTT threshold
            self.mode = CongestionMode::Bad;

            // If we back fall into bad mode quickly, we'll double the wait
            // time before we try out good mode
            if self.mode_switch_delay_exceeded() == false {
                self.time_until_good_mode = cmp::min(
                    self.time_until_good_mode * 2,
                    self.config.congestion_switch_max_delay
                );
            }

            self.last_mode_switch_time = precise_time_ms();

            handler.connection_congested(owner, self);

        }

    }

    fn update_bad_congestion<T>(
        &mut self, owner: &mut T, handler: &mut Handler<T>
    ) {

        if self.rtt() <= self.config.congestion_rtt_threshold {

            // Switch back to good mode if we stay within the RTT threshold for
            // the required time
            if precise_time_ms() - self.last_mode_switch_time
                                 > self.time_until_good_mode {

                self.mode = CongestionMode::Good;
                self.last_mode_switch_time = precise_time_ms();
                handler.connection_congested(owner, self);
            }

        // If we still got congestion, we further delay the next mode
        // switch until we have at least `switch delay` seconds of
        // good RTT
        } else {
            self.last_mode_switch_time = precise_time_ms();
        }

    }


    // Internal Helpers -------------------------------------------------------
    fn send_ack_required(&self, seq: u32) -> bool {
        self.sent_ack_queue.iter().any(|p| p.seq == seq) == false
    }

    fn mode_switch_delay_exceeded(&self) -> bool {
        precise_time_ms() - self.last_mode_switch_time
                          > self.config.congestion_switch_wait
    }

}

// Static Helpers -------------------------------------------------------------
fn seq_bit_index(seq: u32, ack: u32) -> u32 {
    if seq > ack {
        ack + (MAX_SEQ_NUMBER - 1 - seq)

    } else {
        ack - 1 - seq
    }
}

fn seq_is_more_recent(a: u32, b: u32) -> bool {
    (a > b) && (a - b <= MAX_SEQ_NUMBER / 2)
    || (b > a) && (b - a > MAX_SEQ_NUMBER / 2)
}

fn seq_was_acked(seq: u32, ack: u32, bitfield: u32) -> bool {
    if seq == ack {
        true

    } else {
        let bit = seq_bit_index(seq, ack);
        bit < MAX_ACK_BITS && (bitfield & (1 << bit)) != 0
    }
}

fn precise_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}


#[cfg(test)]
mod tests {

    use std::net;
    use std::io::Error;
    use std::sync::mpsc::channel;

    use shared::{Connection, ConnectionState, Config, MessageKind};
    use shared::traits::{Handler, Socket, SocketReader};

    #[test]
    fn test_create() {
        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let conn = Connection::new(Config::default(), address);
        assert_eq!(conn.open(), true);
        assert_eq!(conn.congested(), false);
        assert_eq!(conn.state(), ConnectionState::Connecting);
        assert_eq!(conn.rtt(), 0);
        assert_eq!(conn.packet_loss(), 0.0);
    }

    #[test]
    fn test_close() {
        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let mut conn = Connection::new(Config::default(), address);
        conn.close();
        assert_eq!(conn.open(), false);
        assert_eq!(conn.state(), ConnectionState::Closed);
    }

    #[test]
    fn test_reset() {
        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let mut conn = Connection::new(Config::default(), address);
        conn.close();
        conn.reset();
        assert_eq!(conn.open(), true);
        assert_eq!(conn.state(), ConnectionState::Connecting);
    }

    #[test]
    fn test_send_sequence_wrap_around() {

        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let mut conn = Connection::new(Config::default(), address);

        let mut socket = MockSocket::new(Vec::new());
        let mut owner = MockOwner;
        let mut handler = MockOwnerHandler;

        for i in 0..256 {

            let expected = vec![[
                // protocol id
                1, 2, 3, 4,

                // connection id
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,

                i as u8, // local sequence number
                0, // remote sequence number
                0, 0, 0, 0  // ack bitfield

            ].to_vec()];

            socket.expect(expected);

            conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        }

        // Should now have wrapped around
        let expected = vec![[
            // protocol id
            1, 2, 3, 4,

            // connection id
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            0, // local sequence number
            0, // remote sequence number
            0, 0, 0, 0  // ack bitfield

        ].to_vec()];

        socket.expect(expected);

        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    }

    #[test]
    fn test_send_and_receive_packet() {

        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let mut conn = Connection::new(Config::default(), address);
        let mut expected_packets: Vec<Vec<u8>> = Vec::new();

        // Initial packet
        expected_packets.push([
            // protocol id
            1, 2, 3, 4,

            // connection id
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            0, // local sequence number
            0, // remote sequence number
            0, 0, 0, 0  // ack bitfield

        ].to_vec());

        // Write packet
        expected_packets.push([
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1, // local sequence number
            0,
            0, 0, 0, 0

        ].to_vec());

        // Empty again
        expected_packets.push([
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            2, // local sequence number
            0,
            0, 0, 0, 0

        ].to_vec());


        // Ack Bitfield test
        expected_packets.push([
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            3, // local sequence number
            27, // remove sequence number set by receive_packet)

            // Ack bitfield
            0, 0, 3, 128 // 0000_0000 0000_0000 0000_0011 1000_0000

        ].to_vec());


        // Testing
        let mut socket = MockSocket::new(expected_packets);
        let mut owner = MockOwner;
        let mut handler = MockOwnerHandler;

        // Test Initial Packet
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Test sending of written data
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Write buffer should get cleared
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Test receiving of a packet with acknowledgements for two older packets
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
            17, // local sequence number
            2, // remote sequence number we confirm
            0, 0, 0, 3, // confirm the first two packets

        ].to_vec(), &mut owner, &mut handler);


        // Receive additional packet
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
            18, // local sequence number
            3, // remote sequence number we confirm
            0, 0, 0, 0

        ].to_vec(), &mut owner, &mut handler);

        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
            19, // local sequence number
            4, // remote sequence number we confirm
            0, 0, 0, 0

        ].to_vec(), &mut owner, &mut handler);

        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
            27, // local sequence number
            4, // remote sequence number we confirm
            0, 0, 0, 0

        ].to_vec(), &mut owner, &mut handler);

        // Test Receive Ack Bitfield
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

    }

    #[test]
    fn test_send_and_receive_message() {

        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let mut conn = Connection::new(Config::default(), address);
        let mut owner = MockOwner;
        let mut handler = MockOwnerHandler;

        // Expected packet data
        let mut socket = MockSocket::new(vec![
            [
                1, 2, 3, 4,
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,
                0,
                0,
                0, 0, 0, 0,

                // Foo
                0, 0, 3, 70, 111, 111,

                // Bar
                0, 0, 3, 66, 97, 114,

                // Test
                1, 0, 4, 84, 101, 115, 116,

                // Hello
                2, 0, 5, 72, 101, 108, 108, 111,

                // World
                2, 1, 5, 87, 111, 114, 108, 100

            ].to_vec()
        ]);

        // Test Message Sending
        conn.send(MessageKind::Instant, b"Foo".to_vec());
        conn.send(MessageKind::Instant, b"Bar".to_vec());
        conn.send(MessageKind::Reliable, b"Test".to_vec());
        conn.send(MessageKind::Ordered, b"Hello".to_vec());
        conn.send(MessageKind::Ordered, b"World".to_vec());
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Test Message Receival
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            0,
            0,
            0, 0, 0, 0,

            // Foo
            0, 0, 3, 70, 111, 111,

            // Bar
            0, 0, 3, 66, 97, 114,

            // Test
            1, 0, 4, 84, 101, 115, 116,

            // We actually test inverse receival order here!

            // World
            2, 1, 5, 87, 111, 114, 108, 100,

            // Hello
            2, 0, 5, 72, 101, 108, 108, 111

        ].to_vec(), &mut owner, &mut handler);

        // Get received messages
        let mut messages = Vec::new();
        for msg in conn.received() {
            messages.push(msg);
        }

        assert_eq!(messages, vec![
            b"Foo".to_vec(),
            b"Bar".to_vec(),
            b"Test".to_vec(),
            b"Hello".to_vec(),
            b"World".to_vec()
        ]);

    }

    // Owner Mock -----------------------------------------------------------------
    pub struct MockOwner;
    pub struct MockOwnerHandler;
    impl Handler<MockOwner> for MockOwnerHandler {}


    // Socket Mock ----------------------------------------------------------------
    pub struct MockSocket {
        send_packets: Vec<Vec<u8>>,
        send_index: usize,
        receiver: Option<SocketReader>
    }

    impl MockSocket {

        pub fn new(send_packets: Vec<Vec<u8>>) -> MockSocket {

            let (_, receiver) = channel::<(net::SocketAddr, Vec<u8>)>();

            MockSocket {
                send_packets: send_packets,
                send_index: 0,
                receiver: Some(receiver)
            }

        }

        pub fn expect(&mut self, send_packets: Vec<Vec<u8>>) {
            self.send_index = 0;
            self.send_packets = send_packets;
        }

    }

    impl Socket for MockSocket {

        fn reader(&mut self) -> Option<SocketReader> {
            self.receiver.take()
        }

        fn send<T: net::ToSocketAddrs>(
            &mut self, _: T, data: &[u8])
        -> Result<usize, Error> {

            // Don't run out of expected packets
            assert!(self.send_index < self.send_packets.len());

            // Verify packet data
            assert_eq!(data, &self.send_packets[self.send_index][..]);

            self.send_index += 1;
            Ok(0)

        }

    }

}

