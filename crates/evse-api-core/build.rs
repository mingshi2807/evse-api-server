fn main() {
    let lib_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("libiso15118.git");

    let build_dir = lib_dir.join("build_prod");
    let cbv2g_dir = build_dir.join("_deps/libcbv2g-build/lib/cbv2g");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file(lib_dir.join("api/c/iso15118_c.cpp"))
        .include(lib_dir.join("api/c"))
        .include(lib_dir.join("include"))
        .flag("-fPIC")
        .compile("iso15118_c");

    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("src/iso15118").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("api/c").display()
    );
    println!("cargo:rustc-link-search=native={}", cbv2g_dir.display());

    println!("cargo:rustc-link-lib=static=iso15118");
    println!("cargo:rustc-link-lib=static=iso15118_c");
    println!("cargo:rustc-link-lib=static=cbv2g_iso20");
    println!("cargo:rustc-link-lib=static=cbv2g_exi_codec");
    println!("cargo:rustc-link-lib=static=cbv2g_tp");
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=crypto");
    println!("cargo:rustc-link-lib=dylib=stdc++");

    println!(
        "cargo:rerun-if-changed={}",
        lib_dir.join("api/c/iso15118_c.cpp").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        lib_dir.join("api/c/iso15118_c.h").display()
    );
}
