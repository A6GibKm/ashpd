use async_std::sync::Mutex;
use std::ffi::{CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures_channel::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

// FIXME Manually implement Deserialize to catch non null-terminated arrays.
// FIXME Use OsString directly once
// https://gitlab.freedesktop.org/dbus/zbus/-/merge_requests/638 is merged.
/// A file name represented as a null-terminated byte array.
///
/// Implements `AsRef<Path>`.
#[derive(Debug, Default)]
pub struct FileName(CString);

impl AsRef<Path> for FileName {
    fn as_ref(&self) -> &Path {
        let os_str = OsStr::from_bytes(self.0.as_bytes());

        Path::new(os_str)
    }
}

impl Type for FileName {
    fn signature() -> zbus::zvariant::Signature<'static> {
        <&[u8]>::signature()
    }
}

impl<'de> Deserialize<'de> for FileName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        let c_string = CString::from_vec_with_nul(bytes)
            .map_err(|_| serde::de::Error::custom("Bytes are not null-terminated"))?;

        Ok(Self(c_string))
    }
}

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
    pub current_folder: Option<FileName>,
    pub current_file: Option<FileName>,
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
    pub current_folder: Option<FileName>,
    pub files: Option<Vec<FileName>>,
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

pub struct FileChooser<T: FileChooserImpl + RequestImpl> {
    receiver: Arc<Mutex<Receiver<Action>>>,
    imp: Arc<T>,
    cnx: zbus::Connection,
}

impl<T: FileChooserImpl + RequestImpl> FileChooser<T> {
    pub async fn new(imp: T, backend: &Backend) -> zbus::Result<Self> {
        let (sender, receiver) = futures_channel::mpsc::channel(10);
        let iface = FileChooserInterface::new(sender);
        let object_server = backend.cnx().object_server();

        object_server.at(IMPL_PATH, iface).await?;
        let provider = Self {
            receiver: Arc::new(Mutex::new(receiver)),
            imp: Arc::new(imp),
            cnx: backend.cnx().clone(),
        };

        Ok(provider)
    }

    pub async fn activate(&self, action: Action) -> Result<(), crate::Error> {
        match action {
            Action::OpenFile(path, app_id, window_identifier, title, options, sender) => {
                let request = Request::new(Arc::clone(&self.imp), path, &self.cnx).await?;
                let results = self
                    .imp
                    .open_file(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
            Action::SaveFile(path, app_id, window_identifier, title, options, sender) => {
                let request = Request::new(Arc::clone(&self.imp), path, &self.cnx).await?;
                let results = self
                    .imp
                    .save_file(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
            Action::SaveFiles(path, app_id, window_identifier, title, options, sender) => {
                let request = Request::new(Arc::clone(&self.imp), path, &self.cnx).await?;
                let results = self
                    .imp
                    .save_files(app_id, window_identifier, &title, options)
                    .await;
                let _ = sender.send(results);
                request.next().await?;
            }
        }

        Ok(())
    }

    pub async fn next(&self) -> Option<Action> {
        self.receiver.lock().await.next().await
    }
}

pub enum Action {
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
    sender: Arc<Mutex<Sender<Action>>>,
}

impl FileChooserInterface {
    pub fn new(sender: Sender<Action>) -> Self {
        Self {
            sender: Arc::new(Mutex::new(sender)),
        }
    }
}

#[dbus_interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooserInterface {
    async fn open_file(
        &self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: OpenFileOptions,
    ) -> Response<OpenFileResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::OpenFile(
                handle,
                app_id,
                window_identifier,
                title,
                options,
                sender,
            ))
            .await;

        receiver.await.unwrap()
    }

    async fn save_file(
        &self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: SaveFileOptions,
    ) -> Response<SaveFileResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::SaveFile(
                handle,
                app_id,
                window_identifier,
                title,
                options,
                sender,
            ))
            .await;

        receiver.await.unwrap()
    }

    async fn save_files(
        &self,
        handle: OwnedObjectPath,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        title: String,
        options: SaveFilesOptions,
    ) -> Response<SaveFilesResults> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let _ = self
            .sender
            .lock()
            .await
            .send(Action::SaveFiles(
                handle,
                app_id,
                window_identifier,
                title,
                options,
                sender,
            ))
            .await;

        receiver.await.unwrap()
    }
}
