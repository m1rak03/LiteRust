use literust_browser::{newtab, storage::BrowserData};
fn main() {
    let path = std::env::args().nth(1).expect("output HTML path");
    let html = newtab::render(&BrowserData::default(), false).replace(
        "</head>",
        &format!(
            "<script>{}\nwindow.applyAppearance({{design:0,theme:0}});</script></head>",
            include_str!("../ui/appearance.js")
        ),
    );
    std::fs::write(path, html).unwrap();
}
