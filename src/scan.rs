use windows::{
    Globalization::Language,
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::{Foundation::*, Graphics::Gdi::*},
    core::{HSTRING, Result},
};

use crate::tiers::{CardStats, Tiers};

pub fn english_ocr() -> Result<OcrEngine> {
    let english = Language::CreateLanguage(&HSTRING::from("en-US"))?;
    OcrEngine::TryCreateFromLanguage(&english)
}

/// Measured on a live 3840x1440 offer (HUD GlobalScale 0.25): cards sit 490 px apart,
/// 424 px wide, spanning about 18%-66% of the window height.
pub fn card_centers(width: i32, height: i32) -> [i32; 3] {
    let spacing = (height * 34 / 100).min(width / 3);
    [width / 2 - spacing, width / 2, width / 2 + spacing]
}

pub fn scan(engine: &OcrEngine, tiers: &Tiers, game: RECT) -> Result<[Option<CardStats>; 3]> {
    let mut cards = [None; 3];
    for (index, card) in cards.iter_mut().enumerate() {
        *card = read_card(engine, tiers, game, index)?;
    }
    Ok(cards)
}

/// Cheap check for the augment offer: OCR only the middle card's title region.
pub fn cards_open(engine: &OcrEngine, tiers: &Tiers, game: RECT) -> bool {
    matches!(read_card(engine, tiers, game, 1), Ok(Some(_)))
}

fn read_card(
    engine: &OcrEngine,
    tiers: &Tiers,
    game: RECT,
    index: usize,
) -> Result<Option<CardStats>> {
    let width = game.right - game.left;
    let height = game.bottom - game.top;
    let centers = card_centers(width, height);
    // Stay inside one card so neighbouring titles never leak into the read.
    let crop_width = (height * 28 / 100).min((centers[1] - centers[0]) * 94 / 100);
    let crop_height = height * 19 / 100;
    let top = game.top + height * 33 / 100;
    let left = game.left + centers[index] - crop_width / 2;
    let Some(pixels) = capture(left, top, crop_width, crop_height) else {
        return Ok(None);
    };
    let writer = DataWriter::new()?;
    writer.WriteBytes(&pixels)?;
    let buffer = writer.DetachBuffer()?;
    let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        crop_width,
        crop_height,
        BitmapAlphaMode::Ignore,
    )?;
    let result = engine.RecognizeAsync(&bitmap)?.join()?;
    let mut best = None;
    for line in result.Lines()? {
        let text = line.Text()?.to_string_lossy();
        if let Some((stats, confidence)) = tiers.recognize(&text)
            && best.is_none_or(|(_, score)| confidence > score)
        {
            best = Some((stats, confidence));
        }
    }
    Ok(best.map(|(stats, _)| stats))
}

fn capture(x: i32, y: i32, width: i32, height: i32) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let mut pixels = vec![0; (width * height * 4) as usize];
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut rows = 0;
        if !memory.is_invalid() && !bitmap.is_invalid() {
            let original = SelectObject(memory, bitmap.into());
            let copied = BitBlt(memory, 0, 0, width, height, Some(screen), x, y, SRCCOPY);
            SelectObject(memory, original);
            if copied.is_ok() {
                rows = GetDIBits(
                    memory,
                    bitmap,
                    0,
                    height as u32,
                    Some(pixels.as_mut_ptr().cast()),
                    &mut info,
                    DIB_RGB_COLORS,
                );
            }
        }
        if !bitmap.is_invalid() {
            let _ = DeleteObject(bitmap.into());
        }
        if !memory.is_invalid() {
            let _ = DeleteDC(memory);
        }
        ReleaseDC(None, screen);
        (rows == height).then_some(pixels)
    }
}
