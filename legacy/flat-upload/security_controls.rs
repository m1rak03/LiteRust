fn main() {
    let template = include_str!("../ui/toolbar.html").replace(
        "/* APPEARANCE_SCRIPT */",
        include_str!("../ui/appearance.js"),
    );
    print!(
        "{}",
        literust_browser::security::control_document(&template).unwrap()
    );
}
