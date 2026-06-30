// Build script: compile all elan proto files with tonic_build so both server
// and client stubs are available in elan-common's generated `proto` module.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "../../proto";

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                &format!("{proto_dir}/catalog.proto"),
                &format!("{proto_dir}/coordinator.proto"),
                &format!("{proto_dir}/iam.proto"),
                &format!("{proto_dir}/audit.proto"),
            ],
            &[proto_dir, "../../"],
        )?;

    Ok(())
}
