#[path = "../../../test-data/commodore/amiga/ddf-register-writes/probe-native.rs"]
mod probe;

#[test]
fn ddf_register_writes_match_the_registered_request_corpus()
-> Result<(), Box<dyn std::error::Error>> {
    probe::verify(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/commodore/amiga/ddf-register-writes"),
    )
}
