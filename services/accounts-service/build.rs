fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/accounts.proto", "proto/ledger.proto"], &["proto"])?;
    // Users service client (ValidateApiKey for holder-based account creation)
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/users.proto"], &["proto"])?;
    Ok(())
}

