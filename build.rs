//! Generates the WPE WebKit FFI bindings (`ffi`) with bindgen over the
//! system headers. An app takes this crate only under its own `wpe` feature,
//! so a build without that feature needs no WPE headers at all.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/wrapper.h");
    // Both modules are needed: wpe-webkit-2.0 is the engine, wpe-platform-2.0
    // is the embedding layer (WPEDisplay / WPEView / WPEToplevel) the
    // embedder subclasses. pkg-config emits the link flags for us.
    let mut clang_args = Vec::new();
    for module in ["wpe-webkit-2.0", "wpe-platform-2.0"] {
        let lib = pkg_config::Config::new()
            .probe(module)
            .unwrap_or_else(|e| panic!("`{module}` not found — pacman -S wpewebkit ({e})"));
        for path in &lib.include_paths {
            clang_args.push(format!("-I{}", path.display()));
        }
    }

    let bindings = bindgen::Builder::default()
        .header("src/wrapper.h")
        .clang_args(&clang_args)
        // The engine, the embedding layer, and just enough GObject to
        // register subclasses and turn a main loop.
        .allowlist_item("(wpe|WPE|webkit|WebKit)_?.*")
        // JavaScriptCore: a script message from the page arrives as a
        // JSCValue, so reading one needs `jsc_value_*`. webkit.h already
        // pulls in jsc.h; only the allowlist kept these out.
        .allowlist_item("(jsc|JSC)_?.*")
        .allowlist_item("g_(object|type|signal|bytes|timeout|free|error)_.*")
        // `g_free` itself, with no trailing word, misses the pattern above —
        // and a JSC string comes back owned, so it is needed to hand it back.
        .allowlist_function("g_free")
        // The main loop AND the context: `pump` drains the context directly.
        .allowlist_item("g_main_(loop|context)_.*")
        .allowlist_item("G(Object|Type|Value|Bytes|Error|MainLoop|ParamSpec|Closure).*")
        // The main-context poll protocol: GPollFD is what `query` fills in.
        .allowlist_item("G(MainContext|PollFD|Source).*")
        // Serving `cce:` pages: the handler answers with an input stream.
        .allowlist_item("g_(memory_input_stream|input_stream|file)_.*")
        .allowlist_item("G(InputStream|MemoryInputStream|File|Cancellable|AsyncResult).*")
        .allowlist_item("g_(type|object)_.*")
        // GObject's generated enums are plain C enums; keep them as consts so
        // vfunc tables and property flags stay comparable without casts.
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .derive_default(true)
        .layout_tests(false)
        .generate()
        .expect("bindgen failed over the WPE headers");

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("wpe_bindings.rs"))
        .expect("could not write wpe_bindings.rs");
}
