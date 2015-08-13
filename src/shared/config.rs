/// A generic configuration object.
#[derive(Copy, Clone)]
pub struct Config {

    /// Number of packets send per second. Default is `30`.
    pub send_rate: u32,

    /// Maximum bytes that can be received / send in one packet. Default
    /// `1400`.
    pub packet_max_size: usize,

    /// 32-Bit Protocol ID used to identify UDP related packets. Default is
    /// `[1, 2, 3, 4]`.
    pub protocol_header: [u8; 4],

    /// Maximum RTT in milliseconds before a packet is considered lost. Default
    /// is `1000`.
    pub packet_drop_threshold: u32,

    /// Maximum time in milliseconds until the first packet must be received
    /// before a connection attempt fails. Default is `100`.
    pub connection_init_threshold: u32,

    /// Maximum time in milliseconds between any two packets before the
    /// connection gets dropped. Default is `1000`.
    pub connection_drop_threshold: u32,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Instant` into a packet via a `MessageQueue`.
    pub message_quota_instant: f32,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Reliable` into a packet via a `MessageQueue`.
    pub message_quota_reliable: f32,

    /// The percent of available packet bytes to use when serializing
    /// `MessageKind::Ordered` into a packet via a `MessageQueue`.
    pub message_quota_ordered: f32

}

impl Default for Config {

    fn default() -> Config {
        Config {
            send_rate: 30,
            protocol_header: [1, 2, 3, 4],
            packet_max_size: 1400,
            packet_drop_threshold: 1000,
            connection_init_threshold: 100,
            connection_drop_threshold: 1000,
            message_quota_instant: 60.0,
            message_quota_reliable: 20.0,
            message_quota_ordered: 20.0
        }
    }

}

