use aiks_core::Config;

#[test]
fn backend_defaults_legacy_and_unknown_modes_do_not_silently_start_a_writer() {
    let default = serde_json::to_value(Config::default()).unwrap();
    assert_eq!(default["backend"]["mode"], "legacy");
    for value in ["team", "servce_local", "http://127.0.0.1:1234"] {
        assert!(toml::from_str::<Config>(&format!("[backend]\nmode={value:?}\n")).is_err());
    }
    let config: Config = toml::from_str("[backend]\nmode='service_local'\n").unwrap();
    assert_eq!(serde_json::to_value(config).unwrap()["backend"]["mode"], "service_local");
}
