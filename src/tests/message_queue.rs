// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use super::super::Config;
use super::super::shared::message_queue::{MessageKind, MessageQueue};

#[test]
fn test_send_write() {

    let mut q = MessageQueue::new(Config::default());

    // Filled from quota
    q.send(MessageKind::Instant, b"Hello World".to_vec());
    q.send(MessageKind::Instant, b"Hello World".to_vec());

    // Added by filling buffer
    q.send(MessageKind::Instant, b"Hello World".to_vec());

    // Put into packet 2
    q.send(MessageKind::Instant, b"Hello World2".to_vec());
    q.send(MessageKind::Instant, b"Hello World2".to_vec());

    // Filled from quota
    q.send(MessageKind::Reliable, b"Foo".to_vec());

    // Put into packet 2 by quota
    q.send(MessageKind::Reliable, b"Foo2".to_vec());

    // Put into packet 2 by filling buffer
    q.send(MessageKind::Reliable, b"Foo More".to_vec());

    // Filled from quota
    q.send(MessageKind::Ordered, b"Bar".to_vec());

    // Put into packet 2 by quota
    q.send(MessageKind::Ordered, b"Bar2".to_vec());

    // Put into packet 3
    q.send(MessageKind::Ordered, b"Bar More".to_vec());
    q.send(MessageKind::Ordered, b"Bar Even More".to_vec());

    // Check Packet 1
    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 60);

    assert_eq!(buffer, [
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100,
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100,
        // Foo
        1, 0, 0, 3, 70, 111, 111,
        // Bar
        2, 0, 0, 3, 66, 97, 114,
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100

    ].to_vec());

    // Check Packet 2
    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 64);

    assert_eq!(buffer, [
        // Hello World2
        0, 0, 0, 12, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 50,
        // Hello World2
        0, 0, 0, 12, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 50,
        // Foo2
        1, 0, 0, 4, 70, 111, 111, 50,
        // Bar2
        2, 1, 0, 4, 66, 97, 114, 50,
        // Foo More
        1, 0, 0, 8, 70, 111, 111, 32, 77, 111, 114, 101

    ].to_vec());

    // Check Packet 3
    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 64);

    assert_eq!(buffer, [
        // Bar More
        2, 2, 0, 8, 66, 97, 114, 32, 77, 111, 114, 101,

        // Bar Even More
        2, 3, 0, 13, 66, 97, 114, 32, 69, 118, 101, 110, 32, 77, 111, 114, 101
    ].to_vec());

}

#[test]
fn test_send_write_long() {

    let msg = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
        eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco \
        laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure \
        dolor in reprehenderit in voluptate velit esse cillum dolore eu \
        fugiat nulla pariatur. Excepteur sint occaecat cupidatat non \
        proident, sunt in culpa qui officia deserunt mollit anim id est \
        laborum";

    let mut q = MessageQueue::new(Config::default());
    q.send(MessageKind::Instant, msg.to_vec());

    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 1400);

    assert_eq!(buffer, [
        0, 0, 1, 188, 76, 111, 114, 101, 109, 32, 105, 112, 115, 117, 109,
        32, 100, 111, 108, 111, 114, 32, 115, 105, 116, 32, 97, 109, 101,
        116, 44, 32, 99, 111, 110, 115, 101, 99, 116, 101, 116, 117, 114,
        32, 97, 100, 105, 112, 105, 115, 99, 105, 110, 103, 32, 101, 108,
        105, 116, 44, 32, 115, 101, 100, 32, 100, 111, 32, 101, 105, 117,
        115, 109, 111, 100, 32, 116, 101, 109, 112, 111, 114, 32, 105, 110,
        99, 105, 100, 105, 100, 117, 110, 116, 32, 117, 116, 32, 108, 97,
        98, 111, 114, 101, 32, 101, 116, 32, 100, 111, 108, 111, 114, 101,
        32, 109, 97, 103, 110, 97, 32, 97, 108, 105, 113, 117, 97, 46, 32,
        85, 116, 32, 101, 110, 105, 109, 32, 97, 100, 32, 109, 105, 110,
        105, 109, 32, 118, 101, 110, 105, 97, 109, 44, 32, 113, 117, 105,
        115, 32, 110, 111, 115, 116, 114, 117, 100, 32, 101, 120, 101, 114,
        99, 105, 116, 97, 116, 105, 111, 110, 32, 117, 108, 108, 97, 109,
        99, 111, 32, 108, 97, 98, 111, 114, 105, 115, 32, 110, 105, 115,
        105, 32, 117, 116, 32, 97, 108, 105, 113, 117, 105, 112, 32, 101,
        120, 32, 101, 97, 32, 99, 111, 109, 109, 111, 100, 111, 32, 99,
        111, 110, 115, 101, 113, 117, 97, 116, 46, 32, 68, 117, 105, 115,
        32, 97, 117, 116, 101, 32, 105, 114, 117, 114, 101, 32, 100, 111,
        108, 111, 114, 32, 105, 110, 32, 114, 101, 112, 114, 101, 104, 101,
        110, 100, 101, 114, 105, 116, 32, 105, 110, 32, 118, 111, 108, 117,
        112, 116, 97, 116, 101, 32, 118, 101, 108, 105, 116, 32, 101, 115,
        115, 101, 32, 99, 105, 108, 108, 117, 109, 32, 100, 111, 108, 111,
        114, 101, 32, 101, 117, 32, 102, 117, 103, 105, 97, 116, 32, 110,
        117, 108, 108, 97, 32, 112, 97, 114, 105, 97, 116, 117, 114, 46,
        32, 69, 120, 99, 101, 112, 116, 101, 117, 114, 32, 115, 105, 110,
        116, 32, 111, 99, 99, 97, 101, 99, 97, 116, 32, 99, 117, 112, 105,
        100, 97, 116, 97, 116, 32, 110, 111, 110, 32, 112, 114, 111, 105,
        100, 101, 110, 116, 44, 32, 115, 117, 110, 116, 32, 105, 110, 32,
        99, 117, 108, 112, 97, 32, 113, 117, 105, 32, 111, 102, 102, 105,
        99, 105, 97, 32, 100, 101, 115, 101, 114, 117, 110, 116, 32, 109,
        111, 108, 108, 105, 116, 32, 97, 110, 105, 109, 32, 105, 100, 32,
        101, 115, 116, 32, 108, 97, 98, 111, 114, 117, 109
    ].to_vec());

}

#[test]
fn test_packet_lost_write() {

    let mut q = MessageQueue::new(Config::default());

    q.lost_packet(&[
        // Hello World2
        0, 0, 0, 12, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 50,
        // Hello World2
        0, 0, 0, 12, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 50,
        // Foo2
        1, 0, 0, 4, 70, 111, 111, 50,
        // Bar2
        2, 1, 0, 4, 66, 97, 114, 50,
        // Foo More
        1, 0, 0, 8, 70, 111, 111, 32, 77, 111, 114, 101
    ]);

    // Send some more messages
    q.send(MessageKind::Instant, b"Hello World".to_vec());
    q.send(MessageKind::Reliable, b"Foo5".to_vec());
    q.send(MessageKind::Ordered, b"Bar3".to_vec());

    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 64);
    assert_eq!(buffer, [

        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100,

        // Foo More
        1, 0, 0, 8, 70, 111, 111, 32, 77, 111, 114, 101,

        // Bar2
        2, 1, 0, 4, 66, 97, 114, 50,

        // Foo2
        1, 0, 0, 4, 70, 111, 111, 50,

        // Bar3
        2, 0, 0, 4, 66, 97, 114, 51,

        // Foo5
        1, 0, 0, 4, 70, 111, 111, 53

    ].to_vec());

}

#[test]
fn test_receive_read() {

    let mut q = MessageQueue::new(Config::default());
    let packet = [
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100,
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100,
        // Foo
        1, 0, 0, 3, 70, 111, 111,
        // Bar
        2, 0, 0, 3, 66, 97, 114,
        // Hello World
        0, 0, 0, 11, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100
    ].to_vec();

    q.receive_packet(&packet[..]);

    assert_eq!(messages(&mut q), [
        b"Hello World".to_vec(),
        b"Hello World".to_vec(),
        b"Foo".to_vec(),
        b"Bar".to_vec(),
        b"Hello World".to_vec()
    ]);

}

#[test]
fn test_receive_read_long() {

    let mut q = MessageQueue::new(Config::default());
    let packet = [
        0, 0, 1, 188, 76, 111, 114, 101, 109, 32, 105, 112, 115, 117, 109,
        32, 100, 111, 108, 111, 114, 32, 115, 105, 116, 32, 97, 109, 101,
        116, 44, 32, 99, 111, 110, 115, 101, 99, 116, 101, 116, 117, 114,
        32, 97, 100, 105, 112, 105, 115, 99, 105, 110, 103, 32, 101, 108,
        105, 116, 44, 32, 115, 101, 100, 32, 100, 111, 32, 101, 105, 117,
        115, 109, 111, 100, 32, 116, 101, 109, 112, 111, 114, 32, 105, 110,
        99, 105, 100, 105, 100, 117, 110, 116, 32, 117, 116, 32, 108, 97,
        98, 111, 114, 101, 32, 101, 116, 32, 100, 111, 108, 111, 114, 101,
        32, 109, 97, 103, 110, 97, 32, 97, 108, 105, 113, 117, 97, 46, 32,
        85, 116, 32, 101, 110, 105, 109, 32, 97, 100, 32, 109, 105, 110,
        105, 109, 32, 118, 101, 110, 105, 97, 109, 44, 32, 113, 117, 105,
        115, 32, 110, 111, 115, 116, 114, 117, 100, 32, 101, 120, 101, 114,
        99, 105, 116, 97, 116, 105, 111, 110, 32, 117, 108, 108, 97, 109,
        99, 111, 32, 108, 97, 98, 111, 114, 105, 115, 32, 110, 105, 115,
        105, 32, 117, 116, 32, 97, 108, 105, 113, 117, 105, 112, 32, 101,
        120, 32, 101, 97, 32, 99, 111, 109, 109, 111, 100, 111, 32, 99,
        111, 110, 115, 101, 113, 117, 97, 116, 46, 32, 68, 117, 105, 115,
        32, 97, 117, 116, 101, 32, 105, 114, 117, 114, 101, 32, 100, 111,
        108, 111, 114, 32, 105, 110, 32, 114, 101, 112, 114, 101, 104, 101,
        110, 100, 101, 114, 105, 116, 32, 105, 110, 32, 118, 111, 108, 117,
        112, 116, 97, 116, 101, 32, 118, 101, 108, 105, 116, 32, 101, 115,
        115, 101, 32, 99, 105, 108, 108, 117, 109, 32, 100, 111, 108, 111,
        114, 101, 32, 101, 117, 32, 102, 117, 103, 105, 97, 116, 32, 110,
        117, 108, 108, 97, 32, 112, 97, 114, 105, 97, 116, 117, 114, 46,
        32, 69, 120, 99, 101, 112, 116, 101, 117, 114, 32, 115, 105, 110,
        116, 32, 111, 99, 99, 97, 101, 99, 97, 116, 32, 99, 117, 112, 105,
        100, 97, 116, 97, 116, 32, 110, 111, 110, 32, 112, 114, 111, 105,
        100, 101, 110, 116, 44, 32, 115, 117, 110, 116, 32, 105, 110, 32,
        99, 117, 108, 112, 97, 32, 113, 117, 105, 32, 111, 102, 102, 105,
        99, 105, 97, 32, 100, 101, 115, 101, 114, 117, 110, 116, 32, 109,
        111, 108, 108, 105, 116, 32, 97, 110, 105, 109, 32, 105, 100, 32,
        101, 115, 116, 32, 108, 97, 98, 111, 114, 117, 109
    ].to_vec();

    q.receive_packet(&packet[..]);

    let msg = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
        eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco \
        laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure \
        dolor in reprehenderit in voluptate velit esse cillum dolore eu \
        fugiat nulla pariatur. Excepteur sint occaecat cupidatat non \
        proident, sunt in culpa qui officia deserunt mollit anim id est \
        laborum";

    assert_eq!(messages(&mut q), [
        msg.to_vec(),
    ]);

}

#[test]
fn test_receive_read_out_of_order() {

    let mut q = MessageQueue::new(Config::default());

    // Receive one out of order(#1) "World" message
    q.receive_packet(&[
        2, 1, 0, 5, 87, 111, 114, 108, 100
    ]);

    // We expect no message yet
    assert!(messages(&mut q).is_empty());

    // Receive one out of order(#3) "order!" message
    q.receive_packet(&[
        2, 3, 0, 6, 111, 114, 100, 101, 114, 33
    ]);

    // We still expect no message yet
    assert!(messages(&mut q).is_empty());

    // Receive the actual first "Hello" message
    q.receive_packet(&[
        2, 0, 0, 5, 72, 101, 108, 108, 111
    ]);

    // We now expect both "Hello" and "World"
    assert_eq!(messages(&mut q), [b"Hello", b"World"]);

    // Receive the order(#2) "out of" message
    q.receive_packet(&[
        2, 2, 0, 6, 111, 117, 116, 32, 111, 102
    ]);

    // We now expect both "out of" and "order!"
    assert_eq!(messages(&mut q), [b"out of", b"order!"]);
}

#[test]
fn test_receive_empty() {

    let mut q = MessageQueue::new(Config::default());

    // Receive 2 empty messages
    q.receive_packet(&[
        0, 0, 0, 0,
        0, 0, 0, 0
    ]);

    assert_eq!(messages(&mut q), [b"", b""]);

}

#[test]
fn test_receive_invalid() {

    let mut q = MessageQueue::new(Config::default());

    // Receive a message with a invalid kind
    q.receive_packet(&[
        255, 0, 0, 0
    ]);

    assert!(messages(&mut q).is_empty());

    // Receive a message with incomplete header
    q.receive_packet(&[
        0, 0
    ]);

    q.receive_packet(&[
        0, 0, 0
    ]);

    // Receive a message with incomplete data
    q.receive_packet(&[
        0, 0, 0, 15, 72, 101, 108, 108, 111 // 15 bytes but only 5 in buffer
    ]);

    assert_eq!(messages(&mut q), [b"Hello"]);

}

#[test]
fn test_receive_ordered_decoding_wrap_around() {

    let mut q = MessageQueue::new(Config::default());
    for i in 0..4096 {

        q.receive_packet(&[
            2 | ((i & 0x0F00) >> 4) as u8, (i as u8), 0, 2, (i >> 8) as u8, i as u8
        ]);

        assert_eq!(messages(&mut q), [[(i >> 8) as u8, i as u8]]);

    }

    // Should now expect order=0 again
    q.receive_packet(&[
        2, 0, 0, 2, 0, 0
    ]);
    assert_eq!(messages(&mut q), [[0, 0]]);

}

#[test]
fn test_receive_ordered_encoding_wrap_around() {

    let mut q = MessageQueue::new(Config::default());
    for i in 0..4096 {

        q.send(MessageKind::Ordered, [(i >> 8) as u8, i as u8].to_vec());

        let mut buffer = Vec::new();
        q.send_packet(&mut buffer, 64);
        assert_eq!(buffer, [
            2 | ((i & 0x0F00) >> 4) as u8, (i as u8), 0, 2, (i >> 8) as u8, i as u8].to_vec()
        );

    }

    // Should now write order=0 again
    q.send(MessageKind::Ordered, [0, 0].to_vec());

    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 64);
    assert_eq!(buffer, [2, 0, 0, 2, 0, 0].to_vec());

}

#[test]
fn test_reset() {

    let mut q = MessageQueue::new(Config::default());
    q.send(MessageKind::Instant, b"Hello World".to_vec());
    q.send(MessageKind::Instant, b"Hello World".to_vec());
    q.send(MessageKind::Reliable, b"Hello World".to_vec());
    q.send(MessageKind::Ordered, b"Hello World".to_vec());
    q.send(MessageKind::Ordered, b"Hello World".to_vec());

    // Reset all queues and order ids
    q.reset();

    // Check that nothing gets serialized
    let mut buffer = Vec::new();
    q.send_packet(&mut buffer, 64);
    assert_eq!(buffer, [].to_vec());

    // Check that local_order_id has been reset
    q.send(MessageKind::Ordered, b"".to_vec());
    q.send_packet(&mut buffer, 64);
    assert_eq!(buffer, [2, 0, 0, 0].to_vec());
}

#[test]
fn test_in_order_duplicates() {

    let mut q = MessageQueue::new(Config::default());

    q.receive_packet(&[
        2, 0, 0, 1, 53, // Expected #1
        2, 1, 0, 1, 54, // Expected #2
        2, 1, 0, 1, 55,
        2, 1, 0, 1, 56,
        2, 2, 0, 1, 57, // Expected #3
        2, 2, 0, 1, 58,
        2, 3, 0, 1, 59  // Expected #4
    ]);

    assert_eq!(messages(&mut q), [[53], [54], [57], [59]]);

}

#[test]
fn test_out_of_order_duplicates() {

    let mut q = MessageQueue::new(Config::default());

    q.receive_packet(&[
        2, 0, 0, 1, 53, // Expected #1
        2, 2, 0, 1, 54, // Expected #3
        2, 2, 0, 1, 55,
        2, 2, 0, 1, 56,
        2, 1, 0, 1, 57, // Expected #2
        2, 4, 0, 1, 58, // Expected #5
        2, 3, 0, 1, 59  // Expected #4
    ]);

    assert_eq!(messages(&mut q), [[53], [57], [54], [59], [58]]);

}

// Helpers --------------------------------------------------------------------
fn messages(q: &mut MessageQueue) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    for m in q.received() {
        messages.push(m);
    }
    messages
}

