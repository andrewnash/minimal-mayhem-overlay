# Minimal Mayhem Overlay

A native Rust overlay for ARAM Mayhem on Windows. Press **F8** while an augment
offer is visible to read its three title regions once and display **S–D** badges.
Press **Ctrl+Shift+F8** to quit. The badges are click-through and hide after six
seconds or when League loses focus. Press F8 again after a reroll.

## Run

1. Set League's window mode to **Borderless** and its text language to **English**.
2. Run `cargo run --release` or launch `target/release/minimal-mayhem-overlay.exe`.
3. Open an ARAM Mayhem augment offer and press F8. A **?** means a card title
   could not be matched confidently.

The prototype assumes the default offer layout. It uses Windows' built-in en-US
OCR. Run `minimal-mayhem-overlay.exe --check-ocr` to check that the OCR language is
available. Screen crops are processed in memory. The app waits for Windows
messages while idle and makes no network requests.

## How tiers are calculated

The bundled patch 26.22 snapshot groups augments by rarity and splits each
group into five bands by overall pick-rate rank. These badges measure pick
popularity across champions. They use a separate grading formula from OP.GG.

[Data: arammayhem.com](https://arammayhem.com/data/) ([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)).
The bundled snapshot retains the English name, rarity, and pick rate from each
active augment row.

Initial title band and card spacing informed by [Hexsight's OCR implementation](https://github.com/bard58/hexsight/blob/v0.1.2/src/ocr.rs).
