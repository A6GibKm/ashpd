use std::{cell::RefCell, collections::HashMap};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

use crate::{
    backend::{Backend, IMPL_PATH},
    desktop::settings::Namespace,
    zvariant::OwnedValue,
};

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
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = SettingsInterface::new(sender);
        let object_server = backend.cnx().object_server();

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
