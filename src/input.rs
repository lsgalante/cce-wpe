//! Translating cce-ui input into `WPEEvent`s.
//!
//! Unlike the Servo backend — where `main.rs` carried `dom_key`/`dom_button`
//! helpers and the host took engine types — the mapping lives *here* and
//! The embedding app's `WebKitHost` (cce-browser) takes cce-ui's own `MouseButton` / `KeyEvent`. That
//! keeps engine vocabulary out of the chrome, so switching backends deletes
//! those helpers from `main.rs` rather than rewriting them.
//!
//! **Keyboard is the fiddly part.** `wpe_event_keyboard_new` wants an X11
//! *keysym* (`keyval`), not a character. Latin-1 codepoints are their own
//! keysym; anything above maps to `codepoint + 0x0100_0000`; named keys have
//! fixed `XK_*` values. Getting this wrong is silent — the page just receives
//! nothing useful.

use cce_ui::widget::{ElementState, Key, KeyEvent, MouseButton, NamedKey, ScrollPhase};

use crate::ffi::*;

/// X11 keysyms for the named keys cce-ui reports (`/usr/include/X11/keysymdef.h`).
mod keysym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const SPACE: u32 = 0x0020;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const DELETE: u32 = 0xffff;
    pub const F1: u32 = 0xffbe;
    pub const F2: u32 = 0xffbf;
    pub const F3: u32 = 0xffc0;
    pub const F4: u32 = 0xffc1;
    pub const F5: u32 = 0xffc2;
    pub const F6: u32 = 0xffc3;
    pub const F7: u32 = 0xffc4;
    pub const F8: u32 = 0xffc5;
    pub const F9: u32 = 0xffc6;
    pub const F10: u32 = 0xffc7;
    pub const F11: u32 = 0xffc8;
    pub const F12: u32 = 0xffc9;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const CONTROL_L: u32 = 0xffe3;
    pub const ALT_L: u32 = 0xffe9;
    pub const SUPER_L: u32 = 0xffeb;
}

/// A Unicode scalar as an X11 keysym: Latin-1 is identity, the rest is the
/// codepoint in the 0x01000000 plane.
fn unicode_keysym(c: char) -> u32 {
    match c as u32 {
        cp @ 0x20..=0xff => cp,
        cp => cp + 0x0100_0000,
    }
}

/// cce-ui key -> X11 keysym. `None` for keys with no sensible mapping.
pub fn keyval(key: &Key) -> Option<u32> {
    Some(match key {
        Key::Character(s) => unicode_keysym(s.chars().next()?),
        Key::Named(n) => match n {
            NamedKey::Backspace => keysym::BACKSPACE,
            NamedKey::Tab => keysym::TAB,
            NamedKey::Enter => keysym::RETURN,
            NamedKey::Escape => keysym::ESCAPE,
            NamedKey::Space => keysym::SPACE,
            NamedKey::ArrowDown => keysym::DOWN,
            NamedKey::ArrowLeft => keysym::LEFT,
            NamedKey::ArrowRight => keysym::RIGHT,
            NamedKey::ArrowUp => keysym::UP,
            NamedKey::End => keysym::END,
            NamedKey::Home => keysym::HOME,
            NamedKey::PageDown => keysym::PAGE_DOWN,
            NamedKey::PageUp => keysym::PAGE_UP,
            NamedKey::Delete => keysym::DELETE,
            NamedKey::Control => keysym::CONTROL_L,
            NamedKey::Shift => keysym::SHIFT_L,
            NamedKey::Alt => keysym::ALT_L,
            NamedKey::Super => keysym::SUPER_L,
            NamedKey::F1 => keysym::F1,
            NamedKey::F2 => keysym::F2,
            NamedKey::F3 => keysym::F3,
            NamedKey::F4 => keysym::F4,
            NamedKey::F5 => keysym::F5,
            NamedKey::F6 => keysym::F6,
            NamedKey::F7 => keysym::F7,
            NamedKey::F8 => keysym::F8,
            NamedKey::F9 => keysym::F9,
            NamedKey::F10 => keysym::F10,
            NamedKey::F11 => keysym::F11,
            NamedKey::F12 => keysym::F12,
        },
    })
}

/// X11 button numbering, which is what WPE expects.
pub fn button_number(b: MouseButton) -> Option<u32> {
    Some(match b {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        // Back/Forward are chrome navigation in `main.rs`, deliberately not
        // forwarded to the page.
        _ => return None,
    })
}

/// An X11 button number's held-button bit, the form a pointer event's
/// modifiers carry it in (`WPE_MODIFIER_POINTER_BUTTON1` is button 1, and the
/// five are consecutive).
pub fn button_modifier(n: u32) -> WPEModifiers::Type {
    match n {
        1..=5 => WPEModifiers::WPE_MODIFIER_POINTER_BUTTON1 << (n - 1),
        _ => 0,
    }
}

pub fn modifiers(ctrl: bool, shift: bool, alt: bool) -> WPEModifiers::Type {
    let mut m: WPEModifiers::Type = 0;
    if ctrl {
        m |= WPEModifiers::WPE_MODIFIER_KEYBOARD_CONTROL;
    }
    if shift {
        m |= WPEModifiers::WPE_MODIFIER_KEYBOARD_SHIFT;
    }
    if alt {
        m |= WPEModifiers::WPE_MODIFIER_KEYBOARD_ALT;
    }
    m
}

pub fn is_pressed(e: &KeyEvent) -> bool {
    e.state == ElementState::Pressed
}

/// A precise (pixel) scroll event for `view`, with the deltas and the point
/// `(x, y)` in logical pixels. The deltas are in cce-ui's winit convention
/// (positive = toward the top / left) and pass through **unchanged**.
///
/// Measured, not assumed: WPE already inverts on the way to the DOM, so a
/// negation here double-inverts and the page scrolls backwards. An earlier
/// cut negated them and cce-browser's `examples/wpe_input` caught it — the
/// page reported `deltaY` of the wrong sign.
///
/// `phase` is the gesture stage, and it decides how WebKit reads the event.
/// A host forwarding a live wheel passes cce-ui's
/// `scroll_motion::current_scroll_phase()`; one synthesizing a scroll passes
/// [`ScrollPhase::Wheel`], since outside a dispatch the published phase is
/// whatever the last real event left, possibly a stale `FingerEnd`. Finger
/// phases report the touchpad source so the engine treats the deltas as a
/// gesture, not clicks, and `FingerEnd` — a trackpad's finger lift, arriving
/// as a zero delta — is WebKit's scroll-stop, the signal its own kinetic
/// scrolling keys off. This lived in each host until 2026-10-13, and
/// cce-mail's copy sent every event as a mouse wheel with no stop, so a
/// flick in its HTML view stopped dead at the lift five weeks after
/// cce-browser had fixed the same thing in its own.
///
/// # Safety
///
/// `view` must be a live `WPEView`. The event comes back owned (or null),
/// for the host to dispatch and unref.
pub unsafe fn scroll_event(view: *mut WPEView, phase: ScrollPhase, dx: f64, dy: f64, x: f64, y: f64) -> *mut WPEEvent {
    let (source, is_stop) = match phase {
        ScrollPhase::Wheel => (WPEInputSource::WPE_INPUT_SOURCE_MOUSE, 0),
        ScrollPhase::Finger => (WPEInputSource::WPE_INPUT_SOURCE_TOUCHPAD, 0),
        ScrollPhase::FingerEnd => (WPEInputSource::WPE_INPUT_SOURCE_TOUCHPAD, 1),
    };
    wpe_event_scroll_new(
        view,
        source,
        now_ms(),
        0,
        dx,
        dy,
        1, // precise deltas: these are pixels, not notches
        is_stop,
        x,
        y,
    )
}

/// WPE stamps events with a millisecond clock. It only has to be monotonic
/// and consistent — `wpe_view_compute_press_count` uses it for double-click
/// detection, so a frozen value would turn every click into a triple-click.
pub fn now_ms() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}
