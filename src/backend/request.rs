use std::{cell::RefCell, sync::Arc};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::{dbus_interface, zvariant::OwnedObjectPath};

#[async_trait]
pub trait RequestImpl {
    async fn close(&self);
}

pub(crate) struct Request<T: RequestImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: Arc<T>,
}

unsafe impl<T: Send + RequestImpl> Send for Request<T> {}
unsafe impl<T: Sync + RequestImpl> Sync for Request<T> {}

impl<T: RequestImpl> Request<T> {
    pub async fn new(
        imp: Arc<T>,
        handle_path: OwnedObjectPath,
        cnx: &zbus::Connection,
    ) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = RequestInterface::new(sender, handle_path.clone());
        let object_server = cnx.object_server();

        #[cfg(feature = "tracing")]
        tracing::debug!("Handling object {:?}", handle_path.as_str());
        object_server.at(handle_path, iface).await?;
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
    handle_path: OwnedObjectPath,
}

impl RequestInterface {
    pub fn new(sender: Sender<Action>, handle_path: OwnedObjectPath) -> Self {
        Self {
            sender,
            handle_path,
        }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestInterface {
    async fn close(
        &mut self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::Close(sender));
        let mut stream = receiver.into_stream();
        let next = stream.next().await;
        next.unwrap().unwrap();

        // Drop the request as it served it purpose once closed
        #[cfg(feature = "tracing")]
        tracing::debug!("Releasing object {:?}", self.handle_path.as_str());
        server
            .remove::<Self, &OwnedObjectPath>(&self.handle_path)
            .await?;
        Ok(())
    }
}
