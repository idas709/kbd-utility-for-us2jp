fn main() {
    slint_build::compile("ui/appwindow.slint").expect("Slint compilation failed");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("ui/assets/app_icon.ico");
        let _ = res.compile();
    }
}
