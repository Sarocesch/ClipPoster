use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};
use tauri::{Manager, Emitter};
use std::{thread, time::Duration};
use once_cell::sync::Lazy;
use tokio::sync::Mutex as TokioMutex;

static CLIPS_SERVER_HANDLE: Lazy<TokioMutex<Option<tokio::task::JoinHandle<()>>>> =
    Lazy::new(|| TokioMutex::new(None));

fn resolve_ffmpeg(app: &tauri::AppHandle, user: Option<String>) -> PathBuf {
    if let Some(u) = user {
        let p = PathBuf::from(&u);
        if p.exists() {
            if p.file_name().and_then(|n| n.to_str()).map(|n| n.eq_ignore_ascii_case("ffmpeg.exe")).unwrap_or(false) {
                return p;
            }
        }
    }
    if let Ok(res_dir) = app.path().resource_dir() {
        // Production: bundled in externalBin/ subfolder
        let p = res_dir.join("externalBin").join("ffmpeg.exe");
        if p.exists() { return p; }
        // Directly in resource dir
        let p2 = res_dir.join("ffmpeg.exe");
        if p2.exists() { return p2; }
    }
    // Dev: externalBin relative to working dir
    let dev = PathBuf::from("externalBin\\ffmpeg.exe");
    if dev.exists() { return dev; }
    PathBuf::from("ffmpeg.exe")
}

// ─────────────────────────────────────────────────────────────────────────────
// CLIPS COMMANDS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ClipInfo {
    name: String,
    rel_path: String,
    has_description: bool,
    has_schedule: bool,
    is_done: bool,          // legacy: true wenn alle 3 platform-flags gesetzt ODER altes .done existiert
    done_yt: bool,
    done_tt: bool,
    done_ig: bool,
    done_pt: bool,
    done_sc: bool,
    size_bytes: u64,
}

// Helper: liest platform-done status mit legacy-fallback
fn read_platform_done(clip_path: &Path) -> (bool, bool, bool, bool, bool, bool) {
    let base = clip_path.to_string_lossy().to_string();
    let legacy_done = Path::new(&format!("{}.done", base)).exists();
    let yt = legacy_done || Path::new(&format!("{}.done.yt", base)).exists();
    let tt = legacy_done || Path::new(&format!("{}.done.tt", base)).exists();
    let ig = legacy_done || Path::new(&format!("{}.done.ig", base)).exists();
    let pt = Path::new(&format!("{}.done.pt", base)).exists();
    let sc = Path::new(&format!("{}.done.sc", base)).exists();
    let all_done = yt && tt && ig;
    (all_done, yt, tt, ig, pt, sc)
}

fn is_video_ext(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm")
}

#[tauri::command]
fn list_clip_folders(root: String) -> Result<Vec<String>, String> {
    let p = Path::new(&root);
    if !p.exists() { return Err("Clips-Root existiert nicht".into()); }
    let mut out = vec![];
    let mut entries: Vec<_> = fs::read_dir(p).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        out.push(e.file_name().to_string_lossy().to_string());
    }
    Ok(out)
}

#[tauri::command]
fn create_clip_folder(root: String, name: String) -> Result<(), String> {
    fs::create_dir_all(Path::new(&root).join(&name)).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_clips(folder: String) -> Result<Vec<ClipInfo>, String> {
    let p = Path::new(&folder);
    if !p.exists() { return Err("Ordner existiert nicht".into()); }
    let mut out = vec![];
    let mut entries: Vec<_> = fs::read_dir(p).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|t| t.is_file()).unwrap_or(false)
                && e.path().extension()
                    .and_then(|x| x.to_str())
                    .map(is_video_ext)
                    .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let txt_path = {
            let mut t = path.clone();
            t.set_extension(path.extension().map(|x| format!("{}.txt", x.to_string_lossy())).unwrap_or_default());
            t
        };
        let schedule_path = {
            let mut t = path.clone();
            t.set_extension(path.extension().map(|x| format!("{}.schedule", x.to_string_lossy())).unwrap_or_default());
            t
        };
        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let (all_done, done_yt, done_tt, done_ig, done_pt, done_sc) = read_platform_done(&path);
        out.push(ClipInfo {
            rel_path: path.to_string_lossy().to_string(),
            name,
            has_description: txt_path.exists(),
            has_schedule: schedule_path.exists(),
            is_done: all_done,
            done_yt,
            done_tt,
            done_ig,
            done_pt,
            done_sc,
            size_bytes,
        });
    }
    Ok(out)
}

#[tauri::command]
fn get_clip_description(clip_path: String) -> Result<String, String> {
    let txt = format!("{}.txt", clip_path);
    if Path::new(&txt).exists() {
        fs::read_to_string(&txt).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
fn save_clip_description(clip_path: String, text: String) -> Result<(), String> {
    let txt = format!("{}.txt", clip_path);
    fs::write(&txt, text).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_clip_schedule(clip_path: String) -> Result<String, String> {
    let sch = format!("{}.schedule", clip_path);
    if Path::new(&sch).exists() {
        fs::read_to_string(&sch).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
fn save_clip_schedule(clip_path: String, datetime: String) -> Result<(), String> {
    let sch = format!("{}.schedule", clip_path);
    if datetime.trim().is_empty() {
        if Path::new(&sch).exists() {
            fs::remove_file(&sch).map_err(|e| e.to_string())?;
        }
    } else {
        fs::write(&sch, datetime.trim()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct ScheduleConflict {
    folder: String,
    clip: String,
    scheduled_at: String,
    rel_path: String,
}

#[tauri::command]
fn find_clips_on_same_date(root: String, date: String, exclude: String) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    let Ok(folders) = fs::read_dir(&root) else { return conflicts; };
    for folder_entry in folders.flatten() {
        if !folder_entry.path().is_dir() { continue; }
        let folder_name = folder_entry.file_name().to_string_lossy().to_string();
        let Ok(clips) = fs::read_dir(folder_entry.path()) else { continue; };
        for clip_entry in clips.flatten() {
            let clip_path = clip_entry.path();
            let ext = clip_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "mp4" | "mov" | "mkv" | "avi" | "webm") { continue; }
            let rel_path = format!("{}/{}", folder_name, clip_entry.file_name().to_string_lossy());
            if rel_path == exclude { continue; }
            let sch = format!("{}.schedule", clip_path.to_string_lossy());
            let Ok(schedule) = fs::read_to_string(&sch) else { continue; };
            let schedule = schedule.trim().to_string();
            if schedule.starts_with(&date) {
                conflicts.push(ScheduleConflict {
                    folder: folder_name.clone(),
                    clip: clip_entry.file_name().to_string_lossy().to_string(),
                    scheduled_at: schedule,
                    rel_path,
                });
            }
        }
    }
    conflicts
}

#[tauri::command]
fn toggle_clip_done(clip_path: String) -> Result<bool, String> {
    let done = format!("{}.done", clip_path);
    if Path::new(&done).exists() {
        fs::remove_file(&done).map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        fs::write(&done, "").map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
fn toggle_clip_done_platform(clip_path: String, platform: String) -> Result<bool, String> {
    let suffix = match platform.as_str() {
        "yt" | "youtube"              => "yt",
        "tt" | "tiktok"               => "tt",
        "ig" | "instagram" | "insta"  => "ig",
        "pt" | "pinterest"            => "pt",
        "sc" | "snapchat"             => "sc",
        _ => return Err("Unbekannte Plattform".into()),
    };
    let flag = format!("{}.done.{}", clip_path, suffix);
    let new_state = if Path::new(&flag).exists() {
        fs::remove_file(&flag).map_err(|e| e.to_string())?;
        false
    } else {
        fs::write(&flag, "").map_err(|e| e.to_string())?;
        true
    };
    // Legacy .done File bei erstem Platform-Toggle löschen,
    // damit die drei Flags zur Wahrheit werden
    let legacy = format!("{}.done", clip_path);
    if Path::new(&legacy).exists() {
        let _ = fs::remove_file(&legacy);
    }
    Ok(new_state)
}

#[tauri::command]
fn get_clip_platform_status(clip_path: String) -> Result<serde_json::Value, String> {
    let p = Path::new(&clip_path);
    let (_, yt, tt, ig, pt, sc) = read_platform_done(p);
    Ok(serde_json::json!({ "yt": yt, "tt": tt, "ig": ig, "pt": pt, "sc": sc }))
}

fn caption_path(clip_path: &str, platform: &str) -> Option<String> {
    let suffix = match platform {
        "yt" | "youtube" => "yt",
        "tt" | "tiktok" => "tt",
        "ig" | "instagram" | "insta" => "ig",
        _ => return None,
    };
    Some(format!("{}.{}.txt", clip_path, suffix))
}

#[tauri::command]
fn get_clip_caption(clip_path: String, platform: String) -> Result<String, String> {
    if let Some(pf) = caption_path(&clip_path, &platform) {
        if Path::new(&pf).exists() {
            return fs::read_to_string(&pf).map_err(|e| e.to_string());
        }
    }
    // Fallback: generische .txt
    let generic = format!("{}.txt", clip_path);
    if Path::new(&generic).exists() {
        return fs::read_to_string(&generic).map_err(|e| e.to_string());
    }
    Ok(String::new())
}

#[tauri::command]
fn save_clip_caption(clip_path: String, platform: String, text: String) -> Result<(), String> {
    let pf = caption_path(&clip_path, &platform).ok_or("Unbekannte Plattform")?;
    if text.trim().is_empty() {
        if Path::new(&pf).exists() { let _ = fs::remove_file(&pf); }
    } else {
        fs::write(&pf, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn copy_file_to_clipboard(path: String) -> Result<(), String> {
    use clipboard_win::{Clipboard, Setter};
    let _clip = Clipboard::new_attempts(10).map_err(|e| format!("Clipboard öffnen fehlgeschlagen: {:?}", e))?;
    clipboard_win::formats::FileList.write_clipboard(&[path]).map_err(|e| format!("{:?}", e))
}

#[tauri::command]
fn copy_clip_to_folder(src: String, dest_folder: String) -> Result<String, String> {
    let src_path = Path::new(&src);
    if !src_path.exists() { return Err("Quelldatei nicht gefunden".into()); }
    let file_name = src_path.file_name().ok_or("Ungültiger Dateiname")?;
    let dest = Path::new(&dest_folder).join(file_name);
    fs::copy(&src_path, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// PLATFORM SCHEDULING — HTTP upload to self-hosted VPS scheduler server
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct SchedulePayload {
    platform:     String,
    title:        Option<String>,
    description:  String,
    scheduled_at: String,
    api_key:      String,
    youtube_url:  Option<String>,
    #[serde(default)]
    post_now:     bool,
}

/// Splits a combined caption+hashtag string for YouTube Shorts:
/// - title:       text before the first '#', whitespace-trimmed (clip-specific text)
/// - description: from the first '#' to end (hashtags)
///
/// Example: "Amazing clip!\n#shorts #viral"
///   → title       = "Amazing clip!"
///   → description = "#shorts #viral"
fn parse_for_youtube(text: &str) -> (String, String) {
    match text.find('#') {
        None      => (text.trim().to_string(), String::new()),
        Some(idx) => (text[..idx].trim().to_string(), text[idx..].trim().to_string()),
    }
}

/// Upload a video file + scheduling metadata to the self-hosted VPS scheduler.
/// The server queues the job and fires the platform API at the given timestamp.
#[tauri::command]
async fn schedule_clip(
    clip_path:    String,
    platform:     String,
    description:  String,
    scheduled_at: String,
    server_url:   String,
    upload_url:   Option<String>,
    api_key:      String,
    youtube_url:  Option<String>,
    post_now:     Option<bool>,
) -> Result<String, String> {
    // Use dedicated upload URL if set, otherwise fall back to server_url
    let effective_url = upload_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| server_url.clone());
    let (title, desc) = match platform.as_str() {
        "youtube" => {
            let (t, d) = parse_for_youtube(&description);
            (Some(t), d)
        }
        _ => (None, description.clone()),
    };

    let payload_json = serde_json::to_string(&SchedulePayload {
        platform: platform.clone(),
        title,
        description: desc,
        scheduled_at,
        api_key,
        youtube_url,
        post_now: post_now.unwrap_or(false),
    }).map_err(|e| e.to_string())?;

    let video_bytes = tokio::fs::read(&clip_path).await
        .map_err(|e| format!("Video lesen fehlgeschlagen: {}", e))?;

    let file_name = Path::new(&clip_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let video_part = reqwest::multipart::Part::bytes(video_bytes)
        .file_name(file_name)
        .mime_str("video/mp4")
        .map_err(|e| e.to_string())?;

    let form = reqwest::multipart::Form::new()
        .text("payload", payload_json)
        .part("video", video_part);

    let endpoint = format!("{}/api/schedule", effective_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(&endpoint)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Verbindung zu Server fehlgeschlagen: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("Server-Fehler {}: {}", status, body))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AI HASHTAG GENERATION (Gemini + Groq)
// ─────────────────────────────────────────────────────────────────────────────

async fn serve_video(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::path::Path(path): axum::extract::path::Path<String>,
) -> axum::response::Response {
    let full = format!("{}/{}", state.root, path);
    let p = std::path::Path::new(&full);
    if !p.exists() {
        return axum::response::Response::builder().status(404).body("Not found".into()).unwrap();
    }
    let mime = mime_for_video(&full);
    let data = match std::fs::read(&full) {
        Ok(d) => d,
        Err(_) => return axum::response::Response::builder().status(500).body("Read error".into()).unwrap(),
    };
    let name = p.file_name().unwrap_or_default().to_string_lossy();
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", name))
        .body(data.into())
        .unwrap()
}

fn mime_for_video(path: &str) -> &'static str {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn extract_text_from_gemini(v: &serde_json::Value) -> Option<String> {
    let candidates = v.get("candidates")?.as_array()?;
    let first = candidates.first()?;
    let parts = first.get("content")?.get("parts")?.as_array()?;
    let mut out = String::new();
    for p in parts {
        // Skip thinking tokens (gemini-2.5-flash internal reasoning)
        let is_thought = p.get("thought").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_thought { continue; }
        if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
            out.push_str(t);
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// If the clip is large (>100 MB), create a temporary 720p/3Mbps copy for Gemini analysis.
/// Gemini only needs to understand content — full quality is not needed and slows processing.
/// Returns (path_to_upload, is_temp). Caller must delete the temp file after use.
fn prepare_gemini_video(clip_path: &str) -> (String, bool) {
    let size = std::fs::metadata(clip_path).map(|m| m.len()).unwrap_or(0);
    if size < 100 * 1024 * 1024 {
        return (clip_path.to_string(), false); // small enough, use as-is
    }

    let tmp_path = format!("{}.gemini_tmp.mp4", clip_path);

    // Try to transcode with ffmpeg — fall back to original if not available
    let ffmpeg_candidates = ["ffmpeg", "ffmpeg.exe",
        "externalBin\\ffmpeg.exe"];
    for cmd in &ffmpeg_candidates {
        let status = std::process::Command::new(cmd)
            .args([
                "-i", clip_path,
                "-vf", "scale=-2:720",
                "-c:v", "libx264",
                "-crf", "28",
                "-preset", "ultrafast",
                "-maxrate", "3M",
                "-bufsize", "6M",
                "-c:a", "aac",
                "-b:a", "96k",
                "-y",
                &tmp_path,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if let Ok(s) = status {
            if s.success() && std::path::Path::new(&tmp_path).exists() {
                let tmp_size = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
                if tmp_size > 0 {
                    tracing_log(&format!("Gemini: using downscaled tmp ({} MB → {} MB)",
                        size / 1024 / 1024, tmp_size / 1024 / 1024));
                    return (tmp_path, true);
                }
            }
        }
    }

    // ffmpeg not found or failed — use original
    (clip_path.to_string(), false)
}

fn tracing_log(msg: &str) {
    eprintln!("[gemini] {msg}");
}

#[tauri::command]
fn generate_hashtags_gemini(clip_path: String, api_key: String, prompt: String) -> Result<String, String> {
    if api_key.trim().is_empty() { return Err("Gemini API-Key fehlt".into()); }
    if !Path::new(&clip_path).exists() { return Err("Clip-Datei nicht gefunden".into()); }

    let (upload_path, is_tmp) = prepare_gemini_video(&clip_path);
    let path = Path::new(&upload_path);

    let bytes = fs::read(path).map_err(|e| format!("Datei lesen: {}", e))?;
    let size = bytes.len();
    let mime = mime_for_video(&upload_path);
    let display_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("clip").to_string();

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(600))
        .timeout_write(std::time::Duration::from_secs(600))
        .build();

    // Step 1: start resumable upload → extract X-Goog-Upload-URL
    let start_body = serde_json::json!({ "file": { "display_name": display_name } });
    let start_url = format!("https://generativelanguage.googleapis.com/upload/v1beta/files?key={}", api_key);
    let start_resp = agent.post(&start_url)
        .set("X-Goog-Upload-Protocol", "resumable")
        .set("X-Goog-Upload-Command", "start")
        .set("X-Goog-Upload-Header-Content-Length", &size.to_string())
        .set("X-Goog-Upload-Header-Content-Type", mime)
        .set("Content-Type", "application/json")
        .send_string(&start_body.to_string())
        .map_err(|e| format!("Gemini Upload-Start fehlgeschlagen: {}", e))?;

    let upload_url = start_resp.header("x-goog-upload-url")
        .ok_or("Gemini hat keine Upload-URL zurückgegeben")?
        .to_string();

    // Step 2: upload bytes and finalize
    let upload_resp = agent.post(&upload_url)
        .set("Content-Length", &size.to_string())
        .set("X-Goog-Upload-Offset", "0")
        .set("X-Goog-Upload-Command", "upload, finalize")
        .send_bytes(&bytes)
        .map_err(|e| format!("Gemini Upload fehlgeschlagen: {}", e))?;

    let upload_body = upload_resp.into_string().map_err(|e| format!("Gemini Upload-Antwort lesen: {}", e))?;
    let upload_json: serde_json::Value = serde_json::from_str(&upload_body)
        .map_err(|e| format!("Gemini Upload-Antwort JSON: {} — {}", e, upload_body))?;
    let file_uri = upload_json.pointer("/file/uri").and_then(|v| v.as_str())
        .ok_or("Gemini: file.uri fehlt in Antwort")?.to_string();
    let file_name = upload_json.pointer("/file/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut state = upload_json.pointer("/file/state").and_then(|v| v.as_str()).unwrap_or("PROCESSING").to_string();

    // Step 3: poll until ACTIVE (max ~5min)
    let mut tries = 0;
    while state != "ACTIVE" && tries < 100 {
        if state == "FAILED" { return Err("Gemini: Datei-Verarbeitung fehlgeschlagen".into()); }
        thread::sleep(Duration::from_secs(3));
        let get_url = format!("https://generativelanguage.googleapis.com/v1beta/{}?key={}", file_name, api_key);
        match agent.get(&get_url).call() {
            Ok(r) => {
                if let Ok(body) = r.into_string() {
                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(s) = j.get("state").and_then(|v| v.as_str()) {
                            state = s.to_string();
                        }
                    }
                }
            }
            Err(_) => {}
        }
        tries += 1;
    }
    if state != "ACTIVE" {
        return Err(format!("Gemini: Datei nicht bereit nach {}s (state={})", tries * 3, state));
    }

    // Step 4: generateContent
    let gen_url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", api_key);
    let gen_body = serde_json::json!({
        "contents": [{
            "parts": [
                { "file_data": { "mime_type": mime, "file_uri": file_uri } },
                { "text": prompt }
            ]
        }]
    });
    let gen_resp = agent.post(&gen_url)
        .set("Content-Type", "application/json")
        .send_string(&gen_body.to_string())
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("Gemini HTTP {}: {}", code, body)
            }
            ureq::Error::Transport(t) => format!("Gemini Transport-Fehler: {}", t),
        })?;

    let gen_body = gen_resp.into_string().map_err(|e| format!("Gemini generate Antwort lesen: {}", e))?;
    let gen_json: serde_json::Value = serde_json::from_str(&gen_body)
        .map_err(|e| format!("Gemini generate JSON: {} — {}", e, gen_body))?;
    let result = extract_text_from_gemini(&gen_json)
        .ok_or_else(|| format!("Gemini: keine Text-Antwort ({})", gen_json));
    if is_tmp { let _ = std::fs::remove_file(&upload_path); }
    result
}

#[tauri::command]
fn generate_hashtags_gemini_stream(app: tauri::AppHandle, clip_path: String, api_key: String, prompt: String) -> Result<(), String> {
    if api_key.trim().is_empty() { return Err("Gemini API-Key fehlt".into()); }
    if !Path::new(&clip_path).exists() { return Err("Clip-Datei nicht gefunden".into()); }

    let (upload_path, is_tmp) = prepare_gemini_video(&clip_path);
    let path = Path::new(&upload_path);

    let emit = |text: &str, done: bool, error: Option<&str>| {
        let _ = app.emit("ai-stream", serde_json::json!({
            "text": text,
            "done": done,
            "error": error
        }));
    };

    let bytes = fs::read(path).map_err(|e| format!("Datei lesen: {}", e))?;
    let size = bytes.len();
    let mime = mime_for_video(&upload_path);
    let display_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("clip").to_string();

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(600))
        .timeout_write(std::time::Duration::from_secs(600))
        .build();

    emit("", false, None); // signal: started

    let start_body = serde_json::json!({ "file": { "display_name": display_name } });
    let start_url = format!("https://generativelanguage.googleapis.com/upload/v1beta/files?key={}", api_key);
    let start_resp = agent.post(&start_url)
        .set("X-Goog-Upload-Protocol", "resumable")
        .set("X-Goog-Upload-Command", "start")
        .set("X-Goog-Upload-Header-Content-Length", &size.to_string())
        .set("X-Goog-Upload-Header-Content-Type", mime)
        .set("Content-Type", "application/json")
        .send_string(&start_body.to_string())
        .map_err(|e| { let msg = format!("Upload-Start: {}", e); emit("", true, Some(&msg)); msg })?;

    let upload_url = start_resp.header("x-goog-upload-url")
        .ok_or_else(|| { let m = "Gemini hat keine Upload-URL zurückgegeben".to_string(); emit("", true, Some(&m)); m })?
        .to_string();

    let finalize_resp = agent.post(&upload_url)
        .set("Content-Length", &size.to_string())
        .set("X-Goog-Upload-Offset", "0")
        .set("X-Goog-Upload-Command", "upload, finalize")
        .send_bytes(&bytes)
        .map_err(|e| { let msg = format!("Upload: {}", e); emit("", true, Some(&msg)); msg })?;

    let finalize_body = finalize_resp.into_string().map_err(|e| format!("Upload-Antwort: {}", e))?;
    let finalize_json: serde_json::Value = serde_json::from_str(&finalize_body)
        .map_err(|e| format!("Upload-JSON: {} — {}", e, finalize_body))?;
    let file_uri = finalize_json.pointer("/file/uri").and_then(|v| v.as_str())
        .ok_or_else(|| { let m = "Gemini: file.uri fehlt".to_string(); emit("", true, Some(&m)); m })?
        .to_string();
    let file_name_path = finalize_json.pointer("/file/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut state = finalize_json.pointer("/file/state").and_then(|v| v.as_str()).unwrap_or("PROCESSING").to_string();

    // Poll until ACTIVE (max ~5min)
    emit("📤 Video hochgeladen, warte auf Verarbeitung…\n\n", false, None);
    let mut tries = 0;
    while state != "ACTIVE" && tries < 100 {
        if state == "FAILED" { let m = "Gemini: Datei-Verarbeitung fehlgeschlagen".to_string(); emit("", true, Some(&m)); return Err(m); }
        thread::sleep(Duration::from_secs(3));
        if !file_name_path.is_empty() {
            let poll_url = format!("https://generativelanguage.googleapis.com/v1beta/{}?key={}", file_name_path, api_key);
            if let Ok(r) = agent.get(&poll_url).call() {
                if let Ok(body) = r.into_string() {
                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(s) = j.get("state").and_then(|v| v.as_str()) {
                            state = s.to_string();
                        }
                    }
                }
            }
        } else {
            state = "ACTIVE".to_string();
        }
        tries += 1;
        // Fortschritts-Update alle 15s
        if tries % 5 == 0 && state != "ACTIVE" {
            let secs = tries * 3;
            emit(&format!("⏳ Verarbeitung läuft… ({}s)\n\n", secs), false, None);
        }
    }
    if state != "ACTIVE" {
        let m = format!("Gemini: Datei nicht bereit nach {}s — bitte bei sehr langen Videos nochmal versuchen", tries * 3);
        emit("", true, Some(&m));
        return Err(m);
    }

    // Step 4: streamGenerateContent
    let gen_url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse&key={}",
        api_key
    );
    let gen_body = serde_json::json!({
        "contents": [{
            "parts": [
                { "file_data": { "mime_type": mime, "file_uri": file_uri } },
                { "text": prompt }
            ]
        }]
    });

    let resp = agent.post(&gen_url)
        .set("Content-Type", "application/json")
        .send_string(&gen_body.to_string())
        .map_err(|e| match e {
            ureq::Error::Status(code, r) => { let m = format!("Gemini HTTP {}: {}", code, r.into_string().unwrap_or_default()); emit("", true, Some(&m)); m }
            ureq::Error::Transport(t) => { let m = format!("Gemini Transport: {}", t); emit("", true, Some(&m)); m }
        })?;

    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(resp.into_reader());
    let mut got_text = false;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Stream-Lesen: {}", e))?;
        if let Some(data) = line.strip_prefix("data: ") {
            if data.trim() == "[DONE]" { break; }
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(data) {
                // Check for API-level error in stream chunk
                if let Some(err) = j.get("error") {
                    let msg = format!("Gemini Stream-Fehler: {}", err);
                    emit("", true, Some(&msg));
                    return Err(msg);
                }
                // Iterate ALL parts — gemini-2.5-flash can return multiple parts per chunk
                // Skip thinking tokens (thought: true)
                if let Some(parts) = j.pointer("/candidates/0/content/parts").and_then(|v| v.as_array()) {
                    for part in parts {
                        let is_thought = part.get("thought").and_then(|v| v.as_bool()).unwrap_or(false);
                        if is_thought { continue; }
                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                            if !t.is_empty() {
                                emit(t, false, None);
                                got_text = true;
                            }
                        }
                    }
                }
            }
        }
    }
    if is_tmp { let _ = std::fs::remove_file(&upload_path); }

    if !got_text {
        let msg = "Gemini: Keine Antwort erhalten — API-Key prüfen oder Clip neu versuchen".to_string();
        emit("", true, Some(&msg));
        return Err(msg);
    }
    emit("", true, None);
    Ok(())
}

#[tauri::command]
fn generate_hashtags_groq_stream(app: tauri::AppHandle, clip_path: String, api_key: String, prompt: String, existing_desc: String) -> Result<(), String> {
    if api_key.trim().is_empty() { return Err("Groq API-Key fehlt".into()); }
    let name = Path::new(&clip_path).file_name().and_then(|n| n.to_str()).unwrap_or("clip").to_string();

    let emit = |text: &str, done: bool, error: Option<&str>| {
        let _ = app.emit("ai-stream", serde_json::json!({
            "text": text,
            "done": done,
            "error": error
        }));
    };

    let user_msg = if existing_desc.trim().is_empty() {
        format!("Clipname: {}\n\n(Keine bestehende Description vorhanden. Bewerte anhand des Dateinamens und des Kanal-Kontexts.)", name)
    } else {
        format!("Clipname: {}\n\nBestehende Description:\n{}", name, existing_desc.trim())
    };

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(120))
        .build();

    let body = serde_json::json!({
        "model": "llama-3.3-70b-versatile",
        "messages": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": user_msg }
        ],
        "temperature": 0.7,
        "stream": true
    });

    let resp = agent.post("https://api.groq.com/openai/v1/chat/completions")
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| match e {
            ureq::Error::Status(code, r) => { let m = format!("Groq HTTP {}: {}", code, r.into_string().unwrap_or_default()); emit("", true, Some(&m)); m }
            ureq::Error::Transport(t) => { let m = format!("Groq Transport: {}", t); emit("", true, Some(&m)); m }
        })?;

    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Stream-Lesen: {}", e))?;
        if let Some(data) = line.strip_prefix("data: ") {
            if data.trim() == "[DONE]" { break; }
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(t) = j.pointer("/choices/0/delta/content").and_then(|v| v.as_str()) {
                    if !t.is_empty() { emit(t, false, None); }
                }
            }
        }
    }
    emit("", true, None);
    Ok(())
}

#[tauri::command]
fn generate_hashtags_groq(clip_path: String, api_key: String, prompt: String, existing_desc: String) -> Result<String, String> {
    if api_key.trim().is_empty() { return Err("Groq API-Key fehlt".into()); }
    let name = Path::new(&clip_path).file_name().and_then(|n| n.to_str()).unwrap_or("clip").to_string();

    let user_msg = if existing_desc.trim().is_empty() {
        format!("Clipname: {}\n\n(Keine bestehende Description vorhanden. Bewerte anhand des Dateinamens und des Kanal-Kontexts.)", name)
    } else {
        format!("Clipname: {}\n\nBestehende Description:\n{}", name, existing_desc.trim())
    };

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(120))
        .build();

    let body = serde_json::json!({
        "model": "llama-3.3-70b-versatile",
        "messages": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": user_msg }
        ],
        "temperature": 0.7
    });

    let resp = agent.post("https://api.groq.com/openai/v1/chat/completions")
        .set("Authorization", &format!("Bearer {}", api_key))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("Groq HTTP {}: {}", code, body)
            }
            ureq::Error::Transport(t) => format!("Groq Transport-Fehler: {}", t),
        })?;

    let body = resp.into_string().map_err(|e| format!("Groq Antwort lesen: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("Groq JSON: {} — {}", e, body))?;
    json.pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Groq: keine Antwort ({})", json))
}

#[tauri::command]
fn get_local_ip() -> String {
    // Try to find the Windows hotspot IP (192.168.137.x) first
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in &ifaces {
            if ip.is_ipv4() {
                let s = ip.to_string();
                if s.starts_with("192.168.137.") {
                    return s;
                }
            }
        }
        // Fallback: first non-loopback IPv4
        for (_name, ip) in &ifaces {
            if ip.is_ipv4() && !ip.is_loopback() {
                return ip.to_string();
            }
        }
    }
    if let Ok(ip) = local_ip_address::local_ip() {
        return ip.to_string();
    }
    "127.0.0.1".to_string()
}

// ─── axum server ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ClipsServerState {
    root: String,
}

#[derive(serde::Deserialize)]
struct FolderQuery { folder: Option<String> }
#[derive(serde::Deserialize)]
struct RelQuery { rel: Option<String> }
#[derive(serde::Deserialize)]
struct RelPlatformQuery {
    rel: Option<String>,
    platform: Option<String>,
}

async fn serve_pwa() -> axum::response::Html<&'static str> {
    axum::response::Html(CLIPS_PWA_HTML)
}

async fn api_folders(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let root = Path::new(&state.root);
    let folders: Vec<String> = match fs::read_dir(root) {
        Ok(rd) => {
            let mut v: Vec<_> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            v.sort();
            v
        }
        Err(_) => vec![],
    };
    axum::Json(folders).into_response()
}

async fn api_clips(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<FolderQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let folder_name = match q.folder {
        Some(f) => f,
        None => return axum::Json(serde_json::json!([])).into_response(),
    };
    let folder_path = Path::new(&state.root).join(&folder_name);
    let clips: Vec<serde_json::Value> = match fs::read_dir(&folder_path) {
        Ok(rd) => {
            let mut v: Vec<_> = rd
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && e.path().extension().and_then(|x| x.to_str()).map(is_video_ext).unwrap_or(false)
                })
                .collect();
            v.sort_by_key(|e| e.file_name());
            v.into_iter().map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let rel = format!("{}/{}", folder_name, name);
                let base = e.path().to_string_lossy().to_string();
                let txt = format!("{}.txt", base);
                let sch_path = format!("{}.schedule", base);
                let schedule = if Path::new(&sch_path).exists() {
                    fs::read_to_string(&sch_path).unwrap_or_default()
                } else {
                    String::new()
                };
                let legacy_done = Path::new(&format!("{}.done", base)).exists();
                let done_yt = legacy_done || Path::new(&format!("{}.done.yt", base)).exists();
                let done_tt = legacy_done || Path::new(&format!("{}.done.tt", base)).exists();
                let done_ig = legacy_done || Path::new(&format!("{}.done.ig", base)).exists();
                let size = fs::metadata(e.path()).map(|m| m.len()).unwrap_or(0);
                serde_json::json!({
                    "name": name,
                    "rel": rel,
                    "has_desc": Path::new(&txt).exists(),
                    "schedule": schedule,
                    "done": done_yt && done_tt && done_ig,
                    "done_yt": done_yt,
                    "done_tt": done_tt,
                    "done_ig": done_ig,
                    "size": size
                })
            }).collect()
        }
        Err(_) => vec![],
    };
    axum::Json(clips).into_response()
}

async fn api_description(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<RelQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let rel = match q.rel {
        Some(r) => r,
        None => return "".into_response(),
    };
    let txt = format!("{}/{}.txt", state.root, rel);
    let content = fs::read_to_string(&txt).unwrap_or_default();
    content.into_response()
}

async fn api_schedule(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<RelQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let rel = match q.rel {
        Some(r) => r,
        None => return "".into_response(),
    };
    let sch = format!("{}/{}.schedule", state.root, rel);
    let content = fs::read_to_string(&sch).unwrap_or_default();
    content.into_response()
}

async fn api_toggle_done(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<RelQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let rel = match q.rel {
        Some(r) => r,
        None => return axum::Json(serde_json::json!({"done": false})).into_response(),
    };
    let done_path = format!("{}/{}.done", state.root, rel);
    let is_done = if Path::new(&done_path).exists() {
        let _ = fs::remove_file(&done_path);
        false
    } else {
        let _ = fs::write(&done_path, "");
        true
    };
    axum::Json(serde_json::json!({"done": is_done})).into_response()
}

async fn api_toggle_done_platform(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<RelPlatformQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let rel = match q.rel { Some(r) => r, None => return axum::Json(serde_json::json!({"error":"no rel"})).into_response() };
    let platform = match q.platform.as_deref() {
        Some("yt") | Some("youtube") => "yt",
        Some("tt") | Some("tiktok") => "tt",
        Some("ig") | Some("instagram") | Some("insta") => "ig",
        _ => return axum::Json(serde_json::json!({"error":"bad platform"})).into_response(),
    };
    let base = format!("{}/{}", state.root, rel);
    let flag = format!("{}.done.{}", base, platform);
    let is_done = if Path::new(&flag).exists() {
        let _ = fs::remove_file(&flag);
        false
    } else {
        let _ = fs::write(&flag, "");
        true
    };
    // Legacy done File löschen, damit die drei Flags zur Wahrheit werden
    let legacy = format!("{}.done", base);
    if Path::new(&legacy).exists() {
        let _ = fs::remove_file(&legacy);
    }
    axum::Json(serde_json::json!({"done": is_done, "platform": platform})).into_response()
}

async fn api_caption(
    axum::extract::State(state): axum::extract::State<ClipsServerState>,
    axum::extract::Query(q): axum::extract::Query<RelPlatformQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let rel = match q.rel { Some(r) => r, None => return "".into_response() };
    let platform = q.platform.as_deref().unwrap_or("");
    let suffix = match platform {
        "yt" | "youtube" => "yt",
        "tt" | "tiktok" => "tt",
        "ig" | "instagram" | "insta" => "ig",
        _ => "",
    };
    let base = format!("{}/{}", state.root, rel);
    if !suffix.is_empty() {
        let pf = format!("{}.{}.txt", base, suffix);
        if Path::new(&pf).exists() {
            return fs::read_to_string(&pf).unwrap_or_default().into_response();
        }
    }
    // Fallback
    let generic = format!("{}.txt", base);
    fs::read_to_string(&generic).unwrap_or_default().into_response()
}

#[tauri::command]
async fn start_clips_server(root: String, port: u16) -> Result<String, String> {
    use axum::Router;
    use axum_server::tls_rustls::RustlsConfig;

    // Stop any running server
    {
        let mut handle = CLIPS_SERVER_HANDLE.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }
    }

    let state = ClipsServerState { root: root.clone() };

    let app = Router::new()
        .route("/", axum::routing::get(serve_pwa))
        .route("/api/folders", axum::routing::get(api_folders))
        .route("/api/clips", axum::routing::get(api_clips))
        .route("/api/description", axum::routing::get(api_description))
        .route("/api/caption", axum::routing::get(api_caption))
        .route("/api/schedule", axum::routing::get(api_schedule))
        .route("/api/toggle-done", axum::routing::post(api_toggle_done))
        .route("/api/toggle-done-platform", axum::routing::post(api_toggle_done_platform))
        .route("/video/*path", axum::routing::get(serve_video))
        .with_state(state);

    let ip = get_local_ip();
    let mut params = rcgen::CertificateParams::new(vec![
        ip.clone(),
        "localhost".to_string(),
    ]).map_err(|e| e.to_string())?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "Clips Server");
    let key_pair = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key_pair).map_err(|e| e.to_string())?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    let tls_config = RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await.map_err(|e| e.to_string())?;

    let bind_addr = format!("0.0.0.0:{}", port);
    let addr: std::net::SocketAddr = bind_addr.parse().map_err(|e: std::net::AddrParseError| e.to_string())?;

    let task = tokio::spawn(async move {
        let _ = axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service()).await;
    });

    {
        let mut handle = CLIPS_SERVER_HANDLE.lock().await;
        *handle = Some(task);
    }

    Ok(format!("https://{}:{}", ip, port))
}

#[tauri::command]
async fn stop_clips_server() -> Result<(), String> {
    let mut handle = CLIPS_SERVER_HANDLE.lock().await;
    if let Some(h) = handle.take() {
        h.abort();
    }
    Ok(())
}

// ─── PWA HTML ────────────────────────────────────────────────────────────────

const CLIPS_PWA_HTML: &str = r#"<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<title>Clips</title>
<style>
*{box-sizing:border-box;margin:0;padding:0;-webkit-tap-highlight-color:transparent}
html,body{height:100%;background:#090910;color:#e5e7eb;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;overscroll-behavior:none}
#app{display:flex;flex-direction:column;height:100vh;max-width:600px;margin:0 auto}
header{display:flex;align-items:center;gap:12px;padding:16px;background:rgba(255,255,255,.04);border-bottom:1px solid rgba(255,255,255,.07);flex-shrink:0}
header h1{font-size:18px;font-weight:700;flex:1}
.back-btn{display:none;background:none;border:none;color:#6366f1;font-size:15px;cursor:pointer;padding:4px 8px;border-radius:8px}
.back-btn.visible{display:inline-flex;align-items:center;gap:4px}
main{flex:1;overflow-y:auto;padding:12px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:10px}
.card{background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.09);border-radius:14px;padding:12px;cursor:pointer;transition:background .15s;display:flex;flex-direction:column;gap:6px}
.card:active{background:rgba(255,255,255,.12)}
.card .icon{font-size:28px;text-align:center}
.card .label{font-size:13px;font-weight:600;text-align:center;word-break:break-word;line-height:1.3}
.card .meta{font-size:11px;color:#6b7280;text-align:center}
.dl-btn{display:inline-flex;align-items:center;gap:6px;padding:8px 14px;background:#4f46e5;color:#fff;border:none;border-radius:10px;font-size:13px;font-weight:600;cursor:pointer;text-decoration:none;transition:background .15s}
.dl-btn:active{background:#3730a3}
.dl-btn.secondary{background:rgba(255,255,255,.1)}
.dl-all{width:100%;justify-content:center;padding:12px;margin-bottom:12px;border-radius:12px;font-size:15px}
.player-wrap{background:#000;border-radius:14px;overflow:hidden;width:100%;aspect-ratio:9/16;max-height:60vh;position:relative}
.player-wrap video{width:100%;height:100%;object-fit:contain}
.desc-box{margin-top:12px;background:rgba(255,255,255,.05);border:1px solid rgba(255,255,255,.09);border-radius:12px;padding:12px}
.schedule-box{background:rgba(99,102,241,.1);border:1px solid rgba(99,102,241,.25);border-radius:12px;padding:10px 14px;margin-top:10px;display:flex;align-items:center;gap:8px}
.schedule-box .sch-icon{font-size:20px;flex-shrink:0}
.schedule-box .sch-info{display:flex;flex-direction:column;gap:2px}
.schedule-box .sch-day{font-size:13px;font-weight:700;color:#a5b4fc}
.schedule-box .sch-date{font-size:12px;color:#9ca3af}
.card-badge{position:absolute;bottom:6px;right:6px;font-size:12px}
.card{position:relative}
.card.done{background:rgba(34,197,94,.08);border-color:rgba(34,197,94,.25)}
.card.done .label{color:rgba(74,222,128,.7)}
.done-check{position:absolute;top:6px;left:6px;width:22px;height:22px;border-radius:6px;border:1.5px solid rgba(255,255,255,.2);display:flex;align-items:center;justify-content:center;font-size:12px;cursor:pointer;transition:all .15s;background:rgba(255,255,255,.05);color:transparent}
.done-check.checked{background:#16a34a;border-color:#22c55e;color:#fff}
.done-btn{display:inline-flex;align-items:center;gap:6px;padding:8px 14px;border:1.5px solid rgba(34,197,94,.3);border-radius:10px;font-size:13px;font-weight:600;cursor:pointer;transition:all .15s;background:rgba(34,197,94,.08);color:#4ade80}
.done-btn:active{background:rgba(34,197,94,.2)}
.done-btn.undone{background:rgba(255,255,255,.06);border-color:rgba(255,255,255,.12);color:#9ca3af}
.desc-box h3{font-size:12px;font-weight:600;color:#9ca3af;text-transform:uppercase;letter-spacing:.05em;margin-bottom:8px}
.desc-text{font-size:14px;line-height:1.6;white-space:pre-wrap;color:#e5e7eb}
.desc-empty{color:#6b7280;font-style:italic;font-size:14px}
.player-actions{display:flex;gap:10px;margin-top:12px;flex-wrap:wrap}
.prompt-btn{display:inline-flex;align-items:center;gap:6px;padding:8px 14px;background:#7c3aed;color:#fff;border:none;border-radius:10px;font-size:13px;font-weight:600;cursor:pointer;transition:background .15s}
.prompt-btn:active{background:#5b21b6}
.prompt-toast{position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:#1e1e2e;border:1px solid rgba(255,255,255,.15);color:#fff;padding:10px 20px;border-radius:14px;font-size:14px;z-index:999;opacity:0;transition:opacity .2s}
.copy-modal-overlay{position:fixed;inset:0;background:rgba(0,0,0,.7);z-index:1000;display:flex;align-items:flex-end;justify-content:center;padding:16px}
.copy-modal{background:#1a1a2e;border:1px solid rgba(255,255,255,.12);border-radius:18px 18px 14px 14px;padding:16px;width:100%;max-width:560px}
.copy-modal h4{font-size:13px;color:#9ca3af;margin-bottom:8px;text-align:center}
.copy-modal textarea{width:100%;height:130px;background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.15);border-radius:10px;color:#e5e7eb;font-size:13px;padding:10px;resize:none;font-family:inherit;line-height:1.5}
.copy-modal-close{display:block;width:100%;margin-top:10px;padding:10px;background:#4f46e5;color:#fff;border:none;border-radius:10px;font-size:14px;font-weight:600;cursor:pointer}
.prompt-toast.show{opacity:1}
.empty-state .ico{font-size:48px}
.loading{display:flex;align-items:center;justify-content:center;padding:40px;color:#6b7280}
.plat-badges{position:absolute;bottom:6px;right:6px;display:flex;gap:4px}
.plat-badge{width:20px;height:20px;border-radius:4px;display:flex;align-items:center;justify-content:center;font-size:11px;font-weight:700;background:rgba(255,255,255,.08);color:#666;cursor:pointer;transition:all 0.15s}
.plat-badge:active{transform:scale(0.9)}
.plat-badge.on.yt{background:#dc2626;color:#fff}
.plat-badge.on.tt{background:#000;color:#fff;border:1px solid #333}
.plat-badge.on.ig{background:#db2777;color:#fff}
.card-share{display:flex;gap:6px;margin-top:8px}
.card-share-btn{flex:1;padding:7px 0;border:none;border-radius:8px;color:#fff;font-size:12px;font-weight:600;cursor:pointer;transition:transform 0.15s}
.card-share-btn:active{transform:scale(0.95)}
.card-share-btn.tt{background:#111;border:1px solid #333}
.card-share-btn.ig{background:linear-gradient(45deg,#f09433,#dc2743,#bc1888)}
.share-row{display:flex;gap:8px;margin:12px 0}
.share-btn{flex:1;padding:14px;border:none;border-radius:12px;color:#fff;font-size:15px;font-weight:600;cursor:pointer;transition:transform 0.15s;text-align:center;}
.share-btn.tt{background:#000;border:1px solid #333}
.share-btn.ig{background:linear-gradient(45deg,#f09433,#e6683c,#dc2743,#cc2366,#bc1888)}
.share-btn:active{transform:scale(0.97)}
.batch-toggle{display:block;width:100%;margin-bottom:12px;padding:12px;background:rgba(255,255,255,.06);color:#fff;border:1px solid rgba(255,255,255,.1);border-radius:12px;font-size:14px;font-weight:600;cursor:pointer;}
.batch-toggle.on{background:rgba(74,222,128,.15);border-color:#22c55e;color:#4ade80;}
</style>
</head>
<body>
<div id="app">
  <header>
    <button class="back-btn" id="backBtn" onclick="goBack()">&#8592; Zurück</button>
    <h1 id="pageTitle">📁 Clips</h1>
  </header>
  <main id="main"><div class="loading">Lade...</div></main>
</div>
<script>
var state = {view:'folders', folder:null, clip:null, desc:'', batchMode: false, batchPlatform: null};

function goBack() {
  if (state.view === 'player') { state.view = 'clips'; renderClips(state.folder); }
  else if (state.view === 'clips') { state.view = 'folders'; renderFolders(); }
}

function setHeader(title, showBack) {
  document.getElementById('pageTitle').textContent = title;
  var b = document.getElementById('backBtn');
  b.className = showBack ? 'back-btn visible' : 'back-btn';
}

async function renderFolders() {
  state.view = 'folders';
  setHeader('📁 Clips', false);
  var main = document.getElementById('main');
  main.innerHTML = '<div class="loading">Lade Ordner...</div>';
  var folders = await fetch('/api/folders').then(r => r.json()).catch(() => []);
  if (!folders.length) {
    main.innerHTML = '<div class="empty-state"><div class="ico">📂</div><div>Keine Ordner gefunden</div></div>';
    return;
  }
  main.innerHTML = '<div class="grid" id="grid"></div>';
  var grid = document.getElementById('grid');
  folders.forEach(function(f) {
    var d = document.createElement('div');
    d.className = 'card';
    d.innerHTML = '<div class="icon">🎬</div><div class="label">'+escHtml(f)+'</div>';
    d.onclick = function() { renderClips(f); };
    grid.appendChild(d);
  });
}

async function renderClips(folder) {
  state.view = 'clips'; state.folder = folder;
  setHeader('🎬 ' + folder, true);
  var main = document.getElementById('main');
  main.innerHTML = '<div class="loading">Lade Clips...</div>';
  var clips = await fetch('/api/clips?folder='+encodeURIComponent(folder)).then(r => r.json()).catch(() => []);
  if (!clips.length) {
    main.innerHTML = '<div class="empty-state"><div class="ico">🎞️</div><div>Keine Clips in diesem Ordner</div></div>';
    return;
  }
  main.innerHTML = '';
  var allBtn = document.createElement('button');
  allBtn.className = 'dl-btn dl-all';
  allBtn.innerHTML = '⬇ Alle herunterladen (' + clips.length + ')';
  allBtn.onclick = function() { downloadAll(clips); };
  main.appendChild(allBtn);
  var promptBtn = document.createElement('button');
  promptBtn.className = 'prompt-btn dl-all';
  promptBtn.innerHTML = '📋 Copy Prompt';
  promptBtn.onclick = function() { copyPrompt(); };
  main.appendChild(promptBtn);
  var grid = document.createElement('div');
  grid.className = 'grid';
  grid.id = 'grid';
  
  if (!document.querySelector('.batch-toggle')) {
      var batchBtn = document.createElement('button');
      batchBtn.className = 'batch-toggle ' + (state.batchMode ? 'on' : '');
      batchBtn.innerHTML = state.batchMode
        ? '📱 Batch ' + state.batchPlatform.toUpperCase() + ' — beenden'
        : '📱 Batch-Modus';
      batchBtn.onclick = async function() {
        if (state.batchMode) {
          state.batchMode = false;
          state.batchPlatform = null;
          renderClips(state.folder);
        } else {
          var p = prompt('Welche Plattform? (tt / ig)');
          if (p === 'tt' || p === 'ig') {
            state.batchMode = true;
            state.batchPlatform = p;
            var next = clips.find(function(c) {
              return p === 'tt' ? !c.done_tt : !c.done_ig;
            });
            if (next) renderPlayer(next);
            else showPwaToast('Alle Clips bereits gepostet');
          }
        }
      };
      main.insertBefore(batchBtn, allBtn);
  }
  grid.className = 'grid';
  clips.forEach(function(c) {
    var size = c.size > 0 ? formatSize(c.size) : '';
    var schFmtCard = formatSchedulePwa(c.schedule || '');
    var schMeta = schFmtCard ? '<div class="meta" style="color:#a5b4fc;font-size:10px">'+escHtml(schFmtCard.day)+' '+escHtml(schFmtCard.full)+'</div>' : '';
    var allDone = c.done_yt && c.done_tt && c.done_ig;
    var d = document.createElement('div');
    d.className = allDone ? 'card done' : 'card';
    d.innerHTML =
      '<div class="icon">'+(allDone?'✅':'▶️')+'</div>'+
      '<div class="label">'+escHtml(c.name)+'</div>'+
      (size?'<div class="meta">'+size+'</div>':'')+
      schMeta +
      '<div class="plat-badges">'+
        '<span data-p="yt" class="plat-badge '+(c.done_yt?'on yt':'')+'">Y</span>'+
        '<span data-p="tt" class="plat-badge '+(c.done_tt?'on tt':'')+'">T</span>'+
        '<span data-p="ig" class="plat-badge '+(c.done_ig?'on ig':'')+'">I</span>'+
      '</div>';
    d.querySelector('.icon').onclick = d.querySelector('.label').onclick = function() { renderPlayer(c); };
    d.querySelectorAll('.plat-badge').forEach(function(b) {
      b.onclick = function(ev) {
        ev.stopPropagation();
        var p = b.getAttribute('data-p');
        togglePlatformDonePwa(c, p, function(done) {
          b.className = 'plat-badge ' + (done ? 'on ' + p : '');
          var now = c.done_yt && c.done_tt && c.done_ig;
          d.className = now ? 'card done' : 'card';
          d.querySelector('.icon').textContent = now ? '✅' : '▶️';
        });
      };
    });
    grid.appendChild(d);
  });
  main.appendChild(grid);
}

async function renderPlayer(clip) {
  state.view = 'player'; state.clip = clip;
  setHeader('▶ ' + clip.name, true);
  var main = document.getElementById('main');
  var folder = state.folder;
  var videoUrl = '/video/' + encodeURIComponent(folder) + '/' + encodeURIComponent(clip.name);
  var desc = '';
  if (clip.has_desc) {
    desc = await fetch('/api/description?rel='+encodeURIComponent(clip.rel)).then(r => r.text()).catch(() => '');
  }
  state.desc = desc;
  var scheduleHtml = '';
  var schRaw = clip.schedule || '';
  if (!schRaw && clip.rel) {
    schRaw = await fetch('/api/schedule?rel='+encodeURIComponent(clip.rel)).then(r => r.text()).catch(() => '');
  }
  var schFmt = formatSchedulePwa(schRaw);
  if (schFmt) {
    scheduleHtml = '<div class="schedule-box"><div class="sch-icon">📅</div><div class="sch-info"><div class="sch-day">'+escHtml(schFmt.day)+'</div><div class="sch-date">'+escHtml(schFmt.full)+'</div></div></div>';
  }
  main.innerHTML =
    '<div class="player-wrap"><video controls playsinline autoplay src="'+escAttr(videoUrl)+'"></video></div>' +
    scheduleHtml +
    '<div class="player-actions">' +
      '<button class="done-btn'+(clip.done_yt?'':' undone')+'" onclick="togglePlatformDonePwa(state.clip, \'yt\', function(d){this.className=d?\'done-btn\':\'done-btn undone\';this.innerHTML=d?\'✅ YT\':\'☐ YT\';}.bind(this))">'+(clip.done_yt?'✅ YT':'☐ YT')+'</button>' +
      '<button class="done-btn'+(clip.done_tt?'':' undone')+'" onclick="togglePlatformDonePwa(state.clip, \'tt\', function(d){this.className=d?\'done-btn\':\'done-btn undone\';this.innerHTML=d?\'✅ TT\':\'☐ TT\';}.bind(this))">'+(clip.done_tt?'✅ TT':'☐ TT')+'</button>' +
      '<button class="done-btn'+(clip.done_ig?'':' undone')+'" onclick="togglePlatformDonePwa(state.clip, \'ig\', function(d){this.className=d?\'done-btn\':\'done-btn undone\';this.innerHTML=d?\'✅ IG\':\'☐ IG\';}.bind(this))">'+(clip.done_ig?'✅ IG':'☐ IG')+'</button>' +
      '<a class="dl-btn secondary" href="'+escAttr(videoUrl)+'" download="'+escAttr(clip.name)+'">⬇ Herunterladen</a>' +
      (desc ? '<button class="dl-btn secondary" onclick="copyDescPwa()">📋 Copy Text</button>' : '') +
    '</div>' +
    '<div class="desc-box">' +
      '<h3>Beschreibung (Fallback)</h3>' +
      (desc ? '<div class="desc-text">'+escHtml(desc)+'</div>' : '<div class="desc-empty">Keine Beschreibung</div>') +
    '</div>';
}

function downloadAll(clips) {
  var folder = state.folder;
  var i = 0;
  function next() {
    if (i >= clips.length) return;
    var c = clips[i++];
    var a = document.createElement('a');
    a.href = '/video/' + encodeURIComponent(folder) + '/' + encodeURIComponent(c.name);
    a.download = c.name;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(next, 800);
  }
  next();
}

function formatSize(bytes) {
  if (bytes < 1024*1024) return (bytes/1024).toFixed(0)+' KB';
  return (bytes/1024/1024).toFixed(1)+' MB';
}
function copyDescPwa() {
  if (!state.desc) { showPwaToast('Kein Text vorhanden'); return; }
  copyTextMobile(state.desc);
}
function copyTextMobile(text) {
  // 1) native Share sheet (works on HTTP on iOS/Android)
  if (navigator.share) {
    navigator.share({ text: text }).then(function() {
      showPwaToast('Geteilt!');
    }).catch(function(e) {
      if (e && e.name !== 'AbortError') { showCopyModal(text); }
    });
    return;
  }
  // 2) Clipboard API (only on HTTPS / localhost)
  if (navigator.clipboard && window.isSecureContext) {
    navigator.clipboard.writeText(text).then(function() {
      showPwaToast('Text kopiert!');
    }).catch(function() { showCopyModal(text); });
    return;
  }
  // 3) Fallback: show selectable text modal
  showCopyModal(text);
}
function showCopyModal(text) {
  var overlay = document.createElement('div');
  overlay.className = 'copy-modal-overlay';
  overlay.innerHTML =
    '<div class="copy-modal">' +
      '<h4>Text markieren &amp; kopieren (langes Tippen)</h4>' +
      '<textarea readonly>' + escHtml(text) + '</textarea>' +
      '<button class="copy-modal-close" onclick="this.closest(\'.copy-modal-overlay\').remove()">✕ Schließen</button>' +
    '</div>';
  document.body.appendChild(overlay);
  // auto-select text so user only needs to copy
  var ta = overlay.querySelector('textarea');
  setTimeout(function() { ta.focus(); ta.select(); }, 50);
  overlay.addEventListener('click', function(e) {
    if (e.target === overlay) { overlay.remove(); }
  });
}
async function togglePlatformDonePwa(clip, platform, callback) {
  var res = await fetch('/api/toggle-done-platform?rel='+encodeURIComponent(clip.rel)+'&platform='+platform,
    {method:'POST'}).then(function(r){return r.json();}).catch(function(){return null;});
  if (res && !res.error) {
    if (platform === 'yt') clip.done_yt = res.done;
    if (platform === 'tt') clip.done_tt = res.done;
    if (platform === 'ig') clip.done_ig = res.done;
    clip.done = clip.done_yt && clip.done_tt && clip.done_ig;
    if (callback) callback(res.done);
  }
}
async function toggleDonePwa(clip, callback) {
  var res = await fetch('/api/toggle-done?rel='+encodeURIComponent(clip.rel), {method:'POST'}).then(function(r){return r.json();}).catch(function(){return null;});
  if (res) {
    clip.done = res.done;
    showPwaToast(res.done ? 'Als hochgeladen markiert' : 'Markierung entfernt');
    if (callback) callback(res.done);
  }
}
function formatSchedulePwa(dt) {
  if (!dt || !dt.trim()) return null;
  var d = new Date(dt.trim());
  if (isNaN(d.getTime())) return null;
  var days = ['Sonntag','Montag','Dienstag','Mittwoch','Donnerstag','Freitag','Samstag'];
  var day = days[d.getDay()];
  var date = d.toLocaleDateString('de-DE', {day:'2-digit', month:'2-digit', year:'numeric'});
  var time = d.toLocaleTimeString('de-DE', {hour:'2-digit', minute:'2-digit'});
  return { day: day, full: date + ' \u00b7 ' + time + ' Uhr' };
}
function escHtml(s) { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }
function escAttr(s) { return s.replace(/"/g,'&quot;'); }

var AI_PROMPT = 'Du bist ein Social-Media-Experte fuer TikTok, Instagram Reels und YouTube Shorts. Ich sende dir ein vertikales Kurzvideo.

Aufgabe:
1. Bewerte das Video kurz (Inhalt, Stimmung, Zielgruppe, virales Potenzial) in 2-3 Saetzen.
2. Schreibe eine kurze, authentische deutsche Caption und genau 3-4 virale, zum Video passende Hashtags.
3. Empfiehl die beste Posting-Zeit mit kurzer Begruendung.

Gib Caption und Hashtags klar getrennt aus, damit ich sie direkt kopieren kann.';

function copyPrompt() {
  copyTextMobile(AI_PROMPT);
}

function showPwaToast(msg) {
  var existing = document.querySelector('.prompt-toast');
  if (existing) existing.remove();
  var t = document.createElement('div');
  t.className = 'prompt-toast show';
  t.textContent = msg;
  document.body.appendChild(t);
  setTimeout(function() { t.classList.remove('show'); setTimeout(function() { t.remove(); }, 300); }, 2000);
}

renderFolders();
</script>
</body>
</html>"#;

// ─── Clip Compatibility Check + Transcoding ───────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct ClipCompatInfo {
    pub codec:           String,
    pub needs_transcode: bool,
    pub reason:          Option<String>,
    pub duration_secs:   f64,
}

/// Check video codec via ffprobe. Returns compatibility info for Gemini.
#[tauri::command]
fn check_clip_compat(app: tauri::AppHandle, clip_path: String) -> Result<ClipCompatInfo, String> {
    // Derive ffprobe path from the resolved ffmpeg path (same directory)
    let ffmpeg = resolve_ffmpeg(&app, None);
    let ffprobe = ffmpeg.with_file_name("ffprobe.exe");
    let ffprobe_cmd = if ffprobe.exists() { ffprobe.to_string_lossy().to_string() } else { "ffprobe".to_string() };

    let out = std::process::Command::new(&ffprobe_cmd)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-show_format",
            &clip_path,
        ])
        .output()
        .map_err(|e| format!("ffprobe nicht gefunden ('{ffprobe_cmd}'): {e}"))?;

    let json: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("ffprobe JSON-Fehler: {e}"))?;

    // Extract video stream codec
    let codec = json["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"].as_str() == Some("video")))
        .and_then(|s| s["codec_name"].as_str())
        .unwrap_or("unknown")
        .to_string();

    let duration_secs = json["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Gemini supports h264, hevc, vp9, av1 well — everything else may fail
    let gemini_ok = matches!(codec.as_str(), "h264" | "hevc" | "vp9" | "av1" | "mpeg4");
    let (needs_transcode, reason) = if codec == "unknown" {
        (true, Some("Codec konnte nicht erkannt werden".to_string()))
    } else if !gemini_ok {
        (true, Some(format!("Codec '{codec}' wird von Gemini nicht unterstützt → H.264 empfohlen")))
    } else {
        (false, None)
    };

    Ok(ClipCompatInfo { codec, needs_transcode, reason, duration_secs })
}

/// Transcode clip to H.264 MP4 using ffmpeg, emitting progress events.
/// Replaces the original file on success.
#[tauri::command]
fn transcode_clip(app: tauri::AppHandle, clip_path: String) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let ffmpeg_path = resolve_ffmpeg(&app, None);
    let ffprobe_path = ffmpeg_path.with_file_name("ffprobe.exe");
    let ffprobe_cmd = if ffprobe_path.exists() { Some(ffprobe_path.to_string_lossy().to_string()) } else { None };
    let ffmpeg_cmd  = if ffmpeg_path.exists()  { ffmpeg_path.to_string_lossy().to_string()  } else { "ffmpeg".to_string()  };

    // Try to get duration for progress % — optional, falls back to indeterminate if ffprobe missing
    let total_ms: f64 = ffprobe_cmd.as_deref()
        .and_then(|cmd| Command::new(cmd)
            .args(["-v", "quiet", "-show_entries", "format=duration",
                   "-of", "default=noprint_wrappers=1:nokey=1", &clip_path])
            .output().ok())
        .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok())
        .unwrap_or(0.0) * 1_000_000.0;

    let tmp_path = format!("{}.tc_tmp.mp4", clip_path);

    let emit_progress = |pct: u8, msg: &str| {
        let _ = app.emit("transcode-progress", serde_json::json!({
            "pct": pct, "msg": msg, "done": false, "error": null
        }));
    };
    let emit_done = |ok: bool, msg: &str| {
        let error_val: serde_json::Value = if ok { serde_json::Value::Null } else { msg.into() };
        let _ = app.emit("transcode-progress", serde_json::json!({
            "pct": if ok { 100 } else { 0 },
            "msg": msg, "done": true, "error": error_val
        }));
    };

    emit_progress(0, "Starte Umwandlung…");

    let mut child = Command::new(&ffmpeg_cmd)
        .args([
            "-i", &clip_path,
            "-c:v", "libx264",
            "-profile:v", "high",
            "-crf", "23",
            "-maxrate", "8M",
            "-bufsize", "16M",
            "-c:a", "aac",
            "-b:a", "128k",
            "-movflags", "+faststart",
            "-progress", "pipe:1",
            "-nostats",
            "-y",
            &tmp_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg nicht gefunden: {e}"))?;

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().flatten() {
            if let Some(val) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = val.trim().parse::<f64>() {
                    let pct = if total_ms > 0.0 {
                        ((us / total_ms) * 100.0).min(99.0) as u8
                    } else { 0 };
                    emit_progress(pct, &format!("Umwandlung… {}%", pct));
                }
            }
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        emit_done(false, "ffmpeg Fehler — Umwandlung fehlgeschlagen");
        return Err("ffmpeg Fehler".into());
    }

    std::fs::rename(&tmp_path, &clip_path)
        .map_err(|e| format!("Datei ersetzen fehlgeschlagen: {e}"))?;

    emit_done(true, "✓ Umwandlung abgeschlossen");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_clip_folders,
            create_clip_folder,
            list_clips,
            get_clip_description,
            save_clip_description,
            get_clip_schedule,
            save_clip_schedule,
            toggle_clip_done,
            copy_file_to_clipboard,
            copy_clip_to_folder,
            generate_hashtags_gemini,
            generate_hashtags_gemini_stream,
            generate_hashtags_groq,
            generate_hashtags_groq_stream,
            get_local_ip,
            start_clips_server,
            stop_clips_server,
            toggle_clip_done_platform,
            get_clip_platform_status,
            get_clip_caption,
            save_clip_caption,
            schedule_clip,
            check_clip_compat,
            transcode_clip,
            find_clips_on_same_date
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
