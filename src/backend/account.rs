use std::{cell::RefCell, sync::Arc};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

use super::{request::Request, RequestImpl};
use crate::{
    backend::{Backend, IMPL_PATH},
    desktop::{account::UserInformationResponse, request::Response},
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
    ) -> Response<UserInformationResponse>;
}

pub struct Account<T: AccountImpl, R: RequestImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    backend: Backend,
    imp: T,
    request_imp: Arc<R>,
}

unsafe impl<T: Send + AccountImpl, R: RequestImpl> Send for Account<T, R> {}
unsafe impl<T: Sync + AccountImpl, R: RequestImpl> Sync for Account<T, R> {}

impl<T: AccountImpl + Sync, R: RequestImpl> Account<T, R> {
    pub async fn new(imp: T, request: R, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = AccountInterface::new(sender);
        let object_server = backend.cnx().object_server();

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: RefCell::new(Some(receiver)),
            imp,
            request_imp: Arc::new(request),
            backend: backend.clone(),
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
            handle_path,
            app_id,
            window_identifier,
            options,
            sender,
        )) = response
        {
            let request =
                Request::new(Arc::clone(&self.request_imp), handle_path, &self.backend).await?;
            let result = self
                .imp
                .get_user_information(app_id, window_identifier, options)
                .await;
            let _ = sender.send(result);
            request.next().await?;
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
        oneshot::Sender<Response<UserInformationResponse>>,
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
    ) -> Response<UserInformationResponse> {
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
