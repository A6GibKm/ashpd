use gtk::gdk;
use gtk::prelude::*;
use std::os::raw::{c_uchar, c_ulong};
use x11::xlib;

use gtk::glib::translate::*;

pub struct ExternalX11Window {
    foreign_xid: c_ulong,
    pub x11_display: gdk_x11::X11Display,
}

impl ExternalX11Window {
    pub fn new(foreign_xid: c_ulong) -> Option<Self> {
        let Some(x11_display) = x11_display() else {
            log::warn!("Failed to open X11 display");
            return None;
        };
        Some(Self {
            foreign_xid,
            x11_display,
        })
    }

    pub fn set_parent_of(&self, surface: &gdk_x11::X11Surface) {
        unsafe {
            let display = &self.x11_display;
            let x_display = display.xdisplay();
            let foreign_xid = self.foreign_xid;
            xlib::XSetTransientForHint(x_display, surface.xid(), foreign_xid);
            let atom =
                gdk_x11::x11_get_xatom_by_name_for_display(display, "_NET_WM_WINDOW_TYPE_DIALOG");
            xlib::XChangeProperty(
                x_display,
                surface.xid(),
                gdk_x11::x11_get_xatom_by_name_for_display(display, "_NET_WM_WINDOW_TYPE"),
                xlib::XA_ATOM,
                32,
                xlib::PropModeReplace,
                &atom as *const _ as *const c_uchar,
                1,
            );
        }
    }

    pub fn display(&self) -> &gdk::Display {
        self.x11_display.upcast_ref()
    }
}

fn x11_display() -> Option<gdk_x11::X11Display> {
    gdk::set_allowed_backends("x11");
    let display: Option<gdk::Display> =
        unsafe { from_glib_none(gdk::ffi::gdk_display_open(None::<&str>.to_glib_none().0)) };
    gdk::set_allowed_backends("*");
    display.and_downcast::<gdk_x11::X11Display>()
}
