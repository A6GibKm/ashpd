use gtk::gdk;
use gtk::prelude::*;

use gtk::glib::translate::*;

pub(crate) struct ExternalWaylandWindow {
    exported_handle: String,
    pub wl_display: gdk_wayland::WaylandDisplay,
}

impl ExternalWaylandWindow {
    pub fn new(exported_handle: String) -> Option<Self> {
        let Some(wl_display) = wayland_display() else {
            log::warn!("Failed to open Wayland display");
            return None;
        };

        Some(Self {
            exported_handle,
            wl_display,
        })
    }

    pub fn set_parent_of(&self, surface: &gdk_wayland::WaylandSurface) {
        if !surface
            .downcast_ref::<gdk_wayland::WaylandToplevel>()
            .unwrap()
            .set_transient_for_exported(&self.exported_handle)
        {
            log::warn!("Failed to set portal window transient for external parent");
        }
    }

    pub fn display(&self) -> &gdk::Display {
        self.wl_display.upcast_ref()
    }
}

fn wayland_display() -> Option<gdk_wayland::WaylandDisplay> {
    gdk::set_allowed_backends("wayland");
    let display: Option<gdk::Display> =
        unsafe { from_glib_none(gdk::ffi::gdk_display_open(None::<&str>.to_glib_none().0)) };
    gdk::set_allowed_backends("*");
    display.and_downcast::<gdk_wayland::WaylandDisplay>()
}
