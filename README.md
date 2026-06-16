# ClipPoster

A desktop app (Windows, built with Tauri + Vue) that schedules and uploads short video clips
to TikTok, YouTube Shorts, and Instagram Reels automatically.

Organize clips into folders, set a date/time per clip, and let the app handle the uploads
through each platform's official API at the scheduled time.

## Features

- **Scheduled uploads** — set a date and time per clip; uploads fire automatically.
- **Clip management** — organize clips into folders, see what's posted and what's pending.
- **AI caption suggestions** — optional caption + hashtag generation (Gemini / Groq).
- **Upload status** — every scheduled clip is tracked; failures are flagged with the reason.

## How it works

ClipPoster is the desktop client. It talks to a self-hosted scheduler server (a small Rust
service you run yourself) that holds the videos and calls the platform APIs at the scheduled
time. You configure your own server URL, API key, and platform OAuth credentials in **Settings**.

The app ships with **no credentials baked in** — everything is entered by you at runtime and
stored locally on your machine.

## Build from source

```bash
npm install
npm run tauri build
```

Requires Node.js and the Rust toolchain (for Tauri).

> **Note:** The build bundles `ffmpeg.exe` and `ffprobe.exe` for local clip processing.
> These are not committed to the repo (they exceed GitHub's file-size limit). Download them
> from [ffmpeg.org](https://ffmpeg.org/download.html) and place both in `src-tauri/externalBin/`
> before building.

## Disclaimer

This software is provided **"as is"**, without warranty of any kind, for personal use.
It is **not affiliated with, endorsed by, or sponsored by** TikTok, YouTube, Instagram, or
their parent companies. You are responsible for complying with each platform's Terms of
Service and API usage policies. The author accepts **no liability** and provides **no support**.

## License

[MIT](LICENSE)
