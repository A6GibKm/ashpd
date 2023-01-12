use async_std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::SinkExt;
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
    receiver: Arc<Mutex<Receiver<Action>>>,
    imp: T,
}

impl<T: SettingsImpl> Settings<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = SettingsInterface::new(sender);
        let object_server = backend.cnx().object_server();

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: Arc::new(Mutex::new(receiver)),
            imp,
        };

        Ok(provider)
    }

    pub async fn activate(&self, action: Action) -> Result<(), crate::Error> {
        match action {
            Action::ReadAll(namespaces, sender) => {
                let results = self.imp.read_all(namespaces).await;
                let _ = sender.send(results);
            }
            Action::Read(namespace, key, sender) => {
                let results = self.imp.read(&namespace, &key).await;
                let _ = sender.send(results);
            }
        }

        Ok(())
    }

    pub fn try_next(&self) -> Option<Action> {
        self.receiver.try_lock().unwrap().try_next().ok().flatten()
    }
}

pub enum Action {
    ReadAll(Vec<String>, oneshot::Sender<HashMap<String, Namespace>>),
    Read(String, String, oneshot::Sender<OwnedValue>),
}

struct SettingsInterface {
    sender: Arc<Mutex<Sender<Action>>>,
}

impl SettingsInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self {
            sender: Arc::new(Mutex::new(sender)),
        }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsInterface {
    async fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, Namespace> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::ReadAll(namespaces, sender))
            .await;

        receiver.await.unwrap()
    }

    async fn read(&self, namespace: String, key: String) -> OwnedValue {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::Read(namespace, key, sender))
            .await;

        receiver.await.unwrap()
    }
}
