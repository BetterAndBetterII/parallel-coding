use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    watch_dir(Path::new("templates"));
}

fn watch_dir(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch_dir(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
