use windows::Win32::Graphics::Gdi::*;

pub const SIZE: i32 = 20;
const PIXEL_BYTES: usize = (SIZE * SIZE * 4) as usize;
pub type Icon = &'static [u8; PIXEL_BYTES];

pub fn find(name: &str) -> Option<Icon> {
    let mut atlas = include_bytes!("../data/item-icons.bin").as_slice();
    while let Some((&length, rest)) = atlas.split_first() {
        let (key, rest) = rest.split_at_checked(length.into())?;
        let (pixels, rest) = rest.split_at_checked(PIXEL_BYTES)?;
        if key.eq_ignore_ascii_case(name.as_bytes()) {
            return pixels.try_into().ok();
        }
        atlas = rest;
    }
    None
}

fn bitmap_info() -> BITMAPINFO {
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: SIZE,
        biHeight: -SIZE,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    info
}

pub fn draw(dc: HDC, x: i32, y: i32, icon: Icon) {
    let info = bitmap_info();
    unsafe {
        SetDIBitsToDevice(
            dc,
            x,
            y,
            SIZE as u32,
            SIZE as u32,
            0,
            0,
            0,
            SIZE as u32,
            icon.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_real_icon_to_a_gdi_surface() {
        let icon = find("Essence Reaver").unwrap();
        let (x, y) = (10, 6);
        let offset = ((y * SIZE + x) * 4) as usize;
        let expected = u32::from_le_bytes([icon[offset + 2], icon[offset + 1], icon[offset], 0]);
        assert_ne!(expected, 0);
        let actual = unsafe {
            let mut pixels = std::ptr::null_mut();
            let surface =
                CreateDIBSection(None, &bitmap_info(), DIB_RGB_COLORS, &mut pixels, None, 0)
                    .unwrap();
            let dc = CreateCompatibleDC(None);
            let old = SelectObject(dc, surface.into());
            draw(dc, 0, 0, icon);
            let actual = GetPixel(dc, x, y).0;
            SelectObject(dc, old);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(surface.into());
            actual
        };
        assert_eq!(actual, expected);
    }
}
