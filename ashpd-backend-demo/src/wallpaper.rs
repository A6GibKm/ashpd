use ashpd::backend::{RequestImpl, WallpaperImpl, WallpaperOptions};
use ashpd::desktop::Response;
use ashpd::{AppID, WindowIdentifierType};
use async_trait::async_trait;
use gtk::gio::prelude::*;

#[derive(Default)]
pub struct Wallpaper;

const BACKGROUND_SCHEMA: &str = "org.gnome.desktop.background";

#[async_trait]
impl RequestImpl for Wallpaper {
    async fn close(&self) {
        log::debug!("IN Close()");
    }
}

#[async_trait]
impl WallpaperImpl for Wallpaper {
    async fn set_wallpaper_uri(
        &self,
        app_id: AppID,
        window_identifier: WindowIdentifierType,
        uri: url::Url,
        options: WallpaperOptions,
    ) -> Response<()> {
        log::debug!(
            "IN SetWallpaperURI({app_id}, {window_identifier:?}, {}, {options:?})",
            uri.as_str()
        );
        let response = if set_gsetting(BACKGROUND_SCHEMA, uri.as_str()).is_ok() {
            Response::ok(())
        } else {
            Response::other()
        };

        log::debug!("OUT SetWallpaperURI({response:?})",);
        response
    }
}

fn set_gsetting(schema: &str, uri: &str) -> anyhow::Result<()> {
    let settings = gtk::gio::Settings::new(schema);
    settings.set_string("picture-uri", uri)?;
    settings.set_string("picture-uri-dark", uri)?;

    Ok(())
}
