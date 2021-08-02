//! # Examples
//!
//! ```rust,no_run
//! use ashpd::documents::{DocumentsProxy, Permission};
//!
//! async fn run() -> ashpd::Result<()> {
//!     let connection = zbus::azync::Connection::session().await?;
//!     let proxy = DocumentsProxy::new(&connection).await?;
//!
//!     println!("{:#?}", proxy.mount_point().await?);
//!
//!     for (doc_id, host_path) in proxy.list("org.mozilla.firefox").await? {
//!         if doc_id == "f2ee988d" {
//!             let info = proxy.info(&doc_id).await?;
//!             println!("{:#?}", info);
//!         }
//!     }
//!
//!     proxy
//!         .grant_permissions(
//!             "f2ee988d",
//!             "org.mozilla.firefox",
//!             &[Permission::GrantPermissions],
//!         )
//!         .await?;
//!     proxy
//!         .revoke_permissions("f2ee988d", "org.mozilla.firefox", &[Permission::Write])
//!         .await?;
//!
//!     proxy.delete("f2ee988d").await?;
//!
//!     Ok(())
//! }
//! ```

pub(crate) const DESTINATION: &str = "org.freedesktop.portal.Documents";
pub(crate) const PATH: &str = "/org/freedesktop/portal/documents";

use std::{
    collections::HashMap, ffi::CString, os::unix::ffi::OsStrExt, os::unix::prelude::AsRawFd,
};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    str::FromStr,
};

use enumflags2::BitFlags;
use serde::{de::Deserializer, Deserialize, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use strum_macros::{AsRefStr, EnumString, IntoStaticStr, ToString};
use zvariant::{Fd, Signature};
use zvariant_derive::Type;

use crate::{
    helpers::{call_method, path_from_null_terminated},
    Error,
};

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Copy, Clone, BitFlags, Debug, Type)]
#[repr(u32)]
///
pub enum Flags {
    /// Reuse the existing document store entry for the file.
    ReuseExisting = 1,
    /// Persistent file.
    Persistent = 2,
    /// Depends on the application needs.
    AsNeededByApp = 4,
    /// Export a directory.
    ExportDirectory = 8,
}

/// A [`HashMap`] mapping application IDs to the permissions for that
/// application
pub type Permissions = HashMap<String, Vec<Permission>>;

#[derive(Debug, Clone, AsRefStr, EnumString, IntoStaticStr, ToString, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
/// The possible permissions to grant to a specific application for a specific
/// document.
pub enum Permission {
    /// Read access.
    Read,
    /// Write access.
    Write,
    #[strum(serialize = "grant-permissions")]
    /// The possibility to grant new permissions to the file.
    GrantPermissions,
    /// Delete access.
    Delete,
}

impl zvariant::Type for Permission {
    fn signature() -> Signature<'static> {
        String::signature()
    }
}

impl Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        String::serialize(&self.to_string(), serializer)
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Permission::from_str(&String::deserialize(deserializer)?).expect("invalid permission"))
    }
}

/// The interface lets sandboxed applications make files from the outside world
/// available to sandboxed applications in a controlled way.
///
/// Exported files will be made accessible to the application via a fuse
/// filesystem that gets mounted at `/run/user/$UID/doc/`. The filesystem gets
/// mounted both outside and inside the sandbox, but the view inside the sandbox
/// is restricted to just those files that the application is allowed to access.
///
/// Individual files will appear at `/run/user/$UID/doc/$DOC_ID/filename`,
/// where `$DOC_ID` is the ID of the file in the document store.
/// It is returned by the [`DocumentsProxy::add`] and
/// [`DocumentsProxy::add_named`] calls.
///
/// The permissions that the application has for a document store entry (see
/// [`DocumentsProxy::grant_permissions`]) are reflected in the POSIX mode bits
/// in the fuse filesystem.
///
/// Wrapper of the DBus interface: [`org.freedesktop.portal.Documents`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-org.freedesktop.portal.Documents).
#[derive(Debug)]
#[doc(alias = "org.freedesktop.portal.Documents")]
pub struct DocumentsProxy<'a>(zbus::azync::Proxy<'a>);

impl<'a> DocumentsProxy<'a> {
    /// Create a new instance of [`DocumentsProxy`].
    pub async fn new(connection: &zbus::azync::Connection) -> Result<DocumentsProxy<'a>, Error> {
        let proxy = zbus::azync::ProxyBuilder::new_bare(connection)
            .interface("org.freedesktop.portal.Documents")?
            .path(PATH)?
            .destination(DESTINATION)?
            .build()
            .await?;
        Ok(Self(proxy))
    }

    /// Get a reference to the underlying Proxy.
    pub fn inner(&self) -> &zbus::azync::Proxy<'_> {
        &self.0
    }

    /// Adds a file to the document store.
    /// The file is passed in the form of an open file descriptor
    /// to prove that the caller has access to the file.
    ///
    /// # Arguments
    ///
    /// * `o_path_fd` - Open file descriptor for the file to add.
    /// * `reuse_existing` - Whether to reuse an existing document store entry
    ///   for the file.
    /// * `persistent` - Whether to add the file only for this session or
    ///   permanently.
    ///
    /// # Returns
    ///
    /// The ID of the file in the document store.
    ///
    /// # Specifications
    ///
    /// See also [`Add`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.Add).
    #[doc(alias = "Add")]
    pub async fn add<F>(
        &self,
        o_path_fd: &F,
        reuse_existing: bool,
        persistent: bool,
    ) -> Result<String, Error>
    where
        F: AsRawFd + Debug,
    {
        call_method(
            &self.0,
            "Add",
            &(Fd::from(o_path_fd.as_raw_fd()), reuse_existing, persistent),
        )
        .await
    }

    /// Adds multiple files to the document store.
    /// The files are passed in the form of an open file descriptor
    /// to prove that the caller has access to the file.
    ///
    /// # Arguments
    ///
    /// * `o_path_fds` - Open file descriptors for the files to export.
    /// * `flags` - A [`Flags`].
    /// * `app_id` - An application ID, or empty string.
    /// * `permissions` - The permissions to grant.
    ///
    /// # Returns
    ///
    /// The IDs of the files in the document store along with other extra info.
    ///
    /// # Specifications
    ///
    /// See also [`AddFull`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.AddFull).
    #[doc(alias = "AddFull")]
    pub async fn add_full<F: AsRawFd>(
        &self,
        o_path_fds: &[&F],
        flags: BitFlags<Flags>,
        app_id: &str,
        permissions: &[Permission],
    ) -> Result<(Vec<String>, HashMap<String, zvariant::OwnedValue>), Error> {
        let o_path: Vec<Fd> = o_path_fds.iter().map(|f| Fd::from(f.as_raw_fd())).collect();
        call_method(&self.0, "AddFull", &(o_path, flags, app_id, permissions)).await
    }

    /// Creates an entry in the document store for writing a new file.
    ///
    /// # Arguments
    ///
    /// * `o_path_parent_fd` - Open file descriptor for the parent directory.
    /// * `filename` - The basename for the file.
    /// * `reuse_existing` - Whether to reuse an existing document store entry
    ///   for the file.
    /// * `persistent` - Whether to add the file only for this session or
    ///   permanently.
    ///
    /// # Returns
    ///
    /// The ID of the file in the document store.
    ///
    /// # Specifications
    ///
    /// See also [`AddNamed`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.AddNamed).
    #[doc(alias = "AddNamed")]
    pub async fn add_named<F, P>(
        &self,
        o_path_parent_fd: &F,
        filename: P,
        reuse_existing: bool,
        persistent: bool,
    ) -> Result<String, Error>
    where
        F: AsRawFd + Debug,
        P: AsRef<Path> + Serialize + zvariant::Type + Debug,
    {
        let cstr = CString::new(filename.as_ref().as_os_str().as_bytes())
            .expect("`filename` should not be null terminated");
        call_method(
            &self.0,
            "AddNamed",
            &(
                Fd::from(o_path_parent_fd.as_raw_fd()),
                cstr.as_bytes_with_nul(),
                reuse_existing,
                persistent,
            ),
        )
        .await
    }

    /// Adds multiple files to the document store.
    /// The files are passed in the form of an open file descriptor
    /// to prove that the caller has access to the file.
    ///
    /// # Arguments
    ///
    /// * `o_path_fd` - Open file descriptor for the parent directory.
    /// * `filename` - The basename for the file.
    /// * `flags` - A [`Flags`].
    /// * `app_id` - An application ID, or empty string.
    /// * `permissions` - The permissions to grant.
    ///
    /// # Returns
    ///
    /// The ID of the file in the document store along with other extra info.
    ///
    /// # Specifications
    ///
    /// See also [`AddNamedFull`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.AddNamedFull).
    #[doc(alias = "AddNamedFull")]
    pub async fn add_named_full<F, P>(
        &self,
        o_path_fd: &F,
        filename: P,
        flags: BitFlags<Flags>,
        app_id: &str,
        permissions: &[Permission],
    ) -> Result<(String, HashMap<String, zvariant::OwnedValue>), Error>
    where
        F: AsRawFd + Debug,
        P: AsRef<Path> + Serialize + zvariant::Type + Debug,
    {
        let cstr = CString::new(filename.as_ref().as_os_str().as_bytes())
            .expect("`filename` should not be null terminated");
        call_method(
            &self.0,
            "AddNamedFull",
            &(
                Fd::from(o_path_fd.as_raw_fd()),
                cstr.as_bytes_with_nul(),
                flags,
                app_id,
                permissions,
            ),
        )
        .await
    }

    /// Removes an entry from the document store. The file itself is not
    /// deleted.
    ///
    /// **Note** This call is available inside the sandbox if the
    /// application has the [`Permission::Delete`] for the document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - The ID of the file in the document store.
    ///
    /// # Specifications
    ///
    /// See also [`Delete`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.Delete).
    #[doc(alias = "Delete")]
    pub async fn delete(&self, doc_id: &str) -> Result<(), Error> {
        call_method(&self.0, "Delete", &(doc_id)).await
    }

    /// Returns the path at which the document store fuse filesystem is mounted.
    /// This will typically be `/run/user/$UID/doc/`.
    ///
    /// # Specifications
    ///
    /// See also [`GetMountPoint`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.GetMountPoint).
    #[doc(alias = "GetMountPoint")]
    #[doc(alias = "get_mount_point")]
    pub async fn mount_point(&self) -> Result<PathBuf, Error> {
        let bytes: Vec<u8> = call_method(&self.0, "GetMountPoint", &()).await?;
        Ok(path_from_null_terminated(bytes))
    }

    /// Grants access permissions for a file in the document store to an
    /// application.
    ///
    /// **Note** This call is available inside the sandbox if the
    /// application has the [`Permission::GrantPermissions`] for the document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - The ID of the file in the document store.
    /// * `app_id` - The ID of the application to which permissions are granted.
    /// * `permissions` - The permissions to grant.
    ///
    /// # Specifications
    ///
    /// See also [`GrantPermissions`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.GrantPermissions).
    #[doc(alias = "GrantPermissions")]
    pub async fn grant_permissions(
        &self,
        doc_id: &str,
        app_id: &str,
        permissions: &[Permission],
    ) -> Result<(), Error> {
        call_method(&self.0, "GrantPermissions", &(doc_id, app_id, permissions)).await
    }

    /// Gets the filesystem path and application permissions for a document
    /// store entry.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - The ID of the file in the document store.
    ///
    /// # Returns
    ///
    /// The path of the file in the host filesystem along with the
    /// [`Permissions`].
    ///
    /// # Specifications
    ///
    /// See also [`Info`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.Info).
    #[doc(alias = "Info")]
    pub async fn info(&self, doc_id: &str) -> Result<(PathBuf, Permissions), Error> {
        let (bytes, permissions): (Vec<u8>, Permissions) =
            call_method(&self.0, "Info", &(doc_id)).await?;
        Ok((path_from_null_terminated(bytes), permissions))
    }

    /// Lists documents in the document store for an application (or for all
    /// applications).
    ///
    /// # Arguments
    ///
    /// * `app-id` - The application ID, or '' to list all documents.
    ///
    /// # Returns
    ///
    /// [`HashMap`] mapping document IDs to their filesystem path on the host
    /// system.
    ///
    /// # Specifications
    ///
    /// See also [`List`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.List).
    #[doc(alias = "List")]
    pub async fn list(&self, app_id: &str) -> Result<HashMap<String, String>, Error> {
        call_method(&self.0, "List", &(app_id)).await
    }

    /// Looks up the document ID for a file.
    ///
    /// **Note** This call is not available inside the sandbox.
    ///
    /// # Arguments
    ///
    /// * `filename` - A path in the host filesystem.
    ///
    /// # Returns
    ///
    /// The ID of the file in the document store, or [`None`] if the file is not
    /// in the document store.
    ///
    /// # Specifications
    ///
    /// See also [`Lookup`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.Lookup).
    #[doc(alias = "Lookup")]
    pub async fn lookup<P: AsRef<Path> + Serialize + zvariant::Type + Debug>(
        &self,
        filename: P,
    ) -> Result<Option<String>, Error> {
        let cstr = CString::new(filename.as_ref().as_os_str().as_bytes())
            .expect("`filename` should not be null terminated");
        let doc_id: String = call_method(&self.0, "Lookup", &(cstr.as_bytes_with_nul())).await?;
        if doc_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(doc_id))
        }
    }

    /// Revokes access permissions for a file in the document store from an
    /// application.
    ///
    /// **Note** This call is available inside the sandbox if the
    /// application has the [`Permission::GrantPermissions`] for the document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - The ID of the file in the document store.
    /// * `app_id` - The ID of the application from which the permissions are
    ///   revoked.
    /// * `permissions` - The permissions to revoke.
    ///
    /// # Specifications
    ///
    /// See also [`RevokePermissions`](https://flatpak.github.io/xdg-desktop-portal/portal-docs.html#gdbus-method-org-freedesktop-portal-Documents.RevokePermissions).
    #[doc(alias = "RevokePermissions")]
    pub async fn revoke_permissions(
        &self,
        doc_id: &str,
        app_id: &str,
        permissions: &[Permission],
    ) -> Result<(), Error> {
        call_method(&self.0, "RevokePermissions", &(doc_id, app_id, permissions)).await
    }
}

/// Interact with `org.freedesktop.portal.FileTransfer` interface.
mod file_transfer;

pub use file_transfer::FileTransferProxy;
