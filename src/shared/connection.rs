extern crate rand;
extern crate clock_ticks;

use std::cmp;
use std::net::SocketAddr;
use std::collections::VecDeque;
use super::message_queue::{MessageQueue, MessageIterator};
use super::super::traits::socket::Socket;
use super::super::{Config, MessageKind, Handler, RateLimiter};

/// Maximum number of acknowledgement bits available in the packet header.
const MAX_ACK_BITS: u32 = 32;

/// Maximum packet sequence number before wrap around happens.
const MAX_SEQ_NUMBER: u32 = 256;

/// Number of bytes used by a packet header.
const PACKET_HEADER_SIZE: usize = 14;

/// Enum indicating the state of a `SentPacketAck`.
#[derive(PartialEq)]
enum PacketState {
    Unknown,
    Acked,
    Lost
}

/// Structure used for packet acknowledgment of sent packets.
struct SentPacketAck {
    seq: u32,
    time: u32,
    state: PacketState,
    packet: Option<Vec<u8>>
}

/// Enum indicating the state of a connection.
#[derive(PartialEq, Copy, Clone)]
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

/// Representation of a random id for connection identification.
///
/// Used to uniquely\* identify the reliable connections. The id is send with
/// every packet and is especially helpful in the case of NAT re-assigning
/// local UDP ports which would make a address based identification unreliable.
///
/// > \* Since the id is random integer, there is of course a very small chance
/// > for two clients to end up using the same id, in that case - due to
/// conflicting ack sequences and message data - both will get dropped shortly.
#[derive(PartialEq, Eq, Hash, Copy, Clone)]
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

    /// Queue of recently received packets used for ack bitfield construction
    recv_ack_queue: VecDeque<u32>,

    /// Queue of recently send packets pending acknowledgment
    sent_ack_queue: Vec<SentPacketAck>,

    /// Number of all packets sent over the connection
    sent_packets: u32,

    /// Number of all packets received over the connection
    recv_packets: u32,

    /// Number of all packets sent which were acknowledged
    lost_packets: u32,

    /// Number of all packets sent which were lost
    acked_packets: u32,

    /// The internal message queue of the connection
    message_queue: MessageQueue,

    /// The rate limiter used to handle and avoid network congestion
    rate_limiter: Box<RateLimiter>

}

impl Connection {

    /// Creates a new Virtual Connection over the given `SocketAddr`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::SocketAddr;
    /// use cobalt::{BinaryRateLimiter, Connection, ConnectionState, Config};
    ///
    /// let config = Config::default();
    /// let address: SocketAddr = "127.0.0.1:0".parse().unwrap();
    /// let limiter = BinaryRateLimiter::new(&config);
    /// let conn = Connection::new(config, address, limiter);
    ///
    /// assert!(conn.state() == ConnectionState::Connecting);
    /// assert_eq!(conn.open(), true);
    /// ```
    pub fn new(
        config: Config, peer_addr: SocketAddr, rate_limiter: Box<RateLimiter>

    ) -> Connection {
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
            message_queue: MessageQueue::new(config),
            rate_limiter: rate_limiter
        }
    }

    /// Extracts a `ConnectionID` from packet with a valid protocol header.
    ///
    /// # Examples
    ///
    /// ```
    /// use cobalt::{Connection, ConnectionID, Config};
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
    /// assert!(conn_id == Some(ConnectionID(16909060)));
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

    /// Returns whether the connection is currently congested and should be
    /// sending less packets per second in order to resolve the congestion.
    pub fn congested(&self) -> bool {
        self.rate_limiter.congested()
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
        100.0 / cmp::max(self.sent_packets, 1) as f32 * self.lost_packets as f32
    }

    /// Returns the socket address of the remote peer of this connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_address
    }

    /// Sets the socket address of the remote peer of this connection.
    pub fn set_peer_addr(&mut self, peer_addr: SocketAddr) {
        self.peer_address = peer_addr;
    }

    /// Sends a message of the specified `kind` along with its `payload` over
    /// the connection.
    ///
    /// How exactly the message is send and whether it is guaranteed to be
    /// delivered eventually is determined by its `MessageKind`.
    pub fn send(&mut self, kind: MessageKind, payload: Vec<u8>) {
        self.message_queue.send(kind, payload);
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

        // Ignore any packets shorter then the header length
        if packet.len() < PACKET_HEADER_SIZE {
            return;

        // Update connection state
        } else if self.update_receive_state(&packet, owner, handler) == false {
            return;
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

        // Check recently send packets for their acknowledgment
        for i in 0..self.sent_ack_queue.len() {

            if let Some(lost_packet) = {

                let ack = self.sent_ack_queue.get_mut(i).unwrap();

                // Calculate the roundtrip time from acknowledged packets
                if seq_was_acked(ack.seq, ack_seq_number, bitfield) {
                    self.acked_packets += 1;
                    self.smoothed_rtt = moving_average(
                        self.smoothed_rtt,
                        (self.last_receive_time - ack.time) as f32
                    );
                    ack.state = PacketState::Acked;
                    None

                // Extract data from lost packets
                } else if self.last_receive_time - ack.time
                        > self.config.packet_drop_threshold {

                    self.lost_packets += 1;
                    ack.state = PacketState::Lost;
                    ack.packet.take()

                // Keep all pending packets
                } else {
                    None
                }

            // Push messages from lost packets into the queue and notify the handler
            } {
                self.message_queue.lost_packet(&lost_packet[PACKET_HEADER_SIZE..]);
                handler.connection_packet_lost(owner, self, &lost_packet[PACKET_HEADER_SIZE..]);
            }

        }

        // Push packet data into message queue
        self.message_queue.receive_packet(&packet[PACKET_HEADER_SIZE..]);

        // Remove all acknowledged and lost packets from the sent ack queue
        self.sent_ack_queue.retain(|p| p.state == PacketState::Unknown);

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

        let congested = self.rate_limiter.congested();
        let rtt = self.rtt();
        let packet_loss = self.packet_loss();

        // Update congestion state
        self.rate_limiter.update(rtt, packet_loss);

        // Check if the state changed and invoke handler
        if congested != self.rate_limiter.congested() {
            handler.connection_congestion_state(owner, self, !congested);
        }

        // Check if we should be sending packets, if not skip this packet
        if self.rate_limiter.should_send() == false {
            return;
        }

        // Take write buffer out and insert a fresh, empty one in its place
        let mut packet = Vec::<u8>::with_capacity(PACKET_HEADER_SIZE);

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
            &mut packet, self.config.packet_max_size - PACKET_HEADER_SIZE
        );

        // Send packet to socket
        socket.send(*address, &packet[..]).ok();

        // Insert packet into send acknowledgment queue (but avoid dupes)
        if self.send_ack_required(self.local_seq_number) == true {
            self.sent_ack_queue.push(SentPacketAck {
                seq: self.local_seq_number,
                time: precise_time_ms(),
                state: PacketState::Unknown,
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
        self.rate_limiter.reset();
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

                // The connection handler might decide to immediately disconnect
                // us, so we check out state again
                self.state == ConnectionState::Connected

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

    // Internal Helpers -------------------------------------------------------
    fn send_ack_required(&self, seq: u32) -> bool {
        self.sent_ack_queue.iter().any(|p| p.seq == seq) == false
    }

}

// Static Helpers -------------------------------------------------------------
fn moving_average(a: f32, b: f32) -> f32 {
    (a - (a - b) * 0.10).max(0.0)
}


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
    use std::thread;
    use std::io::Error;
    use std::sync::mpsc::channel;

    use super::super::super::traits::socket::{Socket, SocketReader};
    use super::super::super::{
            BinaryRateLimiter,
            Connection,
            ConnectionState,
            Config,
            MessageKind,
            Handler
    };

    fn connection() -> Connection {
        let config = Config::default();
        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let limiter = BinaryRateLimiter::new(&config);
        Connection::new(config, address, limiter)
    }

    #[test]
    fn test_create() {
        let conn = connection();
        assert_eq!(conn.open(), true);
        assert_eq!(conn.congested(), false);
        assert!(conn.state() == ConnectionState::Connecting);
        assert_eq!(conn.rtt(), 0);
        assert_eq!(conn.packet_loss(), 0.0);
    }

    #[test]
    fn test_close() {
        let mut conn = connection();
        conn.close();
        assert_eq!(conn.open(), false);
        assert!(conn.state() == ConnectionState::Closed);
    }

    #[test]
    fn test_reset() {
        let mut conn = connection();
        conn.close();
        conn.reset();
        assert_eq!(conn.open(), true);
        assert!(conn.state() == ConnectionState::Connecting);
    }

    #[test]
    fn test_send_sequence_wrap_around() {

        let mut conn = connection();
        let address = conn.peer_addr();
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

        let mut conn = connection();
        let address = conn.peer_addr();
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

        let mut conn = connection();
        let address = conn.peer_addr();
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
                0, 0, 0, 3, 70, 111, 111,

                // Bar
                0, 0, 0, 3, 66, 97, 114,

                // Test
                1, 0, 0, 4, 84, 101, 115, 116,

                // Hello
                2, 0, 0, 5, 72, 101, 108, 108, 111,

                // World
                2, 1, 0, 5, 87, 111, 114, 108, 100

            ].to_vec()
        ]);

        // Test Message Sending
        conn.send(MessageKind::Instant, b"Foo".to_vec());
        conn.send(MessageKind::Instant, b"Bar".to_vec());
        conn.send(MessageKind::Reliable, b"Test".to_vec());
        conn.send(MessageKind::Ordered, b"Hello".to_vec());
        conn.send(MessageKind::Ordered, b"World".to_vec());
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Test Message Receiving
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            0,
            0,
            0, 0, 0, 0,

            // Foo
            0, 0, 0, 3, 70, 111, 111,

            // Bar
            0, 0, 0, 3, 66, 97, 114,

            // Test
            1, 0, 0, 4, 84, 101, 115, 116,

            // We actually test inverse receiving order here!

            // World
            2, 1, 0, 5, 87, 111, 114, 108, 100,

            // Hello
            2, 0, 0, 5, 72, 101, 108, 108, 111

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

    #[test]
    fn test_receive_invalid_packets() {

        let mut conn = connection();
        let mut owner = MockOwner;
        let mut handler = MockOwnerHandler;

        // Empty packet
        conn.receive_packet([].to_vec(), &mut owner, &mut handler);

        // Garbage packet
        conn.receive_packet([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14

        ].to_vec(), &mut owner, &mut handler);

    }

    #[test]
    fn test_rtt() {

        let mut conn = connection();
        let address = conn.peer_addr();
        let mut owner = MockOwner;
        let mut handler = MockOwnerHandler;

        let mut socket = MockSocket::new(vec![
            [
                1, 2, 3, 4,
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,
                0,
                0,
                0, 0, 0, 0
            ].to_vec(),
            [
                1, 2, 3, 4,
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,
                1,
                0,
                0, 0, 0, 0
            ].to_vec()
        ]);

        assert_eq!(conn.rtt(), 0);

        // First packet
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
        thread::sleep_ms(100);
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            0,
            0, // confirm the packet above
            0, 0,
            0, 0

        ].to_vec(), &mut owner, &mut handler);

        // Expect RTT value to have moved by 10% of the overall roundtrip time
        assert!(conn.rtt() >= 10);

        // Second packet
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            1,
            1, // confirm the packet above
            0, 0,
            0, 0

        ].to_vec(), &mut owner, &mut handler);

        // Expect RTT to have reduced by 10%
        assert!(conn.rtt() <= 10);

    }

    #[test]
    fn test_packet_loss() {

        struct PacketLossHandler;
        impl Handler<MockOwner> for PacketLossHandler {

            fn connection_packet_lost(
                &mut self, _: &mut MockOwner, _: &mut Connection, packet: &[u8]
            ) {
                assert_eq!([
                    0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,
                    1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,
                    2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100
                ].to_vec(), packet);
            }

        }

        let config = Config {
            // set a low threshold for packet loss
            packet_drop_threshold: 10,
            .. Config::default()
        };

        let address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
        let limiter = BinaryRateLimiter::new(&config);
        let mut conn = Connection::new(config, address, limiter);
        let mut owner = MockOwner;
        let mut handler = PacketLossHandler;

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

                // Packet 1
                0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,

                // Packet 2
                1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

                // Packet 3
                2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

            ].to_vec(),
            [
                1, 2, 3, 4,
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,
                1,
                0,
                0, 0, 0, 0,

                // Packet 2
                1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

                // Packet 3
                2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

            ].to_vec()
        ]);

        conn.send(MessageKind::Instant, b"Packet Instant".to_vec());
        conn.send(MessageKind::Reliable, b"Packet Reliable".to_vec());
        conn.send(MessageKind::Ordered, b"Packet Ordered".to_vec());
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        assert_eq!(conn.packet_loss(), 0.0);

        // Wait a bit so the packets will definitely get dropped
        thread::sleep_ms(20);

        // Now receive a packet and check for the lost packets
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            0, 2, 0, 0, // Set ack seq to non-0 so we trigger the packet loss
            0, 0

        ].to_vec(), &mut owner, &mut handler);

        // RTT should be left untouched the lost packet
        assert_eq!(conn.rtt(), 0);

        // But packet loss should spike up
        assert_eq!(conn.packet_loss(), 100.0);

        // The messages from the lost packet should have been re-inserted into
        // the message_queue and should be send again with the next packet.
        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        // Fully receive the next packet
        conn.receive_packet([
            1, 2, 3, 4,
            0, 0, 0, 0,
            0, 1, 0, 0,
            0, 0

        ].to_vec(), &mut owner, &mut handler);

        // But packet loss should now go down
        assert_eq!(conn.packet_loss(), 50.0);

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

