# cce-wpe

WPE WebKit embedded in a cce-ui app, shared by `cce-browser` and `cce-mail`.
Read `../cce-compositor/WORKSPACE.md` first (multi-repo rule, ccebuild,
concurrent sessions). This crate is its own git repository; origin is
GitHub, and the post-commit hook pushes.

## What lives here

| Module | What it is |
|---|---|
| `ffi` | bindgen over `src/wrapper.h` (`wpe/webkit.h` + `wpe/wpe-platform.h`), with an allowlist of the GLib/GObject/JSC items the embedding needs (`build.rs`) |
| `subclass` | the WPEPlatform `WPEDisplay` / `WPEView` / `WPEToplevel` subclasses: frames go to `FRAME_SINK` instead of a Wayland surface, clipboard sync, the input-method context hook |
| `glib_source` | GLib's main context driven from calloop (one epoll fd over GLib's pollfd set, plus GLib's own deadline) |
| `input` | cce-ui `KeyEvent` / `MouseButton` into `WPEEvent`s |
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
- After pushing, run `../bump-revs.sh --commit cce-wpe` so the two apps'
  git pins follow.
- Needs `pacman -S wpewebkit` (pkg-config `wpe-webkit-2.0`,
  `wpe-platform-2.0`).
