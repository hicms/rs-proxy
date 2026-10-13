use proxy_platform::icons::ExecutableIcons;
use std::{io::Cursor, path::PathBuf, sync::Arc};
use windows_sys::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources,
};

fn decode(png: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (32, 32, png::ColorType::Rgba)
    );
    bytes.truncate(info.buffer_size());
    assert!(bytes.chunks_exact(4).any(|pixel| pixel[3] > 0));
    assert!(bytes.chunks_exact(4).any(|pixel| pixel[3] == 0));
    bytes
}

fn translucent_ico() -> Vec<u8> {
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 8..24 {
        for x in 8..24 {
            rgba[(y * 32 + x) * 4..(y * 32 + x + 1) * 4].copy_from_slice(&[200, 80, 40, 128]);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 32, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    let mut ico = vec![0, 0, 1, 0, 1, 0, 32, 32, 0, 0, 1, 0, 32, 0];
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&png);
    ico
}

#[test]
fn file_icons_preserve_identity_transparency_cache_and_windows_resources() {
    let windows = PathBuf::from(std::env::var_os("WINDIR").unwrap());
    let explorer = windows.join("explorer.exe");
    let powershell = windows.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut cache = ExecutableIcons::default();
    let first = cache.get(&explorer).expect("Explorer has an embedded icon");
    let second = cache
        .get(&powershell)
        .expect("PowerShell has an embedded icon");
    decode(&first);
    decode(&second);
    assert_ne!(first, second);
    assert!(Arc::ptr_eq(&first, &cache.get(&explorer).unwrap()));

    let temporary = tempfile::tempdir().unwrap();
    let fixture = temporary.path().join("透明图标.ico");
    assert!(cache.get(&fixture).is_none());
    std::fs::write(&fixture, translucent_ico()).unwrap();
    let rgba = decode(
        &cache
            .get(&fixture)
            .expect("Negative cache invalidated after creation"),
    );
    let center = &rgba[(16 * 32 + 16) * 4..(16 * 32 + 17) * 4];
    for (actual, expected) in center.iter().zip([200u8, 80, 40, 128]) {
        assert!(actual.abs_diff(expected) <= 2, "RGBA {center:?}");
    }
    std::fs::write(&fixture, b"no icon resource").unwrap();
    assert!(
        cache.get(&fixture).is_none(),
        "Changed file invalidates positive cache"
    );
    assert!(
        cache.get(&std::env::current_exe().unwrap()).is_none(),
        "No embedded icon must not become an association icon"
    );

    for index in 0..512 {
        cache.get(&temporary.path().join(format!("missing-{index}.exe")));
    }
    assert!(
        !Arc::ptr_eq(&first, &cache.get(&explorer).unwrap()),
        "Old entries are evicted"
    );

    // Fresh caches force actual extraction and destruction on every iteration.
    let resources = || unsafe {
        [
            GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS),
            GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS),
        ]
    };
    let before = resources();
    for _ in 0..200 {
        assert!(ExecutableIcons::default().get(&explorer).is_some());
    }
    let after = resources();
    for (before, after) in before.into_iter().zip(after) {
        assert!(
            after <= before + 2,
            "Windows resource leak: {before} -> {after}"
        );
    }
}
