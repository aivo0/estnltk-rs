use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

fn main() {
    let vabamorf_root = PathBuf::from("../../estnltk-src/estnltk");
    let src_root = vabamorf_root.join("src");
    let inc_root = vabamorf_root.join("include");

    // Source directories (non-recursive), matching setup.py lines 39-46
    let src_dirs = ["etana", "etyhh", "fsc", "json", "proof", "estnltk"];

    // Include directories, matching setup.py lines 50-52
    let include_dirs = [
        "etana", "etyhh", "fsc", "json", "proof", "estnltk",
        "fsc/fsjni", "apps",
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .warnings(false) // suppress warnings from legacy C++ code
        .opt_level(2);

    // Add include paths
    for dir in &include_dirs {
        build.include(inc_root.join(dir));
    }

    // Collect all .cpp files from source directories (non-recursive)
    for dir in &src_dirs {
        let dir_path = src_root.join(dir);
        if let Ok(entries) = fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension() == Some(OsStr::new("cpp")) {
                    build.file(&path);
                }
            }
        }
    }

    build.compile("vabamorf");

    // Rerun if any source or header changes
    println!("cargo:rerun-if-changed=build.rs");
    for dir in &src_dirs {
        println!("cargo:rerun-if-changed={}", src_root.join(dir).display());
    }
    for dir in &include_dirs {
        println!("cargo:rerun-if-changed={}", inc_root.join(dir).display());
    }

    // Link C++ standard library
    println!("cargo:rustc-link-lib=stdc++");
}
