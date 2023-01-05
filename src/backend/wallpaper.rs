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
    desktop::{
        request::{BasicResponse, Response},
        wallpaper::SetOn,
    },
    zvariant::{DeserializeDict, OwnedObjectPath, Type},
    AppID, WindowIdentifierType,
};

#[derive(DeserializeDict, Type, Debug)]
#[zvariant(signature = "dict")]
pub struct WallpaperOptions {
    #[zvariant(rename = "show-preview")]
    pub show_preview: Option<bool>,
    #[zvariant(rename = "set-on")]
    pub set_on: Option<SetOn>,
}

#[async_trait]
pub trait WallpaperImpl {
    async fn set_wallpaper_uri(
        &self,
        handle: OwnedObjectPath,
        app_id: impl Into<AppID>,
        window_identifier: WindowIdentifierType,
        uri: url::Url,
        options: WallpaperOptions,
    ) -> Response<BasicResponse>;
}

pub struct Wallpaper<T: WallpaperImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: T,
}

unsafe impl<T: Send + WallpaperImpl> Send for Wallpaper<T> {}
unsafe impl<T: Sync + WallpaperImpl> Sync for Wallpaper<T> {}

impl<T: WallpaperImpl> Wallpaper<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = WallpaperInterface::new(sender);
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

        if let Some(Action::SetWallpaperURI(
            handle,
            app_id,
            window_identifier,
            uri,
            options,
            sender,
        )) = response
        {
            let result = self
                .imp
                .set_wallpaper_uri(handle, app_id, window_identifier, uri, options)
                .await;
            let _ = sender.send(result);
        };

        Ok(())
    }
}

enum Action {
    SetWallpaperURI(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        url::Url,
        WallpaperOptions,
        oneshot::Sender<Response<BasicResponse>>,
    ),
}

struct WallpaperInterface {
    sender: Sender<Action>,
}

impl WallpaperInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self { sender }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Wallpaper")]
impl WallpaperInterface {
    #[dbus_interface(name = "SetWallpaperURI")]
    async fn set_wallpaper_uri(
        &mut self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        uri: url::Url,
        options: WallpaperOptions,
    ) -> Response<BasicResponse> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::SetWallpaperURI(
            handle,
            app_id,
            window_identifier,
            uri,
            options,
            sender,
        ));
        let mut stream = receiver.into_stream();
        let next = stream.next().await;

        next.unwrap().unwrap()
    }
}
