fn main() {
    let mut config = prost_build::Config::new();
    config.bytes(&["."]);

    config
        .compile_protos(&["proto/record.proto"], &["proto/", "src/"])
        .unwrap();
}
