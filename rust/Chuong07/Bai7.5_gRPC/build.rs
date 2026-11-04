// Sinh mã Rust (thông điệp, lớp cơ sở của server, client) từ hợp đồng tra_cuu.proto khi biên dịch.
fn main() {
    // dùng protoc đóng gói sẵn trong crate protoc-bin-vendored
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());
    tonic_build::compile_protos("tra_cuu.proto").unwrap();
}
