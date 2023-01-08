use async_std::sync::Mutex;
use std::sync::Arc;

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::SinkExt;
use zbus::dbus_interface;

use super::{request::Request, RequestImpl};
use crate::{
    backend::{Backend, IMPL_PATH},
    desktop::{account::UserInformation, request::Response},
    zvariant::{DeserializeDict, OwnedObjectPath, Type},
    AppID, WindowIdentifierType,
};

#[derive(Debug, DeserializeDict, Type)]
#[zvariant(signature = "dict")]
pub struct UserInformationOptions {
    reason: Option<String>,
}

impl UserInformationOptions {
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[async_trait]
pub trait AccountImpl: RequestImpl {
    async fn get_user_information(
        &self,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        options: UserInformationOptions,
    ) -> Response<UserInformation>;
}

pub struct Account<T: AccountImpl + RequestImpl> {
    receiver: Arc<Mutex<Receiver<Action>>>,
    cnx: zbus::Connection,
    imp: Arc<T>,
}

impl<T: AccountImpl + RequestImpl> Account<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = AccountInterface::new(sender);
        let object_server = backend.cnx().object_server();

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: Arc::new(Mutex::new(receiver)),
            imp: Arc::new(imp),
            cnx: backend.cnx().clone(),
        };

        Ok(provider)
    }

    pub fn try_next(&self) -> Option<Action> {
        self.receiver.try_lock().unwrap().try_next().ok().flatten()
    }

    pub async fn activate(&self, action: Action) -> Result<(), crate::Error> {
        let Action::GetUserInformation(handle_path, app_id, window_identifier, options, sender) =
            action;
        let request = Request::new(Arc::clone(&self.imp), handle_path, &self.cnx).await?;
        let result = self
            .imp
            .get_user_information(app_id, window_identifier, options)
            .await;
        let _ = sender.send(result);
        request.next().await?;

        Ok(())
    }
}

pub enum Action {
    GetUserInformation(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        UserInformationOptions,
        oneshot::Sender<Response<UserInformation>>,
    ),
}

struct AccountInterface {
    sender: Arc<Mutex<Sender<Action>>>,
}

impl AccountInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self {
            sender: Arc::new(Mutex::new(sender)),
        }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Account")]
impl AccountInterface {
    async fn get_user_information(
        &self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        options: UserInformationOptions,
    ) -> Response<UserInformation> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::GetUserInformation(
                handle,
                app_id,
                window_identifier,
                options,
                sender,
            ))
            .await;

        receiver.await.unwrap()
    }
}
