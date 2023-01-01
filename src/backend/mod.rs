// mod access;
// mod account;
// mod request;
// mod session;
mod wallpaper;

pub use wallpaper::{Wallpaper, WallpaperImpl, WallpaperOptions};

pub(crate) const IMPL_PATH: &str = "/org/freedesktop/portal/desktop";
