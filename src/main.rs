mod scan;
mod tiers;

use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            LibraryLoader::GetModuleHandleW,
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{
                MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, VK_F8,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{Result, w},
};

const SCAN: i32 = 1;
const QUIT: i32 = 2;
const BADGE_HEIGHT: i32 = 34;
const BADGE_CENTER_Y_PERCENT: i32 = 71;
const COLOR_KEY: COLORREF = COLORREF(0x00ff00ff);

thread_local! {
    static BADGES: Cell<[(i32, Option<tiers::CardStats>); 3]> = const { Cell::new([(0, None); 3]) };
}

fn main() -> Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let engine = scan::english_ocr().inspect_err(|_| {
        eprintln!("Windows English OCR could not start. Check the en-US OCR language capability.");
    })?;
    if std::env::args().nth(1).as_deref() == Some("--check-ocr") {
        println!(
            "Windows OCR ready ({})",
            engine.RecognizerLanguage()?.LanguageTag()?
        );
        return Ok(());
    }
    let data: tiers::Dataset =
        serde_json::from_str(include_str!("../data/augments.json")).expect("bundled tier dataset");
    let tiers = data.tiers();
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        let overlay = create_overlay()?;
        RegisterHotKey(None, SCAN, MOD_NOREPEAT, VK_F8.0 as u32)?;
        RegisterHotKey(
            None,
            QUIT,
            MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT,
            VK_F8.0 as u32,
        )?;
        println!(
            "Minimal Mayhem Overlay | patch {} | F8 scan | Ctrl+Shift+F8 quit",
            data.patch
        );

        let mut displayed = None;
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, None, 0, 0).0 {
                -1 => return Err(windows::core::Error::from_thread()),
                0 => return Ok(()),
                _ => {}
            }
            match message.message {
                WM_HOTKEY if message.wParam.0 == QUIT as usize => return Ok(()),
                WM_HOTKEY if message.wParam.0 == SCAN as usize => {
                    hide(overlay, &mut displayed);
                    let Ok(game) =
                        FindWindowW(w!("RiotWindowClass"), w!("League of Legends (TM) Client"))
                    else {
                        continue;
                    };
                    if GetForegroundWindow() != game {
                        continue;
                    }
                    let area = game_area(game)?;
                    match scan::scan(&engine, &tiers, area) {
                        Ok(cards) if GetForegroundWindow() == game => {
                            let width = area.right - area.left;
                            let height = area.bottom - area.top;
                            let centers = scan::card_centers(width, height);
                            BADGES.set(std::array::from_fn(|i| (centers[i], cards[i])));
                            SetWindowPos(
                                overlay,
                                Some(HWND_TOPMOST),
                                area.left,
                                area.top + height * BADGE_CENTER_Y_PERCENT / 100 - BADGE_HEIGHT / 2,
                                width,
                                BADGE_HEIGHT,
                                SWP_NOACTIVATE | SWP_SHOWWINDOW,
                            )?;
                            let _ = InvalidateRect(Some(overlay), None, false);
                            displayed = Some((game, Instant::now()));
                            SetTimer(Some(overlay), 1, 200, None);
                        }
                        Ok(_) => {}
                        Err(error) => eprintln!("Could not read card titles: {error}"),
                    }
                }
                WM_TIMER if message.hwnd == overlay => {
                    if displayed.is_some_and(|(game, shown)| {
                        GetForegroundWindow() != game || shown.elapsed() >= Duration::from_secs(6)
                    }) {
                        hide(overlay, &mut displayed);
                    }
                }
                _ => {
                    DispatchMessageW(&message);
                }
            }
        }
    }
}

unsafe fn hide(overlay: HWND, displayed: &mut Option<(HWND, Instant)>) {
    if displayed.take().is_some() {
        unsafe {
            let _ = ShowWindow(overlay, SW_HIDE);
            let _ = KillTimer(Some(overlay), 1);
        }
    }
}

unsafe fn game_area(game: HWND) -> Result<RECT> {
    let mut area = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        GetClientRect(game, &mut area)?;
        ClientToScreen(game, &mut origin).ok()?;
    }
    area.left += origin.x;
    area.right += origin.x;
    area.top += origin.y;
    area.bottom += origin.y;
    Ok(area)
}

unsafe fn create_overlay() -> Result<HWND> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("MayhemTiersOverlay");
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&descriptor) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let overlay = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class,
            w!("Minimal Mayhem Overlay"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        )?;
        SetLayeredWindowAttributes(overlay, COLOR_KEY, 0, LWA_COLORKEY)?;
        Ok(overlay)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg != WM_PAINT {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        fill(dc, &paint.rcPaint, COLOR_KEY);
        SetBkMode(dc, TRANSPARENT);
        let font = CreateFontW(
            -16,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let previous = SelectObject(dc, font.into());
        let badges = BADGES.get();
        let half_width = ((badges[1].0 - badges[0].0 - 8) / 2).clamp(1, 161);
        for (center, stats) in badges {
            let (grade, mut label) = match stats {
                Some(stats) => (
                    stats.tier,
                    badge_label(stats).encode_utf16().collect::<Vec<_>>(),
                ),
                None => ('?', Vec::new()),
            };
            let half_width = if stats.is_some() {
                half_width
            } else {
                half_width.min(21)
            };
            let mut rect = RECT {
                left: center - half_width,
                right: center + half_width,
                top: 0,
                bottom: BADGE_HEIGHT,
            };
            let accent = COLORREF(match grade {
                'S' => 0x007fe3ff,
                'A' => 0x00f1b8d9,
                'B' => 0x00ffcea1,
                'C' => 0x00d0d0a3,
                _ => 0x00c0b7b0,
            });
            rounded_plate(dc, &rect, accent);
            let mut tier_rect = rect;
            if stats.is_some() && half_width >= 40 {
                tier_rect.right = rect.left + 42;
                rect.left = tier_rect.right;
                rect.right -= 9;
            } else {
                label.clear();
            }
            SetTextColor(dc, accent);
            DrawTextW(
                dc,
                &mut [grade as u16],
                &mut tier_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            if !label.is_empty() {
                SetTextColor(dc, COLORREF(0x00f5f2ed));
                DrawTextW(
                    dc,
                    &mut label,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
        }
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        let _ = EndPaint(hwnd, &paint);
        LRESULT(0)
    }
}

fn badge_label(stats: tiers::CardStats) -> String {
    use std::fmt::Write;

    let mut label = format!(
        "{:.2}% pick  ·  #{}/{}",
        stats.pick_rate, stats.rarity_rank, stats.rarity_count
    );
    if stats.new_in_data {
        label.push_str("  ·  New in data");
    } else if let Some(change) = stats.pick_change {
        let _ = write!(label, "  ·  {change:+.2} pp");
    }
    label
}

unsafe fn rounded_plate(dc: HDC, rect: &RECT, accent: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, accent);
        let brush = CreateSolidBrush(COLORREF(0x00221b13));
        let old_pen = SelectObject(dc, pen.into());
        let old_brush = SelectObject(dc, brush.into());
        let _ = RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, 22, 22);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

unsafe fn fill(dc: HDC, rect: &RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, rect, brush);
        let _ = DeleteObject(brush.into());
    }
}
