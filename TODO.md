# Code

- Optimize removal of disconnected clients
- Use moving average for packet loss percentage over 1 second?


----------------------------

# Tests

- Replace current MockSocket with the new version from the integration tests

- Handle / Verify cases where Reliable messages get delivered, but the confirmation fails, and the message gets delivered again 
    - Should this be handled at the application layer?
    - Or should we detect the dupe?

