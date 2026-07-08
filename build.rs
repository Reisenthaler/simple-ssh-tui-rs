use std::{ 
    env, 
    path::{ PathBuf },
    fs 
};

fn main() {
    let target = env::var("TARGET").unwrap();
    let manifest_dir =  PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let bin_name =  get_rsync_binary_name(&target);
    let source_path = manifest_dir.join("rsync-binaries").join(&bin_name);

    if !source_path.exists() {
        panic!("rsync binary not found for target {}", target);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest_path = out_dir.join("rsync-binary");

    fs::copy(&source_path, &dest_path).expect("failed to coppy rsync binary from source to diestination");

}


fn get_rsync_binary_name(target: &str) -> String {
    match target {
        t if t.contains("aarch64-apple-darwin") => "rsync-macos-aarch64".to_string(),

        _ => panic!("no rsync binary for tharget: {} exists", target)
    }
}
