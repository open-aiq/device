fn main() {
    println!("cargo:rerun-if-env-changed=OLED_CONTROLLER");
    for name in [
        "BACKEND_BASE_URL",
        "WIPE_CONFIG_ON_BOOT",
        "MOCK_WIFI_SSID",
        "MOCK_WIFI_PASSWORD",
        "MOCK_DEVICE_ID",
        "MOCK_DEVICE_KEY",
        "MOCK_LAT",
        "MOCK_LON",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rustc-check-cfg=cfg(oled_sh1106)");
    println!("cargo:rustc-check-cfg=cfg(oled_ssd1306)");

    match std::env::var("OLED_CONTROLLER")
        .unwrap_or_else(|_| "sh1106".to_owned())
        .to_ascii_lowercase()
        .as_str()
    {
        "sh1106" => println!("cargo:rustc-cfg=oled_sh1106"),
        "ssd1306" => println!("cargo:rustc-cfg=oled_ssd1306"),
        value => panic!("unsupported OLED_CONTROLLER '{value}'; expected 'sh1106' or 'ssd1306'"),
    }

    embuild::espidf::sysenv::output();
}
