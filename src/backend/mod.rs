use futures::{executor::ThreadPool, task::SpawnExt};
use zbus::names::WellKnownName;

// mod access;
mod account;
mod file_chooser;
mod request;
// mod session;
mod settings;
mod wallpaper;

pub use account::{Account, AccountImpl, UserInformationOptions};
pub use file_chooser::{
    FileChooser, FileChooserImpl, OpenFileOptions, OpenFileResults, SaveFileOptions,
    SaveFileResults, SaveFilesOptions, SaveFilesResults,
};
pub use request::{Request, RequestImpl};
pub use settings::{Settings, SettingsImpl};
pub use wallpaper::{Wallpaper, WallpaperImpl, WallpaperOptions};

pub(crate) const IMPL_PATH: &str = "/org/freedesktop/portal/desktop";

// We use option to be able to take() without cloning. Unwraping is safe as they
// are set in construction.
pub struct Backend {
    cnx: Option<zbus::Connection>,
    name: Option<WellKnownName<'static>>,
}

impl Backend {
    pub async fn new<N: TryInto<WellKnownName<'static>>>(name: N) -> Result<Self, crate::Error>
    where
        zbus::Error: From<<N as TryInto<WellKnownName<'static>>>::Error>,
    {
        Self::new_inner(name).await.map_err(From::from)
    }

    async fn new_inner<N: TryInto<WellKnownName<'static>>>(name: N) -> zbus::Result<Self>
    where
        zbus::Error: From<<N as TryInto<WellKnownName<'static>>>::Error>,
    {
        let cnx = zbus::Connection::session().await.unwrap();
        let proxy = zbus::fdo::DBusProxy::builder(&cnx).build().await?;
        let name = name.try_into()?;

        proxy
            .request_name(
                name.clone(),
                zbus::fdo::RequestNameFlags::ReplaceExisting.into(),
            )
            .await?;

        Ok(Backend {
            cnx: Some(cnx),
            name: Some(name),
        })
    }

    async fn release(cnx: &zbus::Connection, name: WellKnownName<'_>) -> zbus::Result<()> {
        let proxy = zbus::fdo::DBusProxy::builder(cnx).build().await?;
        proxy.release_name(name).await?;

        Ok(())
    }

    fn cnx(&self) -> &zbus::Connection {
        self.cnx.as_ref().unwrap()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let (Some(cnx), Some(name)) = (self.cnx.take(), self.name.take()) {
            #[cfg(feature = "tracing")]
            tracing::error!("Releasing interface {name}");
            let executor = ThreadPool::new().unwrap();
            if let Err(_err) = executor.spawn(async move {
                if let Err(_err) = Backend::release(&cnx, name).await {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Could not release name: {_err}");
                }
            }) {
                #[cfg(feature = "tracing")]
                tracing::error!("Could not spawn executor: {_err}");
            }
        }
    }
}
