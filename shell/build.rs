//! Windows resource build script.
//! 1. Converts the source PNG logo into a standard ICO file.
//! 2. Embeds the ICO + product metadata into the executable via winresource.

use std::fs;
use std::io::BufWriter;
use std::path::Path;

use image::ImageEncoder;

fn main() {
    // CARGO_MANIFEST_DIR = shell/  →  assets/ is two directories up (../../assets/)
    let manifest_str = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_str = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_str);

    let assets_dir = Path::new(&manifest_str).join("..").join("assets");
    let logo_path = assets_dir.join("logo.png");
    let ico_out_path = out_dir.join("app.ico");

    // ── 1. Convert logo.png → a proper ICO file ────────────────────────────────
    let img = image::open(&logo_path)
        .unwrap_or_else(|e| panic!("failed to open {:?}: {}", logo_path, e));

    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());

    let file = fs::File::create(&ico_out_path)
        .unwrap_or_else(|e| panic!("failed to create {:?}: {}", ico_out_path, e));
    let writer = BufWriter::new(file);
    let encoder = image::codecs::ico::IcoEncoder::new(writer);
    encoder
        .write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8)
        .unwrap_or_else(|e| panic!("failed to encode ICO: {}", e));

    println!("cargo:warning=ICO generated: {}x{}", w, h);

    // ── 2. Embed ICO + product metadata via winresource ─────────────────────────
    let mut res = winresource::WindowsResource::new();

    res.set("ProductName", "Future Academy Link")
        .set(
            "FileDescription",
            "Future Academy Link — local hardware link server",
        )
        .set("LegalCopyright", "Copyright (C) 2026 Windify")
        .set("CompanyName", "Windify");

    res.set_icon(ico_out_path.to_string_lossy().as_ref());

    res.compile()
        .unwrap_or_else(|e| panic!("failed to compile Windows resources: {}", e));
}
