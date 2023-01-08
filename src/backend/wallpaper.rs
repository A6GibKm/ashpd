use std::{cell::RefCell, sync::Arc};

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{FutureExt, StreamExt};
use zbus::dbus_interface;

use crate::{
    backend::{
        request::{Request, RequestImpl},
        Backend, IMPL_PATH,
    },
    desktop::{request::Response, wallpaper::SetOn},
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
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        uri: url::Url,
        options: WallpaperOptions,
    ) -> Response<()>;
}

pub struct Wallpaper<T: WallpaperImpl + RequestImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: Arc<T>,
    cnx: zbus::Connection,
}

unsafe impl<T: Send + WallpaperImpl + RequestImpl> Send for Wallpaper<T> {}
unsafe impl<T: Sync + WallpaperImpl + RequestImpl> Sync for Wallpaper<T> {}

impl<T: WallpaperImpl + RequestImpl> Wallpaper<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = WallpaperInterface::new(sender);
        let object_server = backend.cnx().object_server();

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: RefCell::new(Some(receiver)),
            imp: Arc::new(imp),
            cnx: backend.cnx().clone(),
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
            handle_path,
            app_id,
            window_identifier,
            uri,
            options,
            sender,
        )) = response
        {
            let request = Request::new(Arc::clone(&self.imp), handle_path, &self.cnx).await?;
            let result = self
                .imp
                .set_wallpaper_uri(app_id, window_identifier, uri, options)
                .await;
            let _ = sender.send(result);
            request.next().await?;
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
        oneshot::Sender<Response<()>>,
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
    ) -> Response<()> {
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
