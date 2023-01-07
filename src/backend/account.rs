use std::cell::RefCell;

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

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
pub trait AccountImpl {
    async fn get_user_information(
        &self,
        handle: OwnedObjectPath,
        app_id: impl Into<AppID>,
        window_identifier: WindowIdentifierType,
        options: UserInformationOptions,
    ) -> Response<UserInformation>;
}

pub struct Account<T: AccountImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: T,
}

unsafe impl<T: Send + AccountImpl> Send for Account<T> {}
unsafe impl<T: Sync + AccountImpl> Sync for Account<T> {}

impl<T: AccountImpl> Account<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = AccountInterface::new(sender);
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

        if let Some(Action::GetUserInformation(
            handle,
            app_id,
            window_identifier,
            options,
            sender,
        )) = response
        {
            let result = self
                .imp
                .get_user_information(handle, app_id, window_identifier, options)
                .await;
            let _ = sender.send(result);
        };

        Ok(())
    }
}

enum Action {
    GetUserInformation(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        UserInformationOptions,
        oneshot::Sender<Response<UserInformation>>,
    ),
}

struct AccountInterface {
    sender: Sender<Action>,
}

impl AccountInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self { sender }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Account")]
impl AccountInterface {
    async fn get_user_information(
        &mut self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        options: UserInformationOptions,
    ) -> Response<UserInformation> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::GetUserInformation(
            handle,
            app_id,
            window_identifier,
            options,
            sender,
        ));
        let mut stream = receiver.into_stream();
        let next = stream.next().await;

        next.unwrap().unwrap()
    }
}
