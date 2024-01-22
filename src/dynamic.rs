

// use crate::{channel::Channel, Listener, RecvError, RecvFut};
// use futures::{executor::block_on, Future, FutureExt};
// use std::{
//     pin::Pin,
//     sync::Arc,
//     task::{Context, Poll},
// };

// pub struct DynReceiver<T> {
//     receiver: Arc<dyn DynChannel<T>>,
//     pos: Option<usize>,
// }

// impl<T> DynReceiver<T> {
//     pub fn new(receiver: Arc<dyn DynChannel<T>>, pos: Option<usize>) -> Self {
//         Self { receiver, pos }
//     }

//     pub fn recv(&mut self) -> DynRecvFut<'_, T> {
//         DynRecvFut {
//             channel: &*self.receiver,
//             pos: &mut self.pos,
//         }
//     }

//     pub fn wait(&mut self) -> Result<(), RecvError> {
//         block_on(self)
//     }

//     pub fn close(&self) -> bool {
//         self.receiver.close()
//     }

//     pub fn ready(&self) -> bool {
//         self.receiver.ready()
//     }

//     pub fn get(&self) -> Option<Result<&T, RecvError>> {
//         self.receiver.get()
//     }

//     pub fn into_listener(self) -> Listener {
//         self.receiver.into_listener(self.pos)
//     }
// }

// pub struct DynRecvFut<'a, T> {
//     channel: &'a dyn DynChannel<T>,
//     pos: &'a mut Option<usize>,
// }

// impl<'a, T> Future for DynRecvFut<'a, T> {
//     type Output = Result<&'a T, RecvError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let val: Poll<Result<&'_ T, RecvError>> = self.channel.poll_recv(cx, self.pos);
//         let val: Poll<Result<&'a T, RecvError>> = unsafe { std::mem::transmute(val) };
//         val
//     }
// }

// impl<T> Unpin for DynReceiver<T> {}

// impl<T> Future for DynReceiver<T> {
//     type Output = Result<(), RecvError>;

//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         let this = &mut *self;
//         this.receiver.poll_wait(cx, &mut this.pos)
//     }
// }

// pub trait DynChannel<T>: Send + Unpin {
//     fn poll_recv<'a>(
//         &'a self,
//         cx: &mut Context<'_>,
//         pos: &'a mut Option<usize>,
//     ) -> Poll<Result<&'a T, RecvError>>;
//     fn poll_wait(
//         &self,
//         cx: &mut Context<'_>,
//         pos: &mut Option<usize>,
//     ) -> Poll<Result<(), RecvError>>;
//     fn close(&self) -> bool;
//     fn ready(&self) -> bool;
//     fn get(&self) -> Option<Result<&T, RecvError>>;
//     fn into_listener(self: Arc<Self>, pos: Option<usize>) -> Listener;
// }

// impl<T: Send + Sync + 'static> DynChannel<T> for Channel<T> {
//     fn poll_recv<'a>(
//         &'a self,
//         cx: &mut Context<'_>,
//         pos: &'a mut Option<usize>,
//     ) -> Poll<Result<&'a T, RecvError>> {
//         RecvFut { channel: self, pos }.poll_unpin(cx)
//     }

//     fn poll_wait(
//         &self,
//         cx: &mut Context<'_>,
//         pos: &mut Option<usize>,
//     ) -> Poll<Result<(), RecvError>> {
//         self.inner().exit_fut(pos).poll_unpin(cx)
//     }

//     fn close(&self) -> bool {
//         self.close()
//     }

//     fn ready(&self) -> bool {
//         self.ready()
//     }

//     fn get(&self) -> Option<Result<&T, RecvError>> {
//         self.get()
//     }

//     fn into_listener(self: Arc<Self>, pos: Option<usize>) -> Listener {
//         Listener::new(self, pos)
//     }
// }
