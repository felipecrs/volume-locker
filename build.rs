use winresource::WindowsResource;

fn main() {
    let mut res = WindowsResource::new();
    res.set_icon_with_id("icons/mdi-volume-equal-custom.ico", "app-icon");
    res.compile().unwrap();
}
