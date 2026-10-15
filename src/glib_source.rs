//! Waking on GLib activity instead of polling for it.
//!
//! WPE runs on a GLib `GMainContext`; cce-ui runs a calloop loop. The first
//! cut of cce-browser's `WebKitHost::pump` simply drained the context on a timer,
//! which works but burns wakeups when nothing is happening and adds latency
//! when something is.
//!
//! The bridge here is deliberately narrow: **calloop decides *when to look*,
//! GLib still does its own iteration.** We never reimplement GLib's
//! prepare/check/dispatch protocol — `g_main_context_iteration` does that,
//! correctly, and we only use `g_main_context_query` to learn what to wait on.
//!
//! GLib's fd set changes as WebKit opens sockets, and calloop wants stable
//! registrations, so the changing set lives in an **inner epoll fd**, held
//! by an **outer** one that is the one stable thing calloop watches. Each
//! pump rebuilds the inner set from scratch. GLib also asks for a timeout,
//! which a calloop timer carries.
//!
//! **Rebuilt, not diffed.** The set used to be one epoll kept in step by fd
//! *number*. When GLib closes an fd and the number is reused between two
//! syncs, the number-diff sees nothing change: the old file stays registered
//! (epoll ties an entry to the open file, not the number, and it lives on
//! while anything else holds the file) and the new one is never watched.
//! WebKit rendering into GPU buffers cycles a short-lived fd every frame,
//! and an old one left registered and readable spun the browser's loop at
//! 125,000 pumps a second (2026-10-15). A fresh inner epoll per sync cannot
//! hold anything stale; it costs a handful of syscalls a pump.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::event::epoll;

use crate::ffi::*;

/// The GLib fd set, mirrored into an epoll fd that calloop can watch.
pub struct GlibPoll {
    /// What calloop watches: holds `inner` and nothing else.
    outer: OwnedFd,
    /// GLib's fds as of the last sync, rebuilt by every sync.
    inner: OwnedFd,
    fds: Vec<GPollFD>,
    /// GLib's requested timeout in ms; `None` means "no timer needed".
    pub timeout: Option<u32>,
    /// When GLib next wants dispatching with no fd to say so — its own
    /// timeout, measured from the moment it was asked (`sync`). `None`:
    /// nothing scheduled, so only an fd (or the app) wakes it.
    pub deadline: Option<std::time::Instant>,
}

fn flags_of(events: u16) -> epoll::EventFlags {
    let mut f = epoll::EventFlags::empty();
    // G_IO_IN / OUT / ERR / HUP, which are the poll(2) values.
    if events & 0x001 != 0 {
        f |= epoll::EventFlags::IN;
    }
    if events & 0x004 != 0 {
        f |= epoll::EventFlags::OUT;
    }
    if events & 0x008 != 0 {
        f |= epoll::EventFlags::ERR;
    }
    if events & 0x010 != 0 {
        f |= epoll::EventFlags::HUP;
    }
    f
}

impl GlibPoll {
    pub fn new() -> std::io::Result<Self> {
        let outer = epoll::create(epoll::CreateFlags::CLOEXEC)?;
        let inner = epoll::create(epoll::CreateFlags::CLOEXEC)?;
        epoll::add(&outer, &inner, epoll::EventData::new_u64(0), epoll::EventFlags::IN)?;
        let mut this = Self {
            outer,
            inner,
            fds: Vec::new(),
            timeout: None,
            deadline: None,
        };
        this.sync();
        Ok(this)
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.outer.as_fd()
    }

    /// Ask GLib what it wants polled, and make the epoll set match.
    ///
    /// Called after every dispatch, because WebKit adds and drops fds as it
    /// opens connections — a set captured once goes stale within a page load.
    pub fn sync(&mut self) {
        unsafe {
            let ctx = g_main_context_default();
            // `query` is only meaningful between prepare and check; we are not
            // running that protocol ourselves, but prepare also updates the
            // context's own idea of the timeout, so call it for that.
            let mut max_priority: i32 = 0;
            g_main_context_prepare(ctx, &mut max_priority);

            let mut timeout: i32 = -1;
            // Two-pass: ask for the count, then fill.
            let n = g_main_context_query(ctx, max_priority, &mut timeout, std::ptr::null_mut(), 0);
            self.fds.clear();
            self.fds.resize(n.max(0) as usize, std::mem::zeroed());
            let n = if self.fds.is_empty() {
                0
            } else {
                g_main_context_query(
                    ctx,
                    max_priority,
                    &mut timeout,
                    self.fds.as_mut_ptr(),
                    self.fds.len() as i32,
                )
            };
            self.fds.truncate(n.max(0) as usize);
            self.timeout = (timeout >= 0).then_some(timeout as u32);
            // At least 4 ms out: a source that is ready again at once (a
            // repeating idle) must not turn the loop into a spin.
            self.deadline = self.timeout.map(|ms| {
                std::time::Instant::now() + std::time::Duration::from_millis(u64::from(ms).max(4))
            });
        }

        // A fresh inner set, swapped in for the old one (see the module
        // comment for why it is never diffed). Should the epoll calls fail —
        // out of fds — the old set stays: stale at worst, never empty.
        let Ok(inner) = epoll::create(epoll::CreateFlags::CLOEXEC) else { return };
        for p in &self.fds {
            let borrowed = unsafe { BorrowedFd::borrow_raw(p.fd) };
            let data = epoll::EventData::new_u64(p.fd as u64);
            let _ = epoll::add(&inner, borrowed, data, flags_of(p.events));
        }
        if epoll::add(&self.outer, &inner, epoll::EventData::new_u64(0), epoll::EventFlags::IN).is_err() {
            return;
        }
        let _ = epoll::delete(&self.outer, &self.inner);
        self.inner = inner;
    }

    /// Drain the inner epoll so it stops reporting readable. calloop is
    /// level-triggered on this fd; without this the loop would spin on a
    /// socket GLib has not consumed yet.
    pub fn drain(&self) {
        let mut events = epoll::EventVec::with_capacity(16);
        let _ = epoll::wait(&self.outer, &mut events, 0);
        let _ = epoll::wait(&self.inner, &mut events, 0);
    }
}
