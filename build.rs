fn main() {
    slint_build::compile("ui/mainwindow.slint").unwrap();
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("platforms/windows/icon.ico");
        res.compile().unwrap();
    }
}