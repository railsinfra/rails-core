fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/accounts.proto", "proto/ledger.proto"], &["proto"])?;
    // Users service client (ValidateApiKey for holder-based account creation)
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/users.proto"], &["proto"])?;
    let proto_root = std::path::Path::new("../../proto");
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&[proto_root.join("audit/v1/audit.proto")], &[proto_root])?;
    Ok(())
}

