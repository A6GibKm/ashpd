use ashpd::backend::Backend;
use gettextrs::LocaleCategory;
use gtk::glib;
use std::sync::Arc;
use futures::join;

mod external_wayland_window;
mod external_window;
mod external_x11_window;
mod file_chooser;
mod settings;
mod wallpaper;

use file_chooser::FileChooser;
use settings::Settings;
use wallpaper::Wallpaper;

// NOTE Uncomment if you have ashpd-backend-demo.portal installed.
// const NAME: &str = "org.freedesktop.impl.portal.desktop.ashpd-backend-demo";
const NAME: &str = "org.freedesktop.impl.portal.desktop.gnome";

fn main() -> Result<(), ashpd::Error> {
    // Enable debug with `RUST_LOG=xdp_ashpd_gnome=debug COMMAND`.
    tracing_subscriber::fmt::init();

    // FIXME Use meson here
    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain("ashpd-backend-demo", "/usr/share/locale")
        .expect("Unable to bind the text domain");
    gettextrs::textdomain("ashpd-backend-demo").expect("Unable to switch to the text domain");

    glib::set_prgname(Some("ashpd-backend-demo"));

    // Avoid pointless and confusing recursion
    glib::unsetenv("GTK_USE_PORTAL");
    glib::setenv("ADW_DISABLE_PORTAL", "1", true).unwrap();
    // glib::setenv("GSK_RENDERER", "cairo", true).unwrap();

    gtk::init().unwrap();
    adw::init().unwrap();

    let main_context = glib::MainContext::default();

    log::debug!("Starting Main Loop");

    main_context.block_on(init_interfaces())
}

async fn init_interfaces() -> Result<(), ashpd::Error> {
    log::debug!("Starting interfaces at {NAME}");
    let backend = Backend::new(NAME.to_string()).await?;

    let wallpaper = Arc::new(ashpd::backend::Wallpaper::new(Wallpaper::default(), &backend).await?);
    let settings = Arc::new(ashpd::backend::Settings::new(Settings::default(), &backend).await?);
    let file_chooser =
        Arc::new(ashpd::backend::FileChooser::new(FileChooser::default(), &backend).await?);
    use futures::FutureExt;
    loop {
        // let a = file_chooser.next().fuse();
        // futures::select! {
        //     a_res = a => {
        //         if let Some(action) = a_res {
        //             let imp = Arc::clone(&file_chooser);
        //             let ctx = glib::MainContext::default();
        //             ctx.spawn_local(async move {
        //                 if let Err(err) = imp.activate(action).await {
        //                     log::error!("Could not handle file chooser: {err:?}");
        //                 }
        //             });
        //         }
        //     }
        // };
        // if let Some(action) = settings.next().await {
        //     let imp = Arc::clone(&settings);
        //     if let Err(err) = imp.activate(action).await {
        //         log::error!("Could not handle settings: {err:?}");
        //     }
        // };
        // if let Some(action) = wallpaper.next().await {
        //     let imp = Arc::clone(&wallpaper);
        //     if let Err(err) = imp.activate(action).await {
        //         log::error!("Could not handle wallpaper: {err:?}");
        //     }
        // };
        if let Some(action) = file_chooser.next().await {
            let imp = Arc::clone(&file_chooser);
            if let Err(err) = imp.activate(action).await {
                log::error!("Could not handle file chooser: {err:?}");
            }
        }
    }
}
