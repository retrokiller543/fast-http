use super::*;

#[test]
fn default_settings_cap_bodies_at_8_mib() {
    assert_eq!(Settings::default().max_body_bytes, 8 * 1024 * 1024);
}
