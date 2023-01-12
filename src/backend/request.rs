use async_std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::SinkExt;
use zbus::{dbus_interface, zvariant::OwnedObjectPath};

#[async_trait]
pub trait RequestImpl {
    async fn close(&self);
}

pub(crate) struct Request<T: RequestImpl> {
    receiver: Arc<Mutex<Receiver<Action>>>,
    imp: Arc<T>,
}

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
            receiver: Arc::new(Mutex::new(receiver)),
            imp,
        };

        Ok(provider)
    }

    pub async fn next(&self) -> zbus::fdo::Result<()> {
        let action = self.receiver.try_lock().unwrap().try_next().ok().flatten();
        if let Some(Action::Close(sender)) = action {
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
    sender: Arc<Mutex<Sender<Action>>>,
    handle_path: OwnedObjectPath,
}

impl RequestInterface {
    pub fn new(sender: Sender<Action>, handle_path: OwnedObjectPath) -> Self {
        Self {
            sender: Arc::new(Mutex::new(sender)),
            handle_path,
        }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestInterface {
    async fn close(
        &self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.lock().await.send(Action::Close(sender)).await;
        receiver.await.unwrap();

        // Drop the request as it served it purpose once closed
        #[cfg(feature = "tracing")]
        tracing::debug!("Releasing object {:?}", self.handle_path.as_str());
        server
            .remove::<Self, &OwnedObjectPath>(&self.handle_path)
            .await?;
        Ok(())
    }
}
