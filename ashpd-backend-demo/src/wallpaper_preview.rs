use gtk::glib;
use gtk::{prelude::*, CompositeTemplate};
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(file = "wallpaper.ui")]
    pub struct WallpaperPreview {
        #[template_child]
        pub button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WallpaperPreview {
        const NAME: &'static str = "WallpaperPreview";
        type Type = super::WallpaperPreview;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[gtk::template_callbacks]
    impl WallpaperPreview {
        #[template_callback]
        fn on_button_clicked(&self) {
            todo!();
        }
    }

    impl ObjectImpl for WallpaperPreview {}
    impl WidgetImpl for WallpaperPreview {}
}

glib::wrapper! {
    pub struct WallpaperPreview(ObjectSubclass<imp::WallpaperPreview>)
        @extends gtk::Widget;
}

impl Default for WallpaperPreview {
    fn default() -> Self {
        glib::Object::new(&[]).unwrap()
    }
}

impl WallpaperPreview {}
