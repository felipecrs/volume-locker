use winresource::WindowsResource;

fn main() {
    let mut res = WindowsResource::new();
    // The first icon gets set as the executable icon
    res.set_icon_with_id("icons/volume-locked.ico", "volume-locked-icon");
    res.set_icon_with_id("icons/volume-unlocked.ico", "volume-unlocked-icon");
    res.set_language(0x0009); // English
    res.compile().unwrap();
}
