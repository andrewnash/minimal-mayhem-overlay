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

The bundled [patch snapshot](data/augments.json) groups augments by rarity and splits each
group into five bands by overall pick-rate rank. These badges measure pick
popularity across champions. They use a separate grading formula from OP.GG.

[Data: arammayhem.com](https://arammayhem.com/data/) ([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)).
The bundled snapshot retains the English name, rarity, and pick rate from each
active augment row with a published pick rate.

## Refresh patch data

Once arammayhem.com publishes the new patch export, run these commands from the
repository root using PowerShell 7 and Rust:

```powershell
pwsh -NoProfile -File scripts/update-augments.ps1
cargo build --release --locked
```

Pass `-Patch 26.22` to select a specific patch and confirm the reported patch
and stats date before sharing the rebuilt executable. The script replaces the
bundled snapshot only after validating the export. The app embeds the snapshot
at build time and grades it offline.

Initial title band and card spacing informed by [Hexsight's OCR implementation](https://github.com/bard58/hexsight/blob/v0.1.2/src/ocr.rs).

## License

The code is licensed under [Beerware, Revision 42](LICENSE). The bundled
augment data is licensed separately under CC BY 4.0, as credited above.
