//! WPE WebKit embedded in a cce-ui app.
//!
//! The parts of an embedding that do not depend on what the app does with
//! a page: the bindings ([`ffi`], bindgen over `wpe/webkit.h` and
//! `wpe/wpe-platform.h`), the WPEPlatform subclasses that hand rendered
//! frames to the app instead of a Wayland surface ([`subclass`]), GLib's
//! main context driven from calloop ([`glib_source`]), cce-ui input turned
//! into `WPEEvent`s ([`input`]), a frame's damage ([`damage`]) and the
//! input-method context that says which page field is open ([`ime`]).
//!
//! Each app keeps its own host on top — cce-browser's tabbed `WebKitHost`,
//! cce-mail's sandboxed `MailWebView`. Until 2026-10-10 cce-mail carried a
//! copy of these files meant to stay byte-identical with cce-browser's;
//! by then two of them had drifted (frame pacing and damage, the GLib
//! deadline), and mail ran the older embedding.

pub mod ffi {
    #![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
    // bindgen's output, regenerated every build: its bitfield accessors trip
    // these, and there is no source to fix them in.
    #![allow(clippy::useless_transmute, clippy::ptr_offset_with_cast, clippy::missing_safety_doc, clippy::too_many_arguments)]
    include!(concat!(env!("OUT_DIR"), "/wpe_bindings.rs"));
}

/// What a frame changed, so only that much is read back and uploaded.
pub mod damage;
pub mod glib_source;
/// Which page text field is open, for the on-screen keyboard.
pub mod ime;
pub mod input;
pub mod subclass;
