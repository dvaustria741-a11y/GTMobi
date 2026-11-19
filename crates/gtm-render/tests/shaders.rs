//! Parses and validates the WGSL shaders offline, so a shader error shows up in `cargo test`
//! instead of at device creation on the phone.

fn validate(name: &str, src: &str) {
    let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
}

#[test]
fn pbr_shader_is_valid() {
    validate("pbr.wgsl", include_str!("../src/shaders/pbr.wgsl"));
}

#[test]
fn sky_shader_is_valid() {
    validate("sky.wgsl", include_str!("../src/shaders/sky.wgsl"));
}
