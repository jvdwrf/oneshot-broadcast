[![Crates.io](https://img.shields.io/crates/v/oneshot-broadcast)](https://crates.io/crates/oneshot-broadcast)
[![Documentation](https://docs.rs/oneshot-broadcast/badge.svg)](https://docs.rs/oneshot-broadcast)

# oneshot-broadcast
A oneshot-broadcast channel that broadcasts a value once without cloning the message.

# Example
```rust
#[tokio::main]
async fn main() {
    // Create the oneshot-broadcast channel
    let (mut sender, mut receiver) = oneshot_broadcast::channel::<u32>();

    // Send a message through the sender
    sender.send(10);

    // We can freely clone the receiver
    let mut receiver2 = receiver.clone();

    // and receive the message asynchronously.
    assert_eq!(receiver.recv().await.unwrap(), &10);
    assert_eq!(receiver2.recv().await.unwrap(), &10);

    // Or, if we're sure that it's ready:
    assert_eq!(receiver.get().unwrap().unwrap(), &10);

    // As an extra feature, we can erase the receiver-type by
    // turning it into a listener:
    let listener /* : Listener */ = receiver.into_listener();
    assert_eq!(listener.await, Ok(()));
}

```