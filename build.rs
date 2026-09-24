use std::{env, error::Error, fs::File, io::Cursor, path::PathBuf};

const ICON_SIZES: [u32; 5] = [16, 20, 24, 32, 48];
const SVG: &[u8] = include_bytes!("assets/gauge_arc_icon.svg");

fn main() {
    println!("cargo:rerun-if-changed=assets/gauge_arc_icon.svg");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if let Err(error) = build_windows_icon() {
        panic!("failed to build Windows icon: {error}");
    }
}

fn build_windows_icon() -> Result<(), Box<dyn Error>> {
    let tree = resvg::usvg::Tree::from_data(SVG, &resvg::usvg::Options::default())?;
    let mut directory = ico::IconDir::new(ico::ResourceType::Icon);

    for size in ICON_SIZES {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("invalid icon size")?;
        let scale_x = size as f32 / tree.size().width();
        let scale_y = size as f32 / tree.size().height();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale_x, scale_y),
            &mut pixmap.as_mut(),
        );

        // Going through PNG converts tiny-skia's premultiplied pixels to the
        // straight-alpha RGBA representation expected by the ICO encoder.
        let png = pixmap.encode_png()?;
        let image = ico::IconImage::read_png(Cursor::new(png))?;
        directory.add_entry(ico::IconDirEntry::encode(&image)?);
    }

    let icon_path =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?).join("codex-tray.ico");
    directory.write(File::create(&icon_path)?)?;

    winresource::WindowsResource::new()
        .set_icon(icon_path.to_string_lossy().as_ref())
        .compile()?;
    Ok(())
}
