use std::cmp;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use super::super::Config;

/// Maximum message ordering id before wrap around happens.
const MAX_ORDER_ID: u16 = 4096;

/// Number of bytes used in a single message header.
const MESSAGE_HEADER_BYTES: usize = 4;

/// Enum for specification of a message handling algorithm.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MessageKind {
    /// Message that is going be send exactly once and ignored in case its
    /// containing packet is lost. No guarantees are made as for the order in
    /// which a message of this kind is going to be received by a remote queue.
    Instant = 0,

    /// Message that is going to be re-send in case its containing packet is
    /// lost. No guarantees are made as for the order in which a message of
    /// this kind is going to be received by a remote queue.
    Reliable = 1,

    /// Message that is going to be re-send in case its containing packet is
    /// lost and is also guaranteed to arrive in-order, meaning that if you send
    /// two `Ordered` messages and the second arrives first in the remote queue
    /// , the remote queue will buffer the second message until the first one
    /// arrives and then make both of them available to the application at
    /// once.
    Ordered = 2,

    /// Invalid message which for some reason could not be parsed correctly
    /// from the available packet data.
    Invalid = 3
}

/// Structure for handling messages inside a `MessageQueue` with support for
/// insertion into a binary min heap for order checking on received messages.
#[derive(Debug, Eq, PartialEq)]
struct Message {
    kind: MessageKind,
    order: u16,
    size: u16,
    data: Vec<u8>
}

impl Ord for Message {
    // Explicitly implement the trait so the queue becomes a min-heap
    // instead of a max-heap.
    fn cmp(&self, other: &Message) -> cmp::Ordering {
        other.order.cmp(&self.order)
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Message) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Consuming iterator over the received messages of a `MessageQueue`.
pub struct MessageIterator<'a> {
    messages: &'a mut VecDeque<Message>
}

impl<'a> Iterator for MessageIterator<'a> {

    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.messages.pop_front() {
            Some(m) => Some(m.data),
            None => None
        }
    }

}

/// Implementation of a queue that manages the sending and receiving of both
/// reliable and unreliable message types and also supports optional in order
/// transmission.
#[derive(Debug)]
pub struct MessageQueue {

    /// The queue's configuration
    config: Config,

    /// The local order id which gets attached to all messages send as kind
    /// `MessageKind::Ordered`
    local_order_id: u16,

    /// The remote order id that is expected for the next incoming message of
    /// kind `MessageKind::Ordered`
    remote_order_id: u16,

    /// Queue of outgoing messages of the kind `MessageKind::Instant`
    i_queue: VecDeque<Message>,

    /// Queue of outgoing messages of the kind `MessageKind::Reliable`
    r_queue: VecDeque<Message>,

    /// Queue of outgoing messages of the kind `MessageKind::Ordered`
    o_queue: VecDeque<Message>,

    /// Ordered queue of incoming messages
    recv_queue: VecDeque<Message>,

    /// Binary Min-Heap to manage incomging, out of order messages
    o_recv_heap: BinaryHeap<Message>,

    /// Set for avoiding duplication of out of order messages
    o_recv_set: HashSet<u16>

}

impl MessageQueue {

    /// Creates a new queue for sending and receiving messages.
    pub fn new(config: Config) -> MessageQueue {
        MessageQueue {
            config: config,
            local_order_id: 0,
            remote_order_id: 0,
            i_queue: VecDeque::new(),
            r_queue: VecDeque::new(),
            o_queue: VecDeque::new(),
            recv_queue: VecDeque::new(),
            o_recv_heap: BinaryHeap::new(),
            o_recv_set: HashSet::new()
        }
    }

    /// Returns a consuming iterator over all received messages in the queue.
    pub fn received(&mut self) -> MessageIterator {
        MessageIterator { messages: &mut self.recv_queue }
    }

    /// Pushes a message of the specified `kind` along with its `data` into the
    /// queue. The message will eventually get serialized via
    /// `MessageQueue::send_packet()`.
    pub fn send(&mut self, kind: MessageKind, data: Vec<u8>) {

        let message = Message {
            kind: kind,
            order: self.local_order_id,
            size: data.len() as u16,
            data: data
        };

        match kind {
            MessageKind::Instant => self.i_queue.push_back(message),
            MessageKind::Reliable => self.r_queue.push_back(message),
            MessageKind::Ordered => {

                self.o_queue.push_back(message);
                self.local_order_id += 1;

                if self.local_order_id == MAX_ORDER_ID {
                    self.local_order_id = 0;
                }

            },
            MessageKind::Invalid => {}
        }

    }

    /// Serializes a number of internally queued messages into the
    /// `available` space within the `packet`.
    ///
    /// The used algorithm first tries to fill the available space with the
    /// desired quotas for each `MessageKind` as defined in the queues
    /// configuration.
    ///
    /// Afterwards the remaining available space is filled by alternating
    /// between the different message kinds until there is finally no more
    /// space left to insert any further messages into the packet.
    ///
    /// For example, if we have `512` bytes available inside the packer and we
    /// specify that 60% of the packet data should be filled with
    /// `MessageKind::Instant` messages, then we will try to fill the
    /// buffer with at most `307` bytes of instant messages, at first.
    ///
    /// Then, after the other quotas have been taken into account, we'll try to
    /// fit more instant messages into the remaining available space within the
    /// packet.
    pub fn send_packet(&mut self, packet: &mut Vec<u8>, available: usize) {

        // First we are trying to fill the packet by using the set quotas
        let mut written = 0;
        write_messages(
            &mut self.i_queue, packet,
            (available as f32 / 100.0 * self.config.message_quota_instant) as usize,
            &mut written
        );

        write_messages(
            &mut self.r_queue, packet,
            (available as f32 / 100.0 * self.config.message_quota_reliable) as usize,
            &mut written
        );

        write_messages(
            &mut self.o_queue, packet,
            (available as f32 / 100.0 * self.config.message_quota_ordered) as usize,
            &mut written
        );

        // After that, we try to fill the remaining packet space by trying to
        // add one message of each kind until no more messages can be fit in
        let mut more = true;
        while more {
            more = false;
            more |= write_message(&mut self.i_queue, packet, available, &mut written);
            more |= write_message(&mut self.r_queue, packet, available, &mut written);
            more |= write_message(&mut self.o_queue, packet, available, &mut written);
        }

    }

    /// Parses the contents of a packet into messages, appending all valid
    /// messages into the internal receive queue.
    pub fn receive_packet(&mut self, packet: &[u8]) {
        for m in messages_from_packet(packet) {
            match m.kind {
                MessageKind::Instant => self.recv_queue.push_back(m),
                MessageKind::Reliable => self.recv_queue.push_back(m),
                MessageKind::Ordered => self.receive_ordered_message(m),
                MessageKind::Invalid => { /* ignore all other messages */ }
            }
        }
    }

    /// Parses the contents of a lost packet into messages, dropping all
    /// messages of the type `MessageKind::Instant` and prepending all
    /// remaining valid messages into the internal send queues for
    /// re-transmission.
    pub fn lost_packet(&mut self, packet: &[u8]) {
        for m in messages_from_packet(packet) {
            match m.kind {
                MessageKind::Instant => { /* ignore lost instant messages */ },
                MessageKind::Reliable => self.r_queue.push_front(m),
                MessageKind::Ordered => self.o_queue.push_front(m),
                MessageKind::Invalid => { /* ignore all other messages */ }
            }
        }
    }

    /// Resets the queue, clearing all its internal structures and order ids.
    pub fn reset(&mut self) {
        self.local_order_id = 0;
        self.remote_order_id = 0;
        self.i_queue.clear();
        self.r_queue.clear();
        self.o_queue.clear();
        self.recv_queue.clear();
        self.o_recv_heap.clear();
        self.o_recv_set.clear();
    }

    // Internal Message Handling ----------------------------------------------

    fn receive_ordered_message(&mut self, m: Message) {

        // Check if the order ID matches the currently expected on
        if m.order == self.remote_order_id {

            // Received the message in order
            self.recv_queue.push_back(m);

            self.remote_order_id += 1;
            if self.remote_order_id == MAX_ORDER_ID {
                self.remote_order_id = 0;
            }

            // Now check our heap for further messages we have received
            // out of order and check if they are next in the expected
            // order
            let mut matches = true;
            while matches {

                // TODO refactor

                // Check if the order id of the minimal item in the heap
                // matches the expected next remote order id
                matches = if let Some(msg) = self.o_recv_heap.peek() {
                    msg.order == self.remote_order_id

                } else {
                    false
                };

                // We found another message, matching the next expected order id
                if matches {

                    // Unset duplication marker
                    self.o_recv_set.remove(&self.remote_order_id);

                    // Remove it from the heap and push it into the recv queue
                    let msg = self.o_recv_heap.pop().unwrap();
                    self.recv_queue.push_back(msg);

                    self.remote_order_id += 1;
                    if self.remote_order_id == MAX_ORDER_ID {
                        self.remote_order_id = 0;
                    }

                }

            }

        // Otherwise check if the message order is more recent and if not, we
        // simply drop it. If it IS more recent, then we have received a future
        // message out of order.
        } else if order_is_more_recent(m.order, self.remote_order_id) {

            // Now, before we insert the message into the min-heap, we check
            // that there's no other message with the same order id in the heap
            // already. Duplicates would require additional peek / pop later on
            // when removing messages from the heap, so we resort to a Set here.
            if self.o_recv_set.contains(&m.order) == false {
                self.o_recv_set.insert(m.order);
                self.o_recv_heap.push(m);
            }

        }

    }

}

// Static Helpers -------------------------------------------------------------
fn order_is_more_recent(a: u16, b: u16) -> bool {
    (a > b) && (a - b <= MAX_ORDER_ID / 2)
    || (b > a) && (b - a > MAX_ORDER_ID / 2)
}

fn messages_from_packet(packet: &[u8]) -> Vec<Message> {

    let available = packet.len();
    let mut index = 0;
    let mut messages = Vec::new();

    // Consume as long as message headers can be present
    while index < available && available - index >= MESSAGE_HEADER_BYTES {

        // Upper 4 bits of kind are bits 9..11 of order
        let order_high = ((packet[index] & 0xF0) as u16) << 4;
        let order_low = packet[index + 1] as u16;

        // Byte 2 is the size
        let size_high = (packet[index + 2] as u16) << 8;
        let size = size_high | packet[index + 3] as u16;

        // Read available data
        messages.push(Message {

            // Lower 4 bits of byte 0 are the MessageKind
            kind: match packet[index] & 0x0F {
                0 => MessageKind::Instant,
                1 => MessageKind::Reliable,
                2 => MessageKind::Ordered,
                _ => MessageKind::Invalid
            },

            order: order_high | order_low,
            size: size,
            data: packet[
                index + MESSAGE_HEADER_BYTES..cmp::min(
                    index + MESSAGE_HEADER_BYTES + size as usize,
                    available
                )
            ].to_vec()

        });

        index += size as usize + MESSAGE_HEADER_BYTES;

    }

    messages

}

fn write_messages(
    queue: &mut VecDeque<Message>,
    packet: &mut Vec<u8>,
    available: usize,
    written: &mut usize
) {
    let mut used = 0;
    while write_message(queue, packet, available, &mut used) {}
    *written += used;
}

fn write_message(
    queue: &mut VecDeque<Message>,
    packet: &mut Vec<u8>,
    available: usize,
    written: &mut usize

) -> bool {

    if queue.is_empty() == false {

        let required = {
            (queue.front().unwrap().size as usize) + MESSAGE_HEADER_BYTES
        };

        // If adding this message would exceed the available bytes, exit
        if required > available - *written {
            false

        // Remove and serialize the message into the packet
        } else {
            let message = queue.pop_front().unwrap();
            packet.push(
                ((message.order & 0x0F00) >> 4) as u8 | (message.kind as u8)
            );
            packet.push(message.order as u8);
            packet.push((message.size >> 8) as u8);
            packet.push(message.size as u8);
            packet.extend(message.data.iter().cloned());
            *written += required;
            true
        }

    } else {
        false
    }

}

