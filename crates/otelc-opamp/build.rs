fn main() {
    println!("cargo:rerun-if-changed=../../proto/opamp.proto");
    println!("cargo:rerun-if-changed=../../proto/anyvalue.proto");
    prost_build::compile_protos(&["../../proto/opamp.proto"], &["../../proto"])
        .expect("failed to compile opamp.proto");
}
