use std::cell::RefCell;

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

use crate::backend::IMPL_PATH;

#[async_trait]
pub trait RequestImpl {
    async fn close(&self);
}

pub struct Request<T: RequestImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: T,
}

unsafe impl<T: Send + RequestImpl> Send for Request<T> {}
unsafe impl<T: Sync + RequestImpl> Sync for Request<T> {}

impl<T: RequestImpl> Request<T> {
    pub async fn new<N: TryInto<WellKnownName<'static>>>(
        imp: T,
        cnx: &zbus::Connection,
        proxy: &zbus::fdo::DBusProxy<'_>,
        name: N,
    ) -> zbus::Result<Self>
    where
        zbus::Error: From<<N as TryInto<WellKnownName<'static>>>::Error>,
    {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = RequestInterface::new(sender);
        let object_server = cnx.object_server();

        proxy
            .request_name(
                name.try_into()?,
                zbus::fdo::RequestNameFlags::ReplaceExisting.into(),
            )
            .await?;

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: RefCell::new(Some(receiver)),
            imp,
        };

        Ok(provider)
    }

    pub async fn next(&self) -> zbus::fdo::Result<()> {
        let response = self
            .receiver
            .borrow_mut()
            .as_mut()
            .and_then(|receiver| receiver.try_next().unwrap_or(None));

        if let Some(Action::Close(sender)) = response {
            self.imp.close().await;
            let _ = sender.send(());
        };

        Ok(())
    }
}

enum Action {
    Close(oneshot::Sender<()>),
}

struct RequestInterface {
    sender: Sender<Action>,
}

impl RequestInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self { sender }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestInterface {
    async fn close(&mut self) {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::Close(sender));
        let mut stream = receiver.into_stream();
        let next = stream.next().await;
        next.unwrap().unwrap();
    }
}
