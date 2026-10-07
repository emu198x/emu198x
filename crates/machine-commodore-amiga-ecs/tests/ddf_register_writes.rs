#[path = "../../../test-data/commodore/amiga/ddf-register-writes/probe-copper.rs"]
mod probe;

#[test]
fn copper_ddf_writes_preserve_comparator_and_memory_service_order()
-> Result<(), Box<dyn std::error::Error>> {
    probe::verify()
}
