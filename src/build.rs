const ICON: &str = "assets/img/raster/logo_raster_small.png";

fn main() {
    println!("cargo::rerun-if-changed={ICON}");
    #[cfg(feature = "desktop")]
    windows_icon();
}

#[cfg(feature = "desktop")]
fn windows_icon() {
    use image::ExtendedColorType;
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::imageops::FilterType;
    use std::path::PathBuf;

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let logo = image::open(ICON).expect("the app icon is readable");
    let frames: Vec<IcoFrame> = [16, 24, 32, 48, 256]
        .into_iter()
        .map(|side| {
            let frame = logo
                .resize_exact(side, side, FilterType::Lanczos3)
                .into_rgba8();
            IcoFrame::as_png(frame.as_raw(), side, side, ExtendedColorType::Rgba8)
                .expect("the icon frame encodes")
        })
        .collect();
    let icon = out.join("AppIcon.ico");
    let file = std::fs::File::create(&icon).expect("the Windows icon is writable");
    IcoEncoder::new(file)
        .encode_images(&frames)
        .expect("the Windows icon encodes");
    let resource = out.join("app.rc");
    let path = icon.display().to_string().replace('\\', "/");
    std::fs::write(&resource, format!("32512 ICON \"{path}\"\n"))
        .expect("the resource is writable");
    embed_resource::compile(&resource, embed_resource::NONE)
        .manifest_required()
        .expect("the Windows icon compiles");
}
