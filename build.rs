extern crate protoc_grpcio;

fn main() {
    let proto_root = "src/protos";
    let service_proto = "example/service.proto";
    println!("cargo:rerun-if-changed={}", proto_root);
    println!("cargo:rerun-if-changed={}/{}", proto_root, service_proto);
    protoc_grpcio::compile_grpc_protos(
        &[service_proto],
        &[proto_root],
        &proto_root
    ).expect("Failed to compile gRPC definitions!");
}
