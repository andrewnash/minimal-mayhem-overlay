use windows::{
    Globalization::Language,
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::{Foundation::*, Graphics::Gdi::*},
    core::{HSTRING, Result},
};

use std::{fs, path::Path};

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

/// One card's title crop and what OCR read from it, kept so failed reads can be saved.
#[derive(Default)]
pub struct Sample {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    lines: Vec<String>,
    matched: bool,
}

pub fn scan(
    engine: &OcrEngine,
    tiers: &Tiers,
    game: RECT,
) -> Result<([Option<CardStats>; 3], [Sample; 3])> {
    let mut cards = [None; 3];
    let mut samples: [Sample; 3] = Default::default();
    for index in 0..3 {
        (cards[index], samples[index]) = read_card(engine, tiers, game, index)?;
    }
    Ok((cards, samples))
}

/// Cheap check for the augment offer: OCR only the middle card's title region.
pub fn cards_open(engine: &OcrEngine, tiers: &Tiers, game: RECT) -> bool {
    matches!(read_card(engine, tiers, game, 1), Ok((Some(_), _)))
}

/// Text OCR returned for every card, for the log and for spotting a changed offer.
pub fn describe(samples: &[Sample; 3]) -> String {
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            format!(
                "card {} {}: {:?}",
                index + 1,
                if sample.matched { "ok" } else { "MISS" },
                sample.lines.join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Writes each card's crop (BMP) and OCR text under `dir`, prefixed with `stamp`.
pub fn save_samples(dir: &Path, stamp: &str, samples: &[Sample; 3]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    for (index, sample) in samples.iter().enumerate() {
        let status = if sample.matched { "ok" } else { "miss" };
        let base = dir.join(format!("{stamp}-card{}-{status}", index + 1));
        fs::write(base.with_extension("txt"), sample.lines.join("\n"))?;
        if !sample.pixels.is_empty() {
            fs::write(
                base.with_extension("bmp"),
                bmp(&sample.pixels, sample.width, sample.height),
            )?;
        }
    }
    Ok(())
}

/// 32-bit top-down BMP from BGRA pixels.
fn bmp(pixels: &[u8], width: i32, height: i32) -> Vec<u8> {
    let mut file = Vec::with_capacity(54 + pixels.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&width.to_le_bytes());
    file.extend_from_slice(&(-height).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&[0; 24]);
    file.extend_from_slice(pixels);
    file
}

fn read_card(
    engine: &OcrEngine,
    tiers: &Tiers,
    game: RECT,
    index: usize,
) -> Result<(Option<CardStats>, Sample)> {
    let width = game.right - game.left;
    let height = game.bottom - game.top;
    let centers = card_centers(width, height);
    // Stay inside one card so neighbouring titles never leak into the read.
    let crop_width = (height * 28 / 100).min((centers[1] - centers[0]) * 94 / 100);
    let crop_height = height * 19 / 100;
    let top = game.top + height * 33 / 100;
    let left = game.left + centers[index] - crop_width / 2;
    let Some(pixels) = capture(left, top, crop_width, crop_height) else {
        return Ok((None, Sample::default()));
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
    let mut lines = Vec::new();
    for line in result.Lines()? {
        let text = line.Text()?.to_string_lossy();
        if let Some((stats, confidence)) = tiers.recognize(&text)
            && best.is_none_or(|(_, score)| confidence > score)
        {
            best = Some((stats, confidence));
        }
        lines.push(text);
    }
    let stats = best.map(|(stats, _)| stats);
    let sample = Sample {
        pixels,
        width: crop_width,
        height: crop_height,
        lines,
        matched: stats.is_some(),
    };
    Ok((stats, sample))
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
