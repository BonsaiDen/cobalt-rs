// Copyright (c) 2015-2017 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate rand;


// STD Dependencies -----------------------------------------------------------
use std::cmp;
use std::vec::Drain;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use std::collections::{HashMap, VecDeque};
use std::io::Error;

// Internal Dependencies ------------------------------------------------------
use super::message_queue::MessageQueue;
use ::{Config, MessageKind, PacketModifier, RateLimiter, Socket};

/// Maximum number of acknowledgement bits available in the packet header.
const MAX_ACK_BITS: u32 = 32;

/// Maximum packet sequence number before wrap around happens.
const MAX_SEQ_NUMBER: u32 = 256;

/// Number of bytes used by a packet header.
const PACKET_HEADER_SIZE: usize = 14;

/// Special packet data used to notify of programmtic connection closure.
const CLOSURE_PACKET_DATA: [u8; 6] = [
    0, 128, // Most distant sequence numbers
    85, 85, 85, 85 // ack bitfield with every second bit set
];

/// Enum indicating the state of a `SentPacketAck`.
#[derive(Debug, PartialEq)]
enum PacketState {
    Unknown,
    Acked,
    Lost
}

/// Structure used for packet acknowledgment of sent packets.
#[derive(Debug)]
struct SentPacketAck {
    seq: u32,
    time: Instant,
    state: PacketState,
    packet: Option<Vec<u8>>
}

/// Enum indicating the state of a connection.
#[derive(Debug, Copy, Clone, PartialEq)]
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

    /// The connection is about to be closed.
    Closing,

    /// The connection has been closed programmatically.
    Closed

}

/// Enum of connection related network events.
#[derive(Debug, PartialEq)]
pub enum ConnectionEvent {

    /// Emitted once a the connection has been established.
    Connected,

    /// Emitted when a connection attempt failed.
    FailedToConnect,

    /// Emitted when the already established connection is lost.
    Lost,

    /// Emitted when the already established connection is closed
    /// programmatically.
    Closed(bool),

    /// Emitted for each message that is received over the connection.
    Message(Vec<u8>),

    /// Event emitted for each packet which was not confirmed by the remote end
    /// of the connection within the specified limits.
    PacketLost(Vec<u8>),

    /// Emitted each time the connection's congestion state changes.
    CongestionStateChanged(bool)
}


/// Representation of a random ID for connection identification purposes.
///
/// Used to uniquely\* identify the reliable connections. The ID is send with
/// every packet and allows to support multiple connections behind NAT. It also
/// reduces the chance of connection drop attacks it also and helps with cases
/// where NAT re-assigns local UDP ports which would cause purely address based
/// packet identification mechanisms to break down.
///
/// > \* Since the ID is random integer, there is of course a always a chance
/// > for two connections to end up with the same ID, in that case - due to
/// conflicting ack sequences and message data - both connections will get
/// dropped shortly.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Ord, PartialOrd)]
pub struct ConnectionID(pub u32);


/// Type alias for connection mappings.
pub type ConnectionMap = HashMap<ConnectionID, Connection<RateLimiter, PacketModifier>>;

/// Implementation of a reliable, virtual socket connection.
#[derive(Debug)]
pub struct Connection<R: RateLimiter, M: PacketModifier> {

    /// The connection's configuration
    config: Config,

    /// Random Connection ID
    random_id: ConnectionID,

    /// State of the connection
    state: ConnectionState,

    /// The socket address of the local end of the connection
    local_address: SocketAddr,

    /// The socket address of the remote end of the connection
    peer_address: SocketAddr,

    /// The most recent received remote sequence number
    remote_seq_number: u32,

    /// The current, local sequence number
    local_seq_number: u32,

    /// Exponentially smoothed moving average of the roundtrip time
    smoothed_rtt: f32,

    /// Last time a packet was received
    last_receive_time: Instant,

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
    rate_limiter: R,

    /// The packet modifier used for payload modification
    packet_modifier: M,

    /// List of accumulated connection events
    events: Vec<ConnectionEvent>

}

impl<R: RateLimiter, M: PacketModifier> Connection<R, M> {

    /// Creates a new Virtual Connection over the given `SocketAddr`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::SocketAddr;
    /// use cobalt::{
    ///     BinaryRateLimiter,
    ///     Connection, ConnectionState, Config,
    ///     NoopPacketModifier, PacketModifier, RateLimiter
    /// };
    ///
    /// let config = Config::default();
    /// let local_address: SocketAddr = "127.0.0.1:0".parse().unwrap();
    /// let peer_address: SocketAddr = "255.0.0.1:0".parse().unwrap();
    /// let limiter = BinaryRateLimiter::new(config);
    /// let modifier = NoopPacketModifier::new(config);
    /// let conn = Connection::new(config, local_address, peer_address, limiter, modifier);
    ///
    /// assert!(conn.state() == ConnectionState::Connecting);
    /// assert_eq!(conn.open(), true);
    /// ```
    pub fn new(
        config: Config,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        rate_limiter: R,
        packet_modifier: M

    ) -> Connection<R, M> {
        Connection {
            config: config,
            random_id: ConnectionID(rand::random()),
            state: ConnectionState::Connecting,
            local_address: local_addr,
            peer_address: peer_addr,
            local_seq_number: 0,
            remote_seq_number: 0,
            smoothed_rtt: 0.0,
            last_receive_time: Instant::now(),
            recv_ack_queue: VecDeque::new(),
            sent_ack_queue: Vec::new(),
            sent_packets: 0,
            recv_packets: 0,
            acked_packets: 0,
            lost_packets: 0,
            message_queue: MessageQueue::new(config),
            rate_limiter: rate_limiter,
            packet_modifier: packet_modifier,
            events: Vec::new()
        }
    }

    /// Extracts a `ConnectionID` from packet with a valid protocol header.
    ///
    /// # Examples
    ///
    /// ```
    /// use cobalt::{
    ///     BinaryRateLimiter,
    ///     Connection, ConnectionID, Config,
    ///     NoopPacketModifier
    /// };
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
    /// let conn_id = Connection::<BinaryRateLimiter, NoopPacketModifier>::id_from_packet(&config, &packet);
    ///
    /// assert!(conn_id == Some(ConnectionID(16909060)));
    /// ```
    pub fn id_from_packet(config: &Config, packet: &[u8]) -> Option<ConnectionID> {
        if packet.len() >= 8 && &packet[0..4] == &config.protocol_header {
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
            ConnectionState::Closing |
            ConnectionState::Connecting |
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

    /// Overrides the id of the connection.
    pub fn set_id(&mut self, id: ConnectionID) {
        self.random_id = id;
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

    /// Returns the socket address for the local end of this connection.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_address
    }

    /// Returns the socket address for the remote end of this connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_address
    }

    /// Sets the socket address of the remote peer of this connection.
    pub fn set_peer_addr(&mut self, peer_addr: SocketAddr) {
        self.peer_address = peer_addr;
    }

    /// Overrides the connection's existing configuration.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.message_queue.set_config(config);
    }

    /// Sends a message of the specified `kind` along with its `payload` over
    /// the connection.
    ///
    /// How exactly the message is send and whether it is guaranteed to be
    /// delivered eventually is determined by its `MessageKind`.
    pub fn send(&mut self, kind: MessageKind, payload: Vec<u8>) {
        self.message_queue.send(kind, payload);
    }

    /// Returns a drain iterator over all queued events from this connection.
    pub fn events(&mut self) -> Drain<ConnectionEvent> {

        // We only fetch messages from the queue "on demand" they will otherwise
        // get dismissed once send_packet is called.
        for message in self.message_queue.received() {
            self.events.push(ConnectionEvent::Message(message));
        }

        self.events.drain(0..)

    }

    /// Receives a incoming UDP packet.
    pub fn receive_packet(&mut self, packet: Vec<u8>) -> bool {

        // Ignore any packets shorter then the header length
        if packet.len() < PACKET_HEADER_SIZE {
            return false;
        }

        // Update connection state
        if !self.update_receive_state(&packet) {
            return false;
        }

        // Update time used for disconnect detection
        self.last_receive_time = Instant::now();

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

                let ack = &mut self.sent_ack_queue[i];

                // Calculate the roundtrip time from acknowledged packets
                if seq_was_acked(ack.seq, ack_seq_number, bitfield) {

                    let tick_delay = Duration::from_millis(
                        1000 / self.config.send_rate
                    );

                    self.acked_packets = self.acked_packets.wrapping_add(1);
                    self.smoothed_rtt = moving_average(
                        self.smoothed_rtt,
                        (cmp::max(self.last_receive_time - ack.time, tick_delay) - tick_delay)
                    );

                    ack.state = PacketState::Acked;

                    None

                // Extract data from lost packets
                } else if self.last_receive_time - ack.time
                        > self.config.packet_drop_threshold {

                    self.lost_packets = self.lost_packets.wrapping_add(1);
                    ack.state = PacketState::Lost;
                    ack.packet.take()

                // Keep all pending packets
                } else {
                    None
                }

            } {

                // Push messages from lost packets into the queue
                self.message_queue.lost_packet(&lost_packet[PACKET_HEADER_SIZE..]);

                // Packet lost notification
                self.events.push(ConnectionEvent::PacketLost(
                    lost_packet[PACKET_HEADER_SIZE..].to_vec()
                ));

            }

        }

        // Push packet data into message queue
        if let Some(payload) = self.packet_modifier.incoming(
            &packet[PACKET_HEADER_SIZE..]
        ) {
            self.message_queue.receive_packet(&payload[..]);

        } else {
            self.message_queue.receive_packet(&packet[PACKET_HEADER_SIZE..]);
        }

        // Remove all acknowledged and lost packets from the sent ack queue
        self.sent_ack_queue.retain(|p| p.state == PacketState::Unknown);

        // Insert packet into receive acknowledgment queue
        self.recv_ack_queue.push_front(self.remote_seq_number);

        // Don't keep more entries than we can actually acknowledge per packet
        if self.recv_ack_queue.len() > MAX_ACK_BITS as usize {
            self.recv_ack_queue.pop_back();
        }

        // Update packet statistics
        self.recv_packets = self.recv_packets.wrapping_add(1);

        true

    }

    /// Send a new outgoing UDP packet.
    pub fn send_packet<S: Socket>(
        &mut self,
        socket: &mut S,
        addr: &SocketAddr

    ) -> Result<u32, Error> {

        // Update connection state
        if !self.update_send_state() {
            return Ok(0);
        }

        let congested = self.rate_limiter.congested();
        let rtt = self.rtt();
        let packet_loss = self.packet_loss();

        // Update congestion state
        self.rate_limiter.update(rtt, packet_loss);

        // Check if the state changed and invoke handler
        if congested != self.rate_limiter.congested() {
            self.events.push(ConnectionEvent::CongestionStateChanged(!congested));
        }

        // Check if we should be sending packets, if not skip this packet
        if !self.rate_limiter.should_send() {
            return Ok(0);
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

        // Send closing packets if required
        if self.state == ConnectionState::Closing {
            packet.extend_from_slice(&CLOSURE_PACKET_DATA);

        } else {

            // Set local sequence number
            packet.push(self.local_seq_number as u8);

            // Set packet ack number
            packet.push(self.remote_seq_number as u8);

            // Construct ack bitfield from most recently received packets
            let mut bitfield: u32 = 0;
            for seq in &self.recv_ack_queue {

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

        }

        // Send packet to socket
        let bytes_sent = if let Some(mut payload) = self.packet_modifier.outgoing(
            &packet[PACKET_HEADER_SIZE..]
        ) {

            // Combine existing header with modified packet payload
            let mut packet = packet[..PACKET_HEADER_SIZE].to_vec();
            packet.append(&mut payload);

            socket.send_to(
                &packet[..], *addr

            )?;

            // Number of all bytes sent
            packet.len()

        } else {
            socket.send_to(
                &packet[..], *addr

            )?;

            // Number of all bytes sent
            packet.len()
        };


        // Insert packet into send acknowledgment queue (but avoid dupes)
        if self.send_ack_required(self.local_seq_number) {
            self.sent_ack_queue.push(SentPacketAck {
                seq: self.local_seq_number,
                time: Instant::now(),
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
        self.sent_packets = self.sent_packets.wrapping_add(1);

        // Dismiss any pending, received messages
        self.message_queue.dismiss();

        // Return number of bytes sent over the socket
        Ok(bytes_sent as u32)

    }

    /// Resets the connection for re-use with another address.
    pub fn reset(&mut self) {
        self.state = ConnectionState::Connecting;
        self.local_seq_number = 0;
        self.remote_seq_number = 0;
        self.smoothed_rtt = 0.0;
        self.last_receive_time = Instant::now();
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
        self.state = ConnectionState::Closing;
    }


    // Internal State Handling ------------------------------------------------

    fn update_receive_state(&mut self, packet: &[u8]) -> bool {

        // Ignore any packets which do not match the desired protocol header
        &packet[0..4] == &self.config.protocol_header && match self.state {

            ConnectionState::Lost |
            ConnectionState::Closed |
            ConnectionState::FailedToConnect => false,

            ConnectionState::Closing => true,

            ConnectionState::Connecting => {

                // Once we receive the first valid packet we consider the
                // connection as established
                self.state = ConnectionState::Connected;

                // Reset Packet Loss upon connection
                self.lost_packets = 0;

                self.events.push(ConnectionEvent::Connected);

                true

            },

            ConnectionState::Connected => {

                // Check for closure packet from remote
                if &packet[8..14] == &CLOSURE_PACKET_DATA {
                    self.state = ConnectionState::Closed;
                    self.events.push(ConnectionEvent::Closed(true));
                    false

                } else {
                    // Check if the packet sequence number is more recent,
                    // otherwise drop it as a duplicate
                    seq_is_more_recent(
                        packet[8] as u32, self.remote_seq_number
                    )
                }

            }

        }

    }

    fn update_send_state(&mut self) -> bool {

        // Calculate time since last received packet
        let inactive_time = self.last_receive_time.elapsed();

        match self.state {

            ConnectionState::Lost |
            ConnectionState::Closed |
            ConnectionState::FailedToConnect => false,

            ConnectionState::Connecting => {

                // Quickly detect initial connection failures
                if inactive_time > self.config.connection_init_threshold {
                    self.state = ConnectionState::FailedToConnect;
                    self.events.push(ConnectionEvent::FailedToConnect);
                    false

                } else {
                    true
                }

            },

            ConnectionState::Connected => {

                // Detect connection timeouts
                if inactive_time > self.config.connection_drop_threshold {
                    self.state = ConnectionState::Lost;
                    self.events.push(ConnectionEvent::Lost);
                    false

                } else {
                    true
                }

            },

            ConnectionState::Closing => {

                // Detect connection closure
                if inactive_time > self.config.connection_closing_threshold {
                    self.state = ConnectionState::Closed;
                    self.events.push(ConnectionEvent::Closed(false));
                    false

                } else {
                    true
                }

            }

        }

    }

    // Internal Helpers -------------------------------------------------------
    fn send_ack_required(&self, seq: u32) -> bool {
        !self.sent_ack_queue.iter().any(|p| p.seq == seq)
    }

}


// Static Helpers -------------------------------------------------------------
fn moving_average(a: f32, b: Duration) -> f32 {
    let b = (b.as_secs() as f32) * 1000.0 + (b.subsec_nanos() / 1000000) as f32;
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
    (a > b) && (a - b <= MAX_SEQ_NUMBER / 2) ||
    (b > a) && (b - a >  MAX_SEQ_NUMBER / 2)
}

fn seq_was_acked(seq: u32, ack: u32, bitfield: u32) -> bool {
    if seq == ack {
        true

    } else {
        let bit = seq_bit_index(seq, ack);
        bit < MAX_ACK_BITS && (bitfield & (1 << bit)) != 0
    }
}
