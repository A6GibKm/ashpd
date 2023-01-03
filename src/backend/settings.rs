use std::{cell::RefCell, collections::HashMap};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

use crate::{backend::IMPL_PATH, desktop::settings::Namespace, zvariant::OwnedValue};

#[async_trait]
pub trait SettingsImpl {
    async fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, Namespace>;

    async fn read(&self, namespace: &str, key: &str) -> OwnedValue;
}

pub struct Settings<T: SettingsImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: T,
}

unsafe impl<T: Send + SettingsImpl> Send for Settings<T> {}
unsafe impl<T: Sync + SettingsImpl> Sync for Settings<T> {}

impl<T: SettingsImpl> Settings<T> {
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
        let iface = SettingsInterface::new(sender);
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

        match response {
            Some(Action::ReadAll(namespaces, sender)) => {
                let results = self.imp.read_all(namespaces).await;
                let _ = sender.send(results);
            }
            Some(Action::Read(namespace, key, sender)) => {
                let results = self.imp.read(&namespace, &key).await;
                let _ = sender.send(results);
            }
            None => (),
        }

        Ok(())
    }
}

enum Action {
    ReadAll(Vec<String>, oneshot::Sender<HashMap<String, Namespace>>),
    Read(String, String, oneshot::Sender<OwnedValue>),
}

struct SettingsInterface {
    sender: Sender<Action>,
}

impl SettingsInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self { sender }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsInterface {
    async fn read_all(&mut self, namespaces: Vec<String>) -> HashMap<String, Namespace> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::ReadAll(namespaces, sender));
        let mut stream = receiver.into_stream();

        stream.next().await.unwrap().unwrap()
    }

    async fn read(&mut self, namespace: String, key: String) -> OwnedValue {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::Read(namespace, key, sender));
        let mut stream = receiver.into_stream();

        stream.next().await.unwrap().unwrap()
    }
}
