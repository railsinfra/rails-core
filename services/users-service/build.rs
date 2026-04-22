fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .compile_protos(&["proto/accounts.proto"], &["proto"])?;
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/users.proto"], &["proto"])?;
    tonic_build::configure()
        .build_client(true)
        .compile_protos(
            &["../../proto/audit/v1/audit.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
