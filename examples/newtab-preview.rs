use literust_browser::{newtab, storage::BrowserData};
fn main() {
    let html = newtab::render(&BrowserData::default(), false).replace(
        "</head>",
        &format!(
            "<script>{}\nwindow.applyAppearance({{design:0,theme:0}});</script></head>",
            include_str!("../ui/appearance.js")
        ),
    );
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, html).expect("write new-tab fixture");
    } else {
        print!("{html}");
    }
}
