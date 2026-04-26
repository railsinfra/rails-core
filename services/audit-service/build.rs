use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const CARGO_MANIFEST_DIR_ENV: &str = "CARGO_MANIFEST_DIR";
    let manifest_dir = PathBuf::from(std::env::var(CARGO_MANIFEST_DIR_ENV)?);
    let proto_root = manifest_dir.join("../../proto");
    let audit_proto = proto_root.join("audit/v1/audit.proto");
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[audit_proto], &[proto_root])?;
    Ok(())
}
