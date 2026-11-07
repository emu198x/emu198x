use common_commodore_amiga_ocs as engine;
include!("boards.rs");

/// Locate a local fixture below the user's home, recording an unavailable home.
pub fn home_fixture_path(relative: &str) -> Option<std::path::PathBuf> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        emu198x_test_skip::record(&format!(
            "local fixture {relative} unavailable: neither HOME nor USERPROFILE is set"
        ));
        return None;
    };
    Some(std::path::PathBuf::from(home).join(relative))
}
