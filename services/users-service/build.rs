use std::path::{Path, PathBuf};

const CARGO_MANIFEST_DIR_ENV: &str = "CARGO_MANIFEST_DIR";

fn audit_proto_paths(manifest_dir: &Path) -> (PathBuf, PathBuf) {
    let vendored = manifest_dir.join("proto/audit/v1/audit.proto");
    if vendored.exists() {
        return (vendored, manifest_dir.join("proto"));
    }
    let root = manifest_dir.join("../../proto");
    (root.join("audit/v1/audit.proto"), root)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var(CARGO_MANIFEST_DIR_ENV)?);
    println!("cargo:rerun-if-changed=proto/users.proto");
    println!("cargo:rerun-if-changed=proto/accounts.proto");
    println!("cargo:rerun-if-changed=proto/audit/v1/audit.proto");
    tonic_build::configure()
        .build_client(true)
        .compile_protos(&["proto/accounts.proto"], &["proto"])?;
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/users.proto"], &["proto"])?;
    let (audit_proto, audit_include) = audit_proto_paths(&manifest_dir);
    tonic_build::configure()
        .build_client(true)
        .compile_protos(&[audit_proto], &[audit_include])?;
    Ok(())
}
