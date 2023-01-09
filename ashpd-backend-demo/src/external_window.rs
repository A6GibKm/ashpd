use ashpd::WindowIdentifierType;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::external_wayland_window::ExternalWaylandWindow;
use crate::external_x11_window::ExternalX11Window;

pub(crate) enum ExternalWindow {
    Wayland(ExternalWaylandWindow),
    X11(ExternalX11Window),
}

impl ExternalWindow {
    pub fn try_new(window_identifier: WindowIdentifierType) -> Option<Self> {
        match window_identifier {
            WindowIdentifierType::Wayland(exported_handle) => {
                Some(Self::Wayland(ExternalWaylandWindow::new(exported_handle)?))
            }
            WindowIdentifierType::X11(foreign_xid) => {
                Some(Self::X11(ExternalX11Window::new(foreign_xid)?))
            }
        }
    }

    pub fn set_parent_of<S: glib::IsA<gdk::Surface>>(&self, surface: &S) {
        match self {
            Self::X11(x11_window) => x11_window.set_parent_of(
                surface
                    .as_ref()
                    .downcast_ref::<gdk_x11::X11Surface>()
                    .unwrap(),
            ),
            Self::Wayland(wl_window) => wl_window.set_parent_of(
                surface
                    .as_ref()
                    .downcast_ref::<gdk_wayland::WaylandSurface>()
                    .unwrap(),
            ),
        }
    }

    pub fn new_fake_window(maybe_self: &Option<Self>) -> gtk::Window {
        match maybe_self {
            Some(Self::X11(x11_window)) => glib::Object::new(&[("display", x11_window.display())]),
            Some(Self::Wayland(wl_window)) => {
                glib::Object::new(&[("display", wl_window.display())])
            }
            None => gtk::Window::new(),
        }
    }
}
