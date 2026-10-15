# cce-wpe

WPE WebKit embedded in a cce-ui app, shared by `cce-browser` and `cce-mail`.
Read `../cce-compositor/WORKSPACE.md` first (multi-repo rule, ccebuild,
concurrent sessions). This crate is its own git repository; origin is
GitHub, and the post-commit hook pushes.

## What lives here

| Module | What it is |
|---|---|
| `ffi` | bindgen over `src/wrapper.h` (`wpe/webkit.h` + `wpe/wpe-platform.h`), with an allowlist of the GLib/GObject/JSC items the embedding needs (`build.rs`) |
| `subclass` | the WPEPlatform `WPEDisplay` / `WPEView` / `WPEToplevel` subclasses: frames go to `FRAME_SINK` instead of a Wayland surface, clipboard sync, the input-method context hook, and `offer_dmabuf` (GPU-buffer frames, opt-in) |
| `glib_source` | GLib's main context driven from calloop (an outer epoll fd over an inner one rebuilt from GLib's pollfd set every sync, plus GLib's own deadline) |
| `input` | cce-ui `KeyEvent` / `MouseButton` into `WPEEvent`s, and the wheel's scroll event in its gesture phase (`scroll_event` — the touchpad source and finger-lift stop WebKit's kinetic scrolling needs) |
| `damage` | a frame's damage rects, merged and packed for a partial upload |
| `ime` | the input-method context: which page field is open, its caret, and `inputmode="none"` |

Each app keeps its own host (`cce-browser/src/wpe/host.rs`,
`cce-mail/src/wpe/host.rs`), which re-exports these modules under its
`wpe` module so the host's `super::subclass` etc. paths resolve.

## Rules

- **The frame sink decides pacing.** `FRAME_SINK` returns whether it keeps
  the buffer; a sink that keeps it must later say both `released` and
  `rendered` (see `subclass.rs`). Returning `false` hands both back at
  once, which is what cce-mail does.
- **Frames are mappable memory (SHM) unless the app offers GPU buffers**
  with `subclass::offer_dmabuf`, before its first webview — WebKit settles
  a toplevel's buffer type when it first uses it. The offer names the DRM
  device the app samples on (WebKit ignores a DMABuf offer without one, on
  the display *and* the format group) and keeps SHM offered behind it.
  cce-browser offers (`src/wpe/host/gpu.rs`, from cce-ui's
  `probe_dmabuf`); cce-mail does not, and gets exactly what it got before.
- **GLib's fd set is rebuilt, never diffed** (`glib_source.rs`). Diffing by
  fd number left a closed-and-reused fd's old file registered and
  readable, which spun the loop at 125,000 pumps a second once WebKit
  rendered into GPU buffers (it cycles a short-lived fd every frame).
- After pushing, run `../bump-revs.sh --commit cce-wpe` so the two apps'
  git pins follow.
- Needs `pacman -S wpewebkit` (pkg-config `wpe-webkit-2.0`,
  `wpe-platform-2.0`).
