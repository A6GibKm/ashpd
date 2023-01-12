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
    desktop::{
        file_chooser::{Choice, FileFilter},
        Response,
    },
    zvariant::{DeserializeDict, OwnedObjectPath, SerializeDict, Type},
    AppID, WindowIdentifierType,
};

// Does not coincide with the one in desktop/file_chooser.rs
#[derive(DeserializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct OpenFileOptions {
    pub accept_label: Option<String>,
    pub modal: Option<bool>,
    pub multiple: Option<bool>,
    pub directory: Option<bool>,
    pub filters: Option<Vec<FileFilter>>,
    pub current_filter: Option<FileFilter>,
    pub choices: Option<Vec<Choice>>,
}

// Does not coincide with the one in desktop/file_chooser.rs
#[derive(DeserializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct SaveFileOptions {
    pub accept_label: Option<String>,
    pub modal: Option<bool>,
    pub multiple: Option<bool>,
    pub filters: Option<Vec<FileFilter>>,
    pub current_filter: Option<FileFilter>,
    pub choices: Option<Vec<Choice>>,
    pub current_name: Option<String>,
    pub current_folder: Option<Vec<u8>>,
    pub current_file: Option<Vec<u8>>,
}

#[derive(DeserializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct SaveFilesOptions {
    // TODO Its in the xdp docs, but is it correct? See
    // https://github.com/flatpak/xdg-desktop-portal/issues/938
    // pub handle_token: Option<String>,
    pub accept_label: Option<String>,
    pub modal: Option<bool>,
    pub choices: Option<Vec<Choice>>,
    pub current_folder: Option<Vec<u8>>,
    pub files: Option<Vec<Vec<u8>>>,
}

#[derive(DeserializeDict, SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct OpenFileResults {
    pub uris: Option<Vec<url::Url>>,
    pub choices: Option<Vec<Choice>>,
    pub current_filter: Option<FileFilter>,
    pub writable: Option<bool>,
}

#[derive(DeserializeDict, SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct SaveFileResults {
    pub uris: Option<Vec<url::Url>>,
    pub choices: Option<Vec<Choice>>,
    pub current_filter: Option<FileFilter>,
}

#[derive(DeserializeDict, SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
pub struct SaveFilesResults {
    pub uris: Option<Vec<url::Url>>,
    pub choices: Option<Vec<Choice>>,
}

#[async_trait]
pub trait FileChooserImpl {
    async fn open_file(
        &self,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: &str,
        options: OpenFileOptions,
    ) -> Response<OpenFileResults>;

    async fn save_file(
        &self,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: &str,
        options: SaveFileOptions,
    ) -> Response<SaveFileResults>;

    async fn save_files(
        &self,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: &str,
        options: SaveFilesOptions,
    ) -> Response<SaveFilesResults>;
}

pub struct FileChooser<T: FileChooserImpl, R: RequestImpl> {
    receiver: RefCell<Option<Receiver<Action>>>,
    imp: T,
    backend: Backend,
    request_imp: Arc<R>,
}

unsafe impl<T: Send + FileChooserImpl, R: RequestImpl> Send for FileChooser<T, R> {}
unsafe impl<T: Sync + FileChooserImpl, R: RequestImpl> Sync for FileChooser<T, R> {}

impl<T: FileChooserImpl, R: RequestImpl> FileChooser<T, R> {
    pub async fn new(imp: T, request: R, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = FileChooserInterface::new(sender);
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

        match response {
            Some(Action::OpenFile(path, app_id, window_identifier, title, options, sender)) => {
                let request =
                    Request::new(Arc::clone(&self.request_imp), path, &self.backend).await?;
                let results = self
                    .imp
                    .open_file(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
            Some(Action::SaveFile(path, app_id, window_identifier, title, options, sender)) => {
                let request =
                    Request::new(Arc::clone(&self.request_imp), path, &self.backend).await?;
                let results = self
                    .imp
                    .save_file(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
            Some(Action::SaveFiles(path, app_id, window_identifier, title, options, sender)) => {
                let request =
                    Request::new(Arc::clone(&self.request_imp), path, &self.backend).await?;
                let results = self
                    .imp
                    .save_files(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
            None => (),
        }

        Ok(())
    }
}

enum Action {
    OpenFile(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        String,
        OpenFileOptions,
        oneshot::Sender<Response<OpenFileResults>>,
    ),
    SaveFile(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        String,
        SaveFileOptions,
        oneshot::Sender<Response<SaveFileResults>>,
    ),
    SaveFiles(
        OwnedObjectPath,
        AppID,
        WindowIdentifierType,
        String,
        SaveFilesOptions,
        oneshot::Sender<Response<SaveFilesResults>>,
    ),
}

struct FileChooserInterface {
    sender: Sender<Action>,
}

impl FileChooserInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self { sender }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooserInterface {
    async fn open_file(
        &mut self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: OpenFileOptions,
    ) -> Response<OpenFileResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::OpenFile(
            handle,
            app_id,
            window_identifier,
            title,
            options,
            sender,
        ));
        let mut stream = receiver.into_stream();

        stream.next().await.unwrap().unwrap()
    }

    async fn save_file(
        &mut self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: SaveFileOptions,
    ) -> Response<SaveFileResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::SaveFile(
            handle,
            app_id,
            window_identifier,
            title,
            options,
            sender,
        ));
        let mut stream = receiver.into_stream();

        stream.next().await.unwrap().unwrap()
    }

    async fn save_files(
        &mut self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: SaveFilesOptions,
    ) -> Response<SaveFilesResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self.sender.try_send(Action::SaveFiles(
            handle,
            app_id,
            window_identifier,
            title,
            options,
            sender,
        ));
        let mut stream = receiver.into_stream();

        stream.next().await.unwrap().unwrap()
    }
}
