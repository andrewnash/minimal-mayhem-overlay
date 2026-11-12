mod builds;
mod icons;
mod live;
mod scan;
mod tiers;

use std::{
    cell::{Cell, RefCell},
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
                MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, VK_F8, VK_J, VK_M,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{Result, w},
};

const SCAN: i32 = 1;
const QUIT: i32 = 2;
const DETAILS: i32 = 3;
const TOGGLE_BUILDS: i32 = 4;
const SHOW_BUILD: i32 = 5;
const SCAN_ALTERNATE: i32 = 6;
const BUILD_WIDTH: i32 = 446;
const BUILD_HEIGHT: i32 = 144;
const BADGE_HEIGHT: i32 = 34;
const HINT_HEIGHT: i32 = 24;
const HINT_TOP: i32 = BADGE_HEIGHT + 3;
const CHAMPION_TOP: i32 = HINT_HEIGHT + 3;
const HEADER_HEIGHT: i32 = CHAMPION_TOP + HINT_HEIGHT + 3;
const BADGE_CENTER_Y_PERCENT: i32 = 71;
const COLOR_KEY: COLORREF = COLORREF(0x00ff00ff);

thread_local! {
    static BADGES: RefCell<[Badge; 3]> = RefCell::default();
    static HEADER: RefCell<Option<Vec<u16>>> = const { RefCell::new(None) };
    static SHOW_BUILDS: Cell<bool> = const { Cell::new(true) };
    static BUILD_VIEW: Cell<bool> = const { Cell::new(false) };
    static BUILD_ROWS: RefCell<Option<[BuildLine; 5]>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Badge {
    center: i32,
    stats: Option<tiers::CardStats>,
    hint: Option<Vec<u16>>,
    build_hint: Option<BuildLine>,
    champion_pick: Option<Vec<u16>>,
}

struct BuildLine {
    text: Vec<u16>,
    icons: [Option<icons::Icon>; 2],
}

impl BuildLine {
    fn new(label: String, items: &[&str]) -> Self {
        Self {
            text: label.encode_utf16().collect(),
            icons: std::array::from_fn(|i| items.get(i).and_then(|name| icons::find(name))),
        }
    }
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
    let champion_override = std::env::args()
        .skip_while(|arg| arg != "--champion")
        .nth(1)
        .map(|name| name.to_ascii_lowercase());
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        let overlay = create_overlay()?;
        let primary_scan = RegisterHotKey(None, SCAN, MOD_NOREPEAT, VK_F8.0 as u32);
        let alternate_scan = [
            ("Ctrl+Alt+M", MOD_ALT, VK_M),
            ("Ctrl+Shift+M", MOD_SHIFT, VK_M),
            ("Ctrl+Alt+J", MOD_ALT, VK_J),
        ]
        .into_iter()
        .find_map(|(label, modifier, key)| {
            RegisterHotKey(
                None,
                SCAN_ALTERNATE,
                MOD_NOREPEAT | MOD_CONTROL | modifier,
                key.0 as u32,
            )
            .ok()
            .map(|()| label)
        });
        let scan_keys = match (primary_scan, alternate_scan) {
            (Ok(()), Some(label)) => format!("F8 or {label}"),
            (Ok(()), None) => "F8".into(),
            (Err(_), Some(label)) => label.into(),
            (Err(error), None) => {
                eprintln!("None of the scan shortcuts could be registered.");
                return Err(error);
            }
        };
        RegisterHotKey(None, DETAILS, MOD_NOREPEAT | MOD_SHIFT, VK_F8.0 as u32)?;
        RegisterHotKey(None, SHOW_BUILD, MOD_NOREPEAT | MOD_ALT, VK_F8.0 as u32)?;
        RegisterHotKey(
            None,
            TOGGLE_BUILDS,
            MOD_NOREPEAT | MOD_CONTROL,
            VK_F8.0 as u32,
        )?;
        RegisterHotKey(
            None,
            QUIT,
            MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT,
            VK_F8.0 as u32,
        )?;
        println!(
            "Minimal Mayhem Overlay | patch {} | {scan_keys} scan | Shift+F8 details | Alt+F8 build | Ctrl+F8 toggle builds | Ctrl+Shift+F8 quit",
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
                WM_HOTKEY if message.wParam.0 == TOGGLE_BUILDS as usize => {
                    let show = !SHOW_BUILDS.get();
                    SHOW_BUILDS.set(show);
                    if !show && BUILD_VIEW.get() {
                        hide(overlay, &mut displayed);
                    } else {
                        let _ = InvalidateRect(Some(overlay), None, false);
                    }
                    println!("Build options {}", if show { "on" } else { "off" });
                }
                WM_HOTKEY if message.wParam.0 == SHOW_BUILD as usize => {
                    if !SHOW_BUILDS.get() {
                        continue;
                    }
                    let Some(game) = foreground_game() else {
                        continue;
                    };
                    let champion = champion_override
                        .as_deref()
                        .or_else(|| live::champion(&data));
                    let Some(rows) = champion.and_then(|id| build_rows(&data, id)) else {
                        println!(
                            "Champion unavailable. Try again in game or launch with --champion <Riot champion key>."
                        );
                        continue;
                    };
                    if GetForegroundWindow() != game {
                        continue;
                    }
                    let area = game_area(game)?;
                    let width = BUILD_WIDTH.min(area.right - area.left - 32);
                    if width <= 0 || area.bottom - area.top < BUILD_HEIGHT + 32 {
                        continue;
                    }
                    hide(overlay, &mut displayed);
                    BUILD_ROWS.replace(Some(rows));
                    BUILD_VIEW.set(true);
                    SetWindowPos(
                        overlay,
                        Some(HWND_TOPMOST),
                        area.right - width - 16,
                        area.top + 16,
                        width,
                        BUILD_HEIGHT,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    )?;
                    let _ = InvalidateRect(Some(overlay), None, false);
                    displayed = Some((game, Instant::now()));
                    SetTimer(Some(overlay), 1, 200, None);
                }
                WM_HOTKEY if matches!(message.wParam.0 as i32, SCAN | SCAN_ALTERNATE | DETAILS) => {
                    hide(overlay, &mut displayed);
                    let Some(game) = foreground_game() else {
                        eprintln!("Scan skipped: focus the League game window first.");
                        continue;
                    };
                    BUILD_VIEW.set(false);
                    let area = match game_area(game) {
                        Ok(area) => area,
                        Err(error) => {
                            eprintln!("Could not locate the League game area: {error}");
                            continue;
                        }
                    };
                    let width = area.right - area.left;
                    let height = area.bottom - area.top;
                    println!("Scanning augment cards in the {width}x{height} game window.");
                    match scan::scan(&engine, &tiers, area) {
                        Ok(cards) if GetForegroundWindow() == game => {
                            let recognized = cards.iter().flatten().count();
                            println!("Recognized {recognized}/3 augment titles.");
                            let champion = champion_override.as_deref().or_else(|| {
                                (recognized > 0).then(|| live::champion(&data)).flatten()
                            });
                            if GetForegroundWindow() != game {
                                continue;
                            }
                            let champion_picks = champion
                                .and_then(|name| tiers::ChampionPicks::load(&data.patch, name));
                            let centers = scan::card_centers(width, height);
                            let details = message.wParam.0 == DETAILS as usize;
                            HEADER.replace(details.then(|| {
                                let mut label = format!("Patch {}", data.patch);
                                if let Some(date) = &data.stats_date {
                                    label.push_str("  ·  Stats ");
                                    label.push_str(date);
                                }
                                label.encode_utf16().collect()
                            }));
                            BADGES.replace(std::array::from_fn(|i| Badge {
                                center: centers[i],
                                stats: cards[i],
                                build_hint: champion_picks
                                    .as_ref()
                                    .filter(|_| !details)
                                    .zip(cards[i])
                                    .and_then(|(picks, card)| picks.weapon_items(&data, card))
                                    .map(|items| {
                                        let names: [&str; 2] = std::array::from_fn(|i| {
                                            items.get(i).map_or("", String::as_str)
                                        });
                                        BuildLine::new(items.join(" / "), &names)
                                    }),
                                champion_pick: champion_picks
                                    .as_ref()
                                    .zip(cards[i])
                                    .and_then(|(picks, card)| picks.match_share(&data, card))
                                    .map(|(share, count)| {
                                        format!("CN {share:.2}% match share  ·  {count} picks")
                                            .encode_utf16()
                                            .collect()
                                    }),
                                hint: cards[i]
                                    .and_then(|card| {
                                        if details {
                                            card.rank_change.map(|change| {
                                                format!("Global rank move {change:+}")
                                            })
                                        } else {
                                            champion
                                                .and_then(|name| data.combo_hint(name, card))
                                                .map(|partner| format!("Combo with {partner}"))
                                        }
                                    })
                                    .map(|label| label.encode_utf16().collect()),
                            }));
                            SetWindowPos(
                                overlay,
                                Some(HWND_TOPMOST),
                                area.left,
                                area.top + height * BADGE_CENTER_Y_PERCENT / 100
                                    - BADGE_HEIGHT / 2
                                    - HEADER_HEIGHT,
                                width,
                                HEADER_HEIGHT + HINT_TOP + HINT_HEIGHT,
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

fn build_rows(data: &tiers::Dataset, id: &str) -> Option<[BuildLine; 5]> {
    let champion = data.champions.get(id)?;
    let build = builds::for_champion(id, &champion.role)?;
    Some([
        BuildLine::new(format!("{}  ·  {}", champion.name, build.name), &[]),
        BuildLine::new(
            format!("Core: {} → {}", build.core[0], build.core[1]),
            &build.core[..2],
        ),
        BuildLine::new(
            format!("Then consider: {}", build.core[2]),
            &build.core[2..],
        ),
        BuildLine::new(
            format!("Options: {} / {}", build.options[0], build.options[1]),
            &build.options,
        ),
        BuildLine::new(
            format!("Curated item options  ·  Patch {}", data.patch),
            &[],
        ),
    ])
}

fn foreground_game() -> Option<HWND> {
    unsafe {
        let game = FindWindowW(w!("RiotWindowClass"), w!("League of Legends (TM) Client")).ok()?;
        (GetForegroundWindow() == game).then_some(game)
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
        if BUILD_VIEW.get() {
            let mut area = RECT::default();
            if GetClientRect(hwnd, &mut area).is_ok() {
                BUILD_ROWS.with_borrow_mut(|rows| {
                    if let Some(rows) = rows {
                        draw_build(dc, &area, rows);
                    }
                });
            }
        } else {
            BADGES.with_borrow_mut(|badges| {
                let half_width = ((badges[1].center - badges[0].center - 8) / 2).clamp(1, 161);
                HEADER.with_borrow_mut(|header| {
                    if let Some(label) = header {
                        draw_chip(dc, label, badges[1].center, half_width, 0);
                    }
                });
                for badge in badges {
                    let Badge {
                        center,
                        stats,
                        hint,
                        build_hint,
                        champion_pick,
                    } = badge;
                    let (center, stats) = (*center, *stats);
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
                        top: HEADER_HEIGHT,
                        bottom: HEADER_HEIGHT + BADGE_HEIGHT,
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
                    let build = build_hint.as_mut().filter(|_| SHOW_BUILDS.get());
                    if let Some(build) = build {
                        draw_item_chip(
                            dc,
                            &mut build.text,
                            center,
                            half_width,
                            HEADER_HEIGHT + HINT_TOP,
                            build.icons,
                        );
                    } else if let Some(hint) = hint {
                        draw_chip(dc, hint, center, half_width, HEADER_HEIGHT + HINT_TOP);
                    }
                    if let Some(pick) = champion_pick {
                        draw_chip(dc, pick, center, half_width, CHAMPION_TOP);
                    }
                }
            });
        }
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        let _ = EndPaint(hwnd, &paint);
        LRESULT(0)
    }
}

unsafe fn draw_build(dc: HDC, area: &RECT, rows: &mut [BuildLine; 5]) {
    unsafe {
        rounded_plate(dc, area, COLORREF(0x00645748));
        for (index, row) in rows.iter_mut().enumerate() {
            let top = 12 + index as i32 * 24;
            let mut rect = RECT {
                left: 16,
                right: area.right - 16,
                top,
                bottom: top + 24,
            };
            SetTextColor(
                dc,
                COLORREF(match index {
                    0 => 0x007fe3ff,
                    4 => 0x00b8aca3,
                    _ => 0x00f5f2ed,
                }),
            );
            draw_row_icons(dc, &mut rect, row.icons);
            DrawTextW(
                dc,
                &mut row.text,
                &mut rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
    }
}

unsafe fn draw_chip(dc: HDC, label: &mut [u16], center: i32, half_width: i32, top: i32) {
    unsafe { draw_item_chip(dc, label, center, half_width, top, [None; 2]) }
}

unsafe fn draw_item_chip(
    dc: HDC,
    label: &mut [u16],
    center: i32,
    half_width: i32,
    top: i32,
    icons: [Option<icons::Icon>; 2],
) {
    unsafe {
        let mut rect = RECT {
            left: center - half_width,
            right: center + half_width,
            top,
            bottom: top + HINT_HEIGHT,
        };
        rounded_plate(dc, &rect, COLORREF(0x00645748));
        rect.left += 7;
        rect.right -= 7;
        draw_row_icons(dc, &mut rect, icons);
        SetTextColor(dc, COLORREF(0x00f5f2ed));
        DrawTextW(
            dc,
            label,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

fn draw_row_icons(dc: HDC, rect: &mut RECT, icons: [Option<icons::Icon>; 2]) {
    if rect.right - rect.left < 100 {
        return;
    }
    let y = rect.top + (rect.bottom - rect.top - icons::SIZE) / 2;
    for icon in icons.into_iter().flatten() {
        icons::draw(dc, rect.left, y, icon);
        rect.left += icons::SIZE + 4;
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
