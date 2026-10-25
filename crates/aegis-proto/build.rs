//! Compiles `proto/aegis/v1/aegis.proto` with `protox`, a pure-Rust protobuf
//! compiler, so building the server needs no `protoc` binary.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aegis/v1/aegis.proto");
    let fds = protox::compile(["proto/aegis/v1/aegis.proto"], ["proto"])?;
    tonic_prost_build::configure()
        // `bytes` fields become `bytes::Bytes`: snapshot chunks and ops are
        // sliced and shared without copies.
        .bytes(".")
        .build_client(true)
        .build_server(true)
        .compile_fds(fds)?;
    Ok(())
}
