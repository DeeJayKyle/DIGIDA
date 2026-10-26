<div align="center">

<img src="./.github/readme-icon.svg" alt="" width="60" height="60">

<h1>DIGIDA</h1>

[![Release](https://img.shields.io/github/v/release/DeeJayKyle/DIGIDA?style=flat-square&label=release)](https://github.com/DeeJayKyle/DIGIDA/releases/latest)
[![License](https://img.shields.io/github/license/DeeJayKyle/DIGIDA?style=flat-square&label=license&cacheSeconds=3600)](./COPYING)

### A native music streaming client, dressed in the material you choose

Stream from your favourite services and play local files, all in one **native** app, drawn in
glass, clay, real-world materials or soft-pressed neumorphism.
</div>

> [!IMPORTANT]
> **DIGIDA is not a piracy tool.**
>
> DIGIDA is not a platform for obtaining or sharing copyrighted material. It will not implement any functions that can be used to export decrypted streams, DRM licenses, content keys, or to convert protected streams into media files.
>
> DIGIDA is not designed to circumvent subscriptions or other restrictions put in place by music streaming platforms. If the service demands that you have a valid subscription in order to play back their tracks, so will DIGIDA.

## Styles

Pick a style under **Settings → Appearance → Style**. The palette fades over and the window
settles into the new material, and every style works with the light, dark, system, adaptive
and custom themes.

| Style | What it looks like |
| --- | --- |
| **Glass** | Frosted panes over a slowly drifting aurora, edged with light. The aurora takes the cover's colours when the adaptive theme is on. |
| **Clay** | Soft, puffy pastel surfaces with a lit top, a shaded underside and generous rounding. |
| **Skeuomorphic** | Lit gradients, bevelled edges, crisp contact shadows and a paper grain over the page. |
| **Neumorphic** | One continuous material: controls push out of it on hover and press into it when chosen. |

## Features

- **Apple Music, Spotify, YouTube Music, Deezer, Tidal, Subsonic/Navidrome** and local playback
- Four switchable UI styles: glassmorphism, claymorphism, skeuomorphism and neumorphism
- Gapless playback, audio normalization, shuffle, sleep timer
- Synced/karaoke lyrics, background vocals, and romanization
- Scrobbling with LastFM, ListenBrainz, LibreFM, and Maloja
- Themes, fonts, icons, transparency, blur, and window styling
- Discord Rich Presence, native file opening

## Installation

### Windows

Download and run the [installer](https://github.com/DeeJayKyle/DIGIDA/releases/latest/download/DIGIDA-Setup.exe),
or the [ARM installer](https://github.com/DeeJayKyle/DIGIDA/releases/latest/download/DIGIDA-Setup-arm64.exe)
on Windows on ARM. A portable `digida-*-windows-msvc.exe` for each architecture is published
beside them on the [releases page](https://github.com/DeeJayKyle/DIGIDA/releases/latest).

DIGIDA checks GitHub for a newer release at startup and can install it itself on Windows.

### Android

There is no Android build yet. DIGIDA is drawn with [GPUI](https://github.com/zed-industries/zed),
which has no Android backend, so a phone build needs the interface ported to a toolkit that runs
there first.

### From source

```sh
cargo run --locked --package sonora
```

The package keeps its upstream name; the executable it builds is `digida`. On Linux, mold and a
C++ compiler are needed, see `CLAUDE.md` for the full set of checks.

## Translations

<!-- i18n:start -->

| Language | Translated | Coverage |
| --- | --- | --- |
| English (`en-US`) | 1120/1120 | 100% |
| Deutsch (`de`) | 1120/1120 | 100% |
| Español (`es`) | 1120/1120 | 100% |
| Français (`fr`) | 1120/1120 | 100% |
| Italiano (`it`) | 1120/1120 | 100% |
| Bahasa Indonesia (`id`) | 1120/1120 | 100% |
| 日本語 (`ja`) | 1120/1120 | 100% |
| Русский (`ru`) | 1120/1120 | 100% |
| Українська (`uk`) | 1120/1120 | 100% |
| Polski (`pl`) | 1120/1120 | 100% |
| Čeština (`cs`) | 1120/1120 | 100% |
| Português (Brasil) (`pt-BR`) | 1120/1120 | 100% |
| 简体中文 (`zh-CN`) | 1120/1120 | 100% |
| Türkçe (`tr`) | 1120/1120 | 100% |
| Shqip (`sq`) | 1120/1120 | 100% |

<!-- i18n:end -->

## Credits

DIGIDA is a fork of [Sonora](https://github.com/sonorahq/sonora), and is built with the help of
some incredible open-source projects, including:

- [Zed](https://github.com/zed-industries/zed), which provides `gpui`, its native Rust rendering stack.
- [librespot](https://github.com/librespot-org/librespot), for Spotify playback and library integration.
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), whose ideas live on in [ytmusic-rs](https://github.com/sonorahq/ytmusic-rs).

## License

Copyright (C) 2026 DIGIDA Contributors.
Based on Sonora, Copyright (C) 2026 Sonora Contributors.

DIGIDA is free software, released under the [GNU General Public License version
3 or later](COPYING).

DIGIDA is an unofficial client and is not affiliated with, endorsed by, or
sponsored by Spotify AB.

The binary also embeds the [Inter](https://github.com/rsms/inter) typeface (SIL
Open Font License 1.1) and four interchangeable icon sets:
[Lucide](https://lucide.dev) (ISC), [Iconoir](https://iconoir.com) (MIT),
[Remix Icon](https://remixicon.com) 4.8.0 (Apache 2.0) and the
[Solar](https://www.figma.com/community/file/1166831539721848736) Linear set
(CC BY 4.0, by 480 Design). Each pack keeps its licence beside its files in
`assets/icons`. `THIRD-PARTY.md` lists every bundled dependency.
