/// Trait for implementation of a network congestion avoidance algorithm.
pub trait RateLimiter {

    /// Method implementing a congestion avoidance algorithm based on round
    /// trip time and packet loss.
    fn update(&mut self, rtt: u32, packet_loss: f32);

    /// Method that should return `true` in case the connection is currently
    /// considered congested and should reduce the number of packets it sends
    /// per second.
    fn congested(&self) -> bool;

    /// Method that returns whether a connection should be currently sending
    /// packets or not.
    fn should_send(&self) -> bool;

    /// Method that resets any internal state of the rate limiter.
    fn reset(&mut self);

}

