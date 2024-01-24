[![Crates.io](https://img.shields.io/crates/v/oneshot-broadcast)](https://crates.io/crates/oneshot-broadcast)
[![Documentation](https://docs.rs/oneshot-broadcast/badge.svg)](https://docs.rs/oneshot-broadcast)

# oneshot-broadcast
A oneshot-broadcast channel that broadcasts a value once without cloning the message.

# Example
```rust
// #[tokio::main]
// async fn main() {
//     // Create the oneshot-broadcast channel
//     let channel = oneshot_broadcast::Channel::<u32>::new();

//     // Send a message
//     channel.send(10);

//     // We can freely clone the channel
//     let mut channel2 = channel.clone();

//     // and receive the message asynchronously.
//     assert_eq!(receiver.recv().await.unwrap(), 10);
//     assert_eq!(receiver2.await.unwrap(), 10);

//     // Or, if we're sure that it's ready:
//     assert_eq!(receiver.get().unwrap().unwrap(), 10);

//     // As an extra feature, we can erase the receiver-type by
//     // turning it into a listener:
//     let listener /* : Listener */ = channel.into_any();
//     assert_eq!(listener.await, Ok(()));
// }

```