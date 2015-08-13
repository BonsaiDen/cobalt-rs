/// Trait for implementing a connection rate limiting algorithm used for
/// network congestion avoidance.
pub trait RateLimiter {

    /// A method implementing a congestion avoidance algorithm based on round
    /// trip time and packet loss.
    fn update(&mut self, rtt: u32, packet_loss: f32);

    /// A method that should return `true` in case the connection is currently
    /// considered congested and should reduce the number of packets it sends
    /// per second.
    fn congested(&self) -> bool;

    /// A method that returns wether a connection should be currently sending
    /// packets or not.
    fn should_send(&self) -> bool;

    /// A method that resets any internal state of the rate limiter.
    fn reset(&mut self);

}

