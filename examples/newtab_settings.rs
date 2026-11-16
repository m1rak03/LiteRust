fn main() {
    let mut data = literust_browser::storage::BrowserData::default();
    data.customization.home_layout = std::env::args()
        .nth(1)
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    data.customization.show_services = false;
    data.customization.shortcut_count = 16;
    data.customization.accent = Some(0x22aaff);
    for i in 0..20 {
        data.toggle_bookmark(&format!("https://example.org/{i}"), &format!("Сайт {i}"));
    }
    let html = literust_browser::newtab::render(&data, false);
    let js = include_str!("../ui/appearance.js");
    let prefs = serde_json::to_string(&data.customization).unwrap();
    print!(
        "{}",
        html.replace(
            "</head>",
            &format!("<script>{js}\nwindow.applyCustomization({prefs});</script></head>")
        )
    );
}
