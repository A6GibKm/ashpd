use ashpd::backend::Backend;
use gettextrs::LocaleCategory;
use gtk::glib;
use std::sync::Arc;

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

pub enum Action {
    FileChooser(file_chooser::FileChooserAction),
}

fn main() {
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
    glib::setenv("GSK_RENDERER", "cairo", true).unwrap();

    gtk::init().unwrap();
    adw::init().unwrap();

    log::debug!("Starting interfaces at {NAME}");

    let (sender, receiver) = glib::MainContext::channel::<Action>(glib::PRIORITY_DEFAULT);
    receiver.attach(None, handle_action);

    let main_loop = glib::MainLoop::new(None, false);

    async_std::task::spawn(async move {
        init_interfaces(sender).await.unwrap();
    });

    log::debug!("Starting Main Loop");
    main_loop.run();
}

fn handle_action(action: Action) -> glib::Continue {
    match action {
        Action::FileChooser(action) => file_chooser::handle_action(action),
    }
}

async fn init_interfaces(sender: glib::Sender<Action>) -> Result<(), ashpd::Error> {
    let backend = Backend::new(NAME.to_string()).await?;

    let wallpaper = Arc::new(ashpd::backend::Wallpaper::new(Wallpaper::default(), &backend).await?);
    let settings = Arc::new(ashpd::backend::Settings::new(Settings::default(), &backend).await?);
    let file_chooser =
        Arc::new(ashpd::backend::FileChooser::new(FileChooser::new(sender), &backend).await?);

    loop {
        if let Some(action) = settings.try_next() {
            let imp = Arc::clone(&settings);
            async_std::task::spawn(async move {
                if let Err(err) = imp.activate(action).await {
                    log::error!("Could not handle settings: {err:?}");
                }
            });
        };
        if let Some(action) = wallpaper.try_next() {
            let imp = Arc::clone(&wallpaper);
            async_std::task::spawn(async move {
                if let Err(err) = imp.activate(action).await {
                    log::error!("Could not handle wallpaper: {err:?}");
                }
            });
        };
        if let Some(action) = file_chooser.try_next() {
            let imp = Arc::clone(&file_chooser);
            async_std::task::spawn(async move {
                if let Err(err) = imp.activate(action).await {
                    log::error!("Could not handle file chooser: {err:?}");
                }
            });
        };
    }
}
