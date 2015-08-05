extern crate rand;
extern crate clock_ticks;

use std::cmp;
use std::net::SocketAddr;
use std::collections::VecDeque;
use shared::{Config, Handler, Socket};

/// Maximum number of acknowledgement bits available in the packet header.
pub const MAX_ACK_BITS: u32 = 32;

/// Maximum packet sequence number before wrap around happens.
pub const MAX_SEQ_NUMBER: u32 = 256;

/// Number of bytes used by the protocol header data.
pub const HEADER_BYTES: usize = 14;

/// A value indicating the current state of a connection.
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
#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub struct ConnectionID(pub u32);

/// Implementation of reliable UDP based messaging protocol.
pub struct Connection {

    /// Connection configuration
    config: Config,

    /// Random Connection ID
    id: ConnectionID,

    /// State of the connection
    state: ConnectionState,

    /// The most recent received remote sequence number
    remote_seq_number: u32,

    /// The current, local sequence number
    local_seq_number: u32,

    /// Exponentially smoothed moving average of the roundtrip time
    rtt: f32,

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

    /// Buffer from the data received with the last incoming packet
    recv_buffer: Option<Vec<u8>>,

    /// Buffer for data to be send with the next outgoing packet
    send_buffer: Option<Vec<u8>>,

    /// Sending mode of the connection, based on congestion
    mode: CongestionMode,

    /// Time in milliseconds until we try out `Good` mode when in `Bad`
    /// congestion mode.
    time_until_good_mode: u32,

    /// Last time the congestion mode switched
    last_mode_switch_time: u32

}

impl Connection {

    /// Creates a new Virtual Connection over the given `SocketAddr`.
    pub fn new(config: Config) -> Connection {
        Connection {
            config: config,
            id: ConnectionID(rand::random()),
            state: ConnectionState::Connecting,
            local_seq_number: 0,
            remote_seq_number: 0,
            rtt: 0.0,
            last_receive_time: get_time_ms(),
            recv_ack_queue: VecDeque::new(),
            sent_ack_queue: Vec::new(),
            sent_packets: 0,
            recv_packets: 0,
            acked_packets: 0,
            lost_packets: 0,
            send_buffer: Some(empty_packet()),
            recv_buffer: Some(empty_packet()),
            mode: CongestionMode::Good,
            time_until_good_mode: config.congestion_switch_min_delay,
            last_mode_switch_time: 0
        }
    }

    /// Extracts a connection id from a valid packet header.
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
    pub fn is_open(&self) -> bool {
        match self.state {
            ConnectionState::Connecting => true,
            ConnectionState::Connected => true,
            _ => false
        }
    }

    /// Return whether the connection is currently congested and should be
    /// sending less data.
    pub fn is_congested(&self) -> bool {
        self.mode == CongestionMode::Bad
    }

    /// Returns the id of the connection.
    pub fn get_id(&self) -> ConnectionID {
        self.id
    }

    /// Returns the current state of the connection.
    pub fn get_state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the average roundtrip time for the connection.
    pub fn get_rtt(&self) -> u32 {
        self.rtt.ceil() as u32
    }

    /// Returns the percent of packets that were sent and never acknowledged
    /// over the total number of packets that have been send across the
    /// connection.
    pub fn get_packet_loss(&self) -> f32 {
        100.0 / self.sent_packets as f32 * self.lost_packets as f32
    }

    /// Appends to the data that is going to be send with the next outgoing
    /// packet.
    ///
    /// The number of bytes that will actually be received by the remote end of
    /// the connectio is limited by the remote configuration value of
    /// `cobalt::shared::Config.packet_max_size`.
    pub fn write(&mut self, data: &[u8]) {
        self.send_buffer.as_mut().unwrap().extend(data.iter().cloned());
    }

    /// Returns the data contained by the last received packet.
    ///
    /// This will return at most `cobalt::shared::Config.packet_max_size` bytes.
    pub fn read(&mut self) -> &[u8] {
        &self.recv_buffer.as_ref().unwrap()[HEADER_BYTES..]
    }

    /// Receives a incoming UDP packet.
    pub fn receive<T>(
        &mut self, packet: Vec<u8>, owner: &mut T, handler: &mut Handler<T>
    ) {

        // Update connection state
        if self.update_receive_state(&packet, owner, handler) == false {
            return
        }

        // Update time used for disconnect detection
        self.last_receive_time = get_time_ms();

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
                self.rtt = (self.rtt - (self.rtt - (self.last_receive_time - p.time) as f32) * 0.10).max(0.0);
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

        // Notify handler of lost packets
        for packet in lost_packets {
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

        // Move packet into internal buffer
        self.recv_buffer = Some(packet);

    }

    /// Creates a new outgoing UDP packet.
    pub fn send<T>(
        &mut self,
        socket: &mut Socket, address: &SocketAddr,
        owner: &mut T, handler: &mut Handler<T>
    ) {

        // Update connection state
        if self.update_send_state(owner, handler) == false {
            return;
        }

        // Update congestion state
        self.update_congestion_state(owner, handler);

        // Take write buffer out and insert a fresh, empty one in its place
        let mut packet = self.send_buffer.take().unwrap();
        self.send_buffer = Some(empty_packet());

        // Set packet protocol header
        packet[0] = self.config.protocol_header[0];
        packet[1] = self.config.protocol_header[1];
        packet[2] = self.config.protocol_header[2];
        packet[3] = self.config.protocol_header[3];

        // Set connection ID
        packet[4] = (self.id.0 >> 24) as u8;
        packet[5] = (self.id.0 >> 16) as u8;
        packet[6] = (self.id.0 >> 8) as u8;
        packet[7] = self.id.0 as u8;

        // Set local sequence number
        packet[8] = self.local_seq_number as u8;

        // Set packet ack number
        packet[9] = self.remote_seq_number as u8;

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
        packet[10] = (bitfield >> 24) as u8;
        packet[11] = (bitfield >> 16) as u8;
        packet[12] = (bitfield >> 8) as u8;
        packet[13] = bitfield as u8;

        // Send packet to socket
        socket.send(*address, &packet[..]).ok();

        // Insert packet into send acknowledgment queue (but avoid dupes)
        if self.send_ack_required(self.local_seq_number) == true {
            self.sent_ack_queue.push(SentPacketAck {
                seq: self.local_seq_number,
                time: get_time_ms(),
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
        self.rtt = 0.0;
        self.last_receive_time = get_time_ms();
        self.recv_ack_queue.clear();
        self.sent_ack_queue.clear();
        self.sent_packets = 0;
        self.recv_packets = 0;
        self.acked_packets = 0;
        self.lost_packets = 0;
        self.recv_buffer = Some(empty_packet());
        self.send_buffer = Some(empty_packet());
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
        let inactive_time = get_time_ms() - self.last_receive_time;

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

    fn update_good_congestion<T>(&mut self, owner: &mut T, handler: &mut Handler<T>) {

        if self.get_rtt() <= self.config.congestion_rtt_threshold {

            // The we stay in good mode for a prolonged period of time,
            // we reduce the wait time before trying out good mode in case we
            // entered bad mode
            if self.mode_switch_delay_exceeded() {
                self.time_until_good_mode = cmp::max(
                    self.time_until_good_mode / 2,
                    self.config.congestion_switch_min_delay
                );
                self.last_mode_switch_time = get_time_ms();
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

            self.last_mode_switch_time = get_time_ms();

            handler.connection_congested(owner, self);

        }

    }

    fn update_bad_congestion<T>(&mut self, owner: &mut T, handler: &mut Handler<T>) {

        if self.get_rtt() <= self.config.congestion_rtt_threshold {

            // Switch back to good mode if we stay within the RTT threshold for
            // the required time
            if get_time_ms() - self.last_mode_switch_time
                             > self.time_until_good_mode {

                self.mode = CongestionMode::Good;
                self.last_mode_switch_time = get_time_ms();
                handler.connection_congested(owner, self);
            }

        // If we still got congestion, we further delay the next mode
        // switch until we have at least `switch delay` seconds of
        // good RTT
        } else {
            self.last_mode_switch_time = get_time_ms();
        }

    }


    // Internal Helpers -------------------------------------------------------
    fn send_ack_required(&self, seq: u32) -> bool {
        self.sent_ack_queue.iter().any(|p| p.seq == seq) == false
    }

    fn mode_switch_delay_exceeded(&self) -> bool {
        get_time_ms() - self.last_mode_switch_time
                      > self.config.congestion_switch_wait
    }

}

// Static Helpers -------------------------------------------------------------
fn empty_packet() -> Vec<u8> {
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].to_vec()
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

fn get_time_ms() -> u32 {
    (clock_ticks::precise_time_ns() / 1000000) as u32
}

