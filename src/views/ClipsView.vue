<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import QRCode from 'qrcode';

// ─── Types ────────────────────────────────────────────────────────────────────

interface ClipInfo {
  name: string;
  rel_path: string;
  has_description: boolean;
  has_schedule: boolean;
  is_done: boolean;
  done_yt: boolean;
  done_tt: boolean;
  done_ig: boolean;
  done_pt: boolean;
  done_sc: boolean;
  size_bytes: number;
}

// ─── Settings ─────────────────────────────────────────────────────────────────

const clipsRoot = ref('');
const serverPort = ref(7890);
const rootInputVisible = ref(false);

const geminiKey = ref('');
const groqKey = ref('');
const aiDefaultProvider = ref<'gemini' | 'groq'>('gemini');

async function loadSettings() {
  try {
    const { getStore } = await import('../store');
    const store = await getStore();
    clipsRoot.value = (await store.get<string>('clips.root')) || '';
    serverPort.value = (await store.get<number>('clips.port')) || 7890;
    geminiKey.value = (await store.get<string>('ai.gemini_key')) || '';
    groqKey.value = (await store.get<string>('ai.groq_key')) || '';
    const prov = (await store.get<string>('ai.default_provider')) || 'gemini';
    aiDefaultProvider.value = prov === 'groq' ? 'groq' : 'gemini';
  } catch { /* ignore */ }
}

async function saveSettings() {
  try {
    const { getStore } = await import('../store');
    const store = await getStore();
    await store.set('clips.root', clipsRoot.value);
    await store.set('clips.port', serverPort.value);
    await store.save();
  } catch { /* ignore */ }
}

async function browseRoot() {
  const selected = await open({ directory: true, title: 'Clips-Ordner wählen' });
  if (selected) {
    clipsRoot.value = selected as string;
    await saveSettings();
    await loadFolders();
  }
}

// ─── Folders ──────────────────────────────────────────────────────────────────

const folders = ref<string[]>([]);
const selectedFolder = ref('');
const newFolderName = ref('');
const showNewFolder = ref(false);
const folderError = ref('');

async function loadFolders() {
  if (!clipsRoot.value) return;
  try {
    folders.value = await invoke<string[]>('list_clip_folders', { root: clipsRoot.value });
    if (folders.value.length > 0 && !selectedFolder.value) {
      await selectFolder(folders.value[0]);
    }
  } catch (e) {
    folderError.value = String(e);
  }
}

function clearVideo() {
  if (videoEl.value) {
    videoEl.value.pause();
    videoEl.value.removeAttribute('src');
    videoEl.value.load();
  }
}

async function selectFolder(name: string) {
  clearVideo();
  selectedFolder.value = name;
  selectedClip.value = null;
  await Promise.all([loadClips(), loadMainVideoUrl()]);
}

// ─── Main Video URL (YouTube-Verlinkung pro Ordner) ───────────────────────────

const mainVideoUrl = ref('');

async function loadMainVideoUrl() {
  if (!selectedFolder.value) { mainVideoUrl.value = ''; return; }
  try {
    const { getStore } = await import('../store');
    const store = await getStore();
    mainVideoUrl.value = (await store.get<string>(`clips.main_video.${selectedFolder.value}`)) || '';
  } catch { mainVideoUrl.value = ''; }
}

async function saveMainVideoUrl() {
  if (!selectedFolder.value) return;
  try {
    const { getStore } = await import('../store');
    const store = await getStore();
    await store.set(`clips.main_video.${selectedFolder.value}`, mainVideoUrl.value);
    await store.save();
  } catch { /* ignore */ }
}

function extractYouTubeId(url: string): string {
  const m = url.match(/(?:v=|youtu\.be\/)([A-Za-z0-9_-]{11})/);
  return m ? m[1] : '';
}

// ─── Auto-Datum-Setter ────────────────────────────────────────────────────────

const showDateSetter = ref(false);
const dateSetterStart = ref('');
const isSettingDates = ref(false);

// Optimale Posting-Uhrzeiten pro Wochentag (0=Sonntag … 6=Samstag)
const OPTIMAL_TIMES: Record<number, string> = {
  0: '11:00', // Sonntag
  1: '17:00', // Montag
  2: '18:00', // Dienstag
  3: '17:00', // Mittwoch
  4: '18:00', // Donnerstag
  5: '16:00', // Freitag
  6: '11:00', // Samstag
};

function sortedClipsNumerically() {
  return [...clips.value].sort((a, b) => {
    const numA = parseInt(a.name.match(/(\d+)/)?.[1] ?? '0', 10);
    const numB = parseInt(b.name.match(/(\d+)/)?.[1] ?? '0', 10);
    return numA !== numB ? numA - numB : a.name.localeCompare(b.name);
  });
}

async function setDatesForAll() {
  if (!dateSetterStart.value) return;
  isSettingDates.value = true;
  const sorted = sortedClipsNumerically();
  const pad = (n: number) => n.toString().padStart(2, '0');
  let current = new Date(dateSetterStart.value + 'T12:00:00');
  let count = 0;

  for (const clip of sorted) {
    if (clip.has_schedule) {
      current.setDate(current.getDate() + 1);
      continue;
    }
    const dow  = current.getDay();
    const time = OPTIMAL_TIMES[dow];
    const [h, m] = time.split(':').map(Number);
    const dt = new Date(current);
    dt.setHours(h, m, 0, 0);
    const dtStr = `${dt.getFullYear()}-${pad(dt.getMonth()+1)}-${pad(dt.getDate())}T${pad(dt.getHours())}:${pad(dt.getMinutes())}`;
    try {
      await invoke('save_clip_schedule', { clipPath: clip.rel_path, datetime: dtStr });
      clip.has_schedule = true;
      count++;
    } catch { /* skip */ }
    current.setDate(current.getDate() + 1);
  }

  isSettingDates.value = false;
  showDateSetter.value  = false;
  showToast(`✅ ${count} Clips: Datum + optimale Uhrzeit gesetzt`);
  await loadClips();
}

async function createFolder() {
  const n = newFolderName.value.trim();
  if (!n) return;
  try {
    await invoke('create_clip_folder', { root: clipsRoot.value, name: n });
    newFolderName.value = '';
    showNewFolder.value = false;
    await loadFolders();
    await selectFolder(n);
  } catch (e) {
    folderError.value = String(e);
  }
}

// ─── Clips ────────────────────────────────────────────────────────────────────

const clips = ref<ClipInfo[]>([]);
const selectedClip = ref<ClipInfo | null>(null);
const clipError = ref('');
interface Toast { id: number; msg: string; isError: boolean; }
const toasts = ref<Toast[]>([]);
let toastId = 0;

function showToast(msg: string, persistent = false) {
  const isError = persistent || msg.startsWith('❌') || msg.startsWith('Fehler');
  const id = ++toastId;
  toasts.value.push({ id, msg, isError });
  if (!isError) {
    setTimeout(() => dismissToast(id), 2500);
  }
}

function dismissToast(id: number) {
  toasts.value = toasts.value.filter(t => t.id !== id);
}

async function loadClips() {
  if (!selectedFolder.value || !clipsRoot.value) return;
  const folderPath = clipsRoot.value + '/' + selectedFolder.value;
  try {
    clips.value = await invoke<ClipInfo[]>('list_clips', { folder: folderPath });
  } catch (e) {
    clipError.value = String(e);
  }
}

const videoEl = ref<HTMLVideoElement | null>(null);

async function selectClip(clip: ClipInfo) {
  clearVideo();
  selectedClip.value = clip;
  const [desc, sched] = await Promise.all([
    invoke<string>('get_clip_description', { clipPath: clip.rel_path }),
    invoke<string>('get_clip_schedule',    { clipPath: clip.rel_path }),
  ]);
  description.value = desc;
  scheduledAt.value = sched;
  await checkScheduleConflicts();
}


function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

async function togglePlatformDone(clip: ClipInfo, platform: 'yt' | 'tt' | 'ig' | 'pt' | 'sc', e?: Event) {
  if (e) e.stopPropagation();
  try {
    const newState = await invoke<boolean>('toggle_clip_done_platform', {
      clipPath: clip.rel_path,
      platform,
    });
    if (platform === 'yt') clip.done_yt = newState;
    if (platform === 'tt') clip.done_tt = newState;
    if (platform === 'ig') clip.done_ig = newState;
    if (platform === 'pt') clip.done_pt = newState;
    if (platform === 'sc') clip.done_sc = newState;
    clip.is_done = clip.done_yt && clip.done_tt && clip.done_ig;
  } catch (err) {
    showToast('Fehler: ' + String(err));
  }
}

// ─── Description ─────────────────────────────────────────────────────────────

const description = ref('');
let descSaveTimer: ReturnType<typeof setTimeout> | null = null;

function onDescriptionInput() {
  if (descSaveTimer) clearTimeout(descSaveTimer);
  descSaveTimer = setTimeout(saveDescription, 600);
}

async function saveDescription() {
  if (!selectedClip.value) return;
  try {
    await invoke('save_clip_description', {
      clipPath: selectedClip.value.rel_path,
      text: description.value,
    });
    // Update has_description on the clip
    const c = clips.value.find(c => c.rel_path === selectedClip.value!.rel_path);
    if (c) c.has_description = description.value.length > 0;
  } catch { /* ignore */ }
}

async function copyDescription() {
  if (!description.value) return;
  await navigator.clipboard.writeText(description.value);
  showToast('Text kopiert!');
}

// ─── Schedule ─────────────────────────────────────────────────────────────────

const scheduledAt = ref('');
let scheduleSaveTimer: ReturnType<typeof setTimeout> | null = null;

interface ScheduleConflict { folder: string; clip: string; scheduled_at: string; rel_path: string; }
const scheduleConflicts = ref<ScheduleConflict[]>([]);

async function checkScheduleConflicts() {
  if (!scheduledAt.value || !selectedClip.value || !clipsRoot.value) {
    scheduleConflicts.value = [];
    return;
  }
  const date = scheduledAt.value.slice(0, 10);
  try {
    scheduleConflicts.value = await invoke<ScheduleConflict[]>('find_clips_on_same_date', {
      root: clipsRoot.value,
      date,
      exclude: selectedClip.value.rel_path,
    });
  } catch { scheduleConflicts.value = []; }
}

function onScheduleInput() {
  if (scheduleSaveTimer) clearTimeout(scheduleSaveTimer);
  scheduleSaveTimer = setTimeout(async () => {
    await saveSchedule();
    await checkScheduleConflicts();
  }, 600);
}

async function saveSchedule() {
  if (!selectedClip.value) return;
  try {
    await invoke('save_clip_schedule', {
      clipPath: selectedClip.value.rel_path,
      datetime: scheduledAt.value,
    });
    const c = clips.value.find(c => c.rel_path === selectedClip.value!.rel_path);
    if (c) c.has_schedule = scheduledAt.value.length > 0;
  } catch { /* ignore */ }
}

function formatScheduleLabel(dt: string): string {
  if (!dt) return '';
  const d = new Date(dt);
  if (isNaN(d.getTime())) return '';
  const days = ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'];
  const day = days[d.getDay()];
  const date = d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' });
  const time = d.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
  return `${day}, ${date} · ${time} Uhr`;
}

// ─── Clipboard (file) ─────────────────────────────────────────────────────────

async function copyFileToClipboard() {
  if (!selectedClip.value) return;
  try {
    await invoke('copy_file_to_clipboard', { path: selectedClip.value.rel_path });
    showToast('Datei in Zwischenablage kopiert!');
  } catch (e) {
    showToast('Fehler: ' + String(e));
  }
}

// ─── Scheduling ──────────────────────────────────────────────────────────────

const isScheduling = ref(false);
type ScheduleStatus = 'idle' | 'uploading' | 'done' | 'failed';
const scheduleStatus = ref<Record<string, ScheduleStatus>>({
  youtube: 'idle', tiktok: 'idle', instagram: 'idle', pinterest: 'idle', snapchat: 'idle', tiktok_draft: 'idle',
});

function resetScheduleStatus() {
  scheduleStatus.value = { youtube: 'idle', tiktok: 'idle', instagram: 'idle', pinterest: 'idle', snapchat: 'idle', tiktok_draft: 'idle' };
}

async function scheduleForPlatform(platform: 'youtube' | 'tiktok' | 'instagram' | 'pinterest' | 'snapchat' | 'tiktok_draft') {
  if (!selectedClip.value) { showToast('Kein Clip ausgewählt'); return; }
  if (!scheduledAt.value) { showToast('Kein Upload-Termin gesetzt'); return; }

  // Validate: must be at least 2 minutes in the future
  const scheduledDate = new Date(scheduledAt.value);
  if (scheduledDate.getTime() < Date.now() + 60 * 1000) {
    showToast('Upload-Termin muss mindestens 1 Minute in der Zukunft liegen');
    return;
  }

  const { getStore } = await import('../store');
  const store = await getStore();
  const serverUrl = (await store.get<string>('scheduler.server_url')) || '';
  const uploadUrl = (await store.get<string>('scheduler.upload_url')) || '';
  const apiKey    = (await store.get<string>('scheduler.api_key')) || '';
  if (!serverUrl) { showToast('VPS-Server-URL fehlt — bitte in Einstellungen setzen'); return; }

  // Token-Check für YouTube und TikTok — nur Warnung, kein Block
  if (platform === 'youtube' || platform === 'tiktok' || platform === 'tiktok_draft') {
    try {
      const res = await fetch(`${serverUrl}/api/auth-status/${platform === 'tiktok_draft' ? 'tiktok' : platform}`);
      if (res.ok) {
        const status = await res.json();
        if (status.connected === false) {
          showToast(`⚠️ ${platform === 'youtube' ? 'YouTube' : 'TikTok'} nicht verbunden — bitte in Einstellungen verbinden`);
        }
      }
    } catch { /* VPS nicht erreichbar — ignorieren */ }
  }

  isScheduling.value = true;
  scheduleStatus.value[platform] = 'uploading';

  try {
    const scheduledIso = new Date(scheduledAt.value).toISOString();

    // YouTube: Hauptvideo-Link ans Ende der Description anhängen
    let desc = description.value;
    if (platform === 'youtube' && mainVideoUrl.value.trim()) {
      const id = extractYouTubeId(mainVideoUrl.value.trim());
      const link = id ? `https://youtu.be/${id}` : mainVideoUrl.value.trim();
      desc += `\n\n📺 Ganze Folge: ${link}`;
    }

    const resultRaw = await invoke<string>('schedule_clip', {
      clipPath:    selectedClip.value.rel_path,
      platform,
      description: desc,
      scheduledAt: scheduledIso,
      serverUrl,
      uploadUrl:   uploadUrl || null,
      apiKey,
      youtubeUrl:  mainVideoUrl.value.trim() || null,
    });

    scheduleStatus.value[platform] = 'done';
    showToast(`✓ ${platform.toUpperCase()} geplant`);

    // Job-Status kurz pollen — schnelle Fehler (z.B. invalidTitle, invalid_grant) sofort anzeigen
    try {
      const jobId = JSON.parse(resultRaw)?.job_id;
      if (jobId && serverUrl) {
        pollJobStatus(jobId, serverUrl, platform);
      }
    } catch { /* ignorieren */ }

    // Plattform als geplant markieren (nur für Plattformen mit done-Flags)
    if (selectedClip.value) {
      const platformMap: Record<string, 'yt' | 'tt' | 'ig'> = { youtube: 'yt', tiktok: 'tt', instagram: 'ig' };
      const flag = platformMap[platform];
      if (flag) {
        await invoke('toggle_clip_done_platform', { clipPath: selectedClip.value.rel_path, platform: flag });
        const c = clips.value.find(c => c.rel_path === selectedClip.value!.rel_path);
        if (c) {
          if (platform === 'youtube')   { c.done_yt = true; selectedClip.value.done_yt = true; }
          if (platform === 'tiktok')    { c.done_tt = true; selectedClip.value.done_tt = true; }
          if (platform === 'instagram') { c.done_ig = true; selectedClip.value.done_ig = true; }
          c.is_done = c.done_yt && c.done_tt && c.done_ig;
        }
      }
    }
  } catch (e) {
    scheduleStatus.value[platform] = 'failed';
    showToast('Fehler: ' + String(e));
  } finally {
    isScheduling.value = false;
  }
}

async function pollJobStatus(jobId: string, serverUrl: string, platform: string) {
  const label = platform.toUpperCase().replace('_DRAFT', ' ENTWURF');
  for (let i = 0; i < 12; i++) {
    await new Promise(r => setTimeout(r, 5000));
    try {
      const res = await fetch(`${serverUrl}/api/status/${jobId}`);
      if (!res.ok) continue;
      const data = await res.json();
      if (data.status === 'done') {
        if (platform === 'tiktok_draft') {
          showToast('✅ Entwurf erfolgreich an TikTok übertragen (video.upload)');
        }
        return;
      }
      if (data.status === 'failed') {
        const err = data.error || 'Unbekannter Fehler';
        showToast(`❌ ${label} Upload fehlgeschlagen: ${err}`, true);
        scheduleStatus.value[platform] = 'failed';
        return;
      }
    } catch { return; }
  }
}

async function scheduleAll() {
  resetScheduleStatus();
  for (const p of ['youtube', 'tiktok', 'instagram', 'pinterest', 'snapchat'] as const) {
    await scheduleForPlatform(p);
  }
}

// ─── Sofort posten (Demo/Test für App-Review) ─────────────────────────────────
// Postet ohne auf den geplanten Zeitpunkt zu warten — der Server feuert die API direkt.
const isPostingNow = ref(false);

async function postNowForPlatform(platform: 'youtube' | 'tiktok' | 'instagram' | 'tiktok_draft') {
  if (!selectedClip.value) { showToast('Kein Clip ausgewählt'); return; }
  if (isPostingNow.value) return;

  const { getStore } = await import('../store');
  const store = await getStore();
  const serverUrl = (await store.get<string>('scheduler.server_url')) || '';
  const uploadUrl = (await store.get<string>('scheduler.upload_url')) || '';
  const apiKey    = (await store.get<string>('scheduler.api_key')) || '';
  if (!serverUrl) { showToast('VPS-Server-URL fehlt — bitte in Einstellungen setzen'); return; }

  isPostingNow.value = true;
  scheduleStatus.value[platform] = 'uploading';
  try {
    let desc = description.value;
    if (platform === 'youtube' && mainVideoUrl.value.trim()) {
      const id = extractYouTubeId(mainVideoUrl.value.trim());
      const link = id ? `https://youtu.be/${id}` : mainVideoUrl.value.trim();
      desc += `\n\n📺 Ganze Folge: ${link}`;
    }
    const resultRaw = await invoke<string>('schedule_clip', {
      clipPath:    selectedClip.value.rel_path,
      platform,
      description: desc,
      scheduledAt: new Date().toISOString(),
      serverUrl,
      uploadUrl:   uploadUrl || null,
      apiKey,
      youtubeUrl:  mainVideoUrl.value.trim() || null,
      postNow:     true,
    });
    scheduleStatus.value[platform] = 'done';
    showToast(`⚡ ${platform.toUpperCase().replace('_DRAFT', ' ENTWURF')} wird jetzt gepostet…`);
    try {
      const jobId = JSON.parse(resultRaw)?.job_id;
      if (jobId) pollJobStatus(jobId, serverUrl, platform);
    } catch { /* ignorieren */ }
  } catch (e) {
    scheduleStatus.value[platform] = 'failed';
    showToast('Fehler: ' + String(e));
  } finally {
    isPostingNow.value = false;
  }
}

// ─── Bulk Scheduling (alle Clips im Ordner) ───────────────────────────────────

const isBulkScheduling = ref(false);
const bulkProgress = ref({ current: 0, total: 0, platform: '', clip: '' });

async function checkDiskSpace(serverUrl: string): Promise<boolean> {
  try {
    const res = await fetch(`${serverUrl}/api/disk-space`);
    if (!res.ok) return true; // Wenn Endpoint nicht erreichbar, fortfahren
    const data = await res.json();
    const availableMb = data.available_mb as number;
    if (availableMb < 500) {
      showToast(`⚠️ VPS Speicher kritisch: nur noch ${availableMb} MB frei!`);
      return false;
    }
    if (availableMb < 2000) {
      showToast(`⚠️ VPS Speicher niedrig: ${availableMb} MB frei — fahre fort…`);
    }
    return true;
  } catch { return true; }
}

async function scheduleBulkForPlatform(platform: 'youtube' | 'tiktok' | 'instagram' | 'pinterest' | 'snapchat') {
  if (isBulkScheduling.value) return;

  const { getStore } = await import('../store');
  const store = await getStore();
  const serverUrl = (await store.get<string>('scheduler.server_url')) || '';
  const uploadUrl = (await store.get<string>('scheduler.upload_url')) || '';
  const apiKey    = (await store.get<string>('scheduler.api_key')) || '';
  if (!serverUrl) { showToast('VPS-Server-URL fehlt — bitte in Einstellungen setzen'); return; }

  // PT/SC haben keine done-Flags — alle mit Beschreibung + Datum planen
  const allPending = clips.value.filter(c => {
    if (platform === 'youtube')   return !c.done_yt;
    if (platform === 'tiktok')    return !c.done_tt;
    if (platform === 'instagram') return !c.done_ig;
    return true; // pinterest + snapchat: kein done-Flag, immer einschließen
  });

  if (allPending.length === 0) {
    showToast(`Alle Clips bereits auf ${platform.toUpperCase()} hochgeladen`);
    return;
  }

  // Clips ohne Beschreibung oder ohne Datum überspringen
  const skippedNoDesc = allPending.filter(c => !c.has_description);
  const skippedNoDate = allPending.filter(c => c.has_description && !c.has_schedule);
  const pending       = allPending.filter(c => c.has_description && c.has_schedule);

  const skippedTotal = skippedNoDesc.length + skippedNoDate.length;
  if (skippedTotal > 0) {
    const parts = [];
    if (skippedNoDesc.length) parts.push(`${skippedNoDesc.length} ohne Beschreibung`);
    if (skippedNoDate.length) parts.push(`${skippedNoDate.length} ohne Datum`);
    showToast(`⚠️ Übersprungen: ${parts.join(', ')}`);
    await new Promise(r => setTimeout(r, 2000));
  }

  if (pending.length === 0) {
    showToast('Keine Clips mit Beschreibung + Datum zum Planen');
    return;
  }

  // Speicherplatz prüfen — grobe Schätzung: 30MB * Anzahl pending
  const ok = await checkDiskSpace(serverUrl);
  if (!ok) return;

  const label = platform === 'youtube' ? 'YouTube' : platform === 'tiktok' ? 'TikTok' : platform === 'instagram' ? 'Instagram' : platform === 'pinterest' ? 'Pinterest' : 'Snapchat';
  const confirmed = window.confirm(`${pending.length} Clips auf ${label} planen?\n\nBereits abgehakte werden übersprungen.`);
  if (!confirmed) return;

  isBulkScheduling.value = true;
  bulkProgress.value = { current: 0, total: pending.length, platform: label, clip: '' };

  let done = 0, failed = 0;

  for (const clip of pending) {
    bulkProgress.value.current = done + failed + 1;
    bulkProgress.value.clip = clip.name;

    // Speicher alle 5 Clips neu prüfen
    if ((done + failed) % 5 === 0 && done + failed > 0) {
      const spaceOk = await checkDiskSpace(serverUrl);
      if (!spaceOk) {
        showToast(`⛔ Bulk-Upload gestoppt: VPS Speicher zu voll! ${done} Clips geplant.`);
        break;
      }
    }

    try {
      const clipDate = await invoke<string>('get_clip_schedule', { clipPath: clip.rel_path });
      const scheduledDate = new Date(clipDate.trim());
      if (scheduledDate.getTime() < Date.now() + 60 * 1000) {
        failed++;
        showToast(`⚠️ ${clip.name}: Datum liegt in der Vergangenheit — übersprungen`);
        continue;
      }
      const scheduledIso = scheduledDate.toISOString();
      let descText = await invoke<string>('get_clip_description', { clipPath: clip.rel_path });
      if (platform === 'youtube' && mainVideoUrl.value.trim()) {
        const id = extractYouTubeId(mainVideoUrl.value.trim());
        const link = id ? `https://youtu.be/${id}` : mainVideoUrl.value.trim();
        descText += `\n\n📺 Ganze Folge: ${link}`;
      }
      await invoke<string>('schedule_clip', {
        clipPath: clip.rel_path,
        platform,
        description: descText,
        scheduledAt: scheduledIso,
        serverUrl,
        uploadUrl: uploadUrl || null,
        apiKey,
      });

      // Als erledigt markieren
      if (platform === 'youtube') {
        await invoke('toggle_clip_done_platform', { clipPath: clip.rel_path, platform: 'yt' });
        clip.done_yt = true;
      } else if (platform === 'tiktok') {
        await invoke('toggle_clip_done_platform', { clipPath: clip.rel_path, platform: 'tt' });
        clip.done_tt = true;
      } else if (platform === 'instagram') {
        await invoke('toggle_clip_done_platform', { clipPath: clip.rel_path, platform: 'ig' });
        clip.done_ig = true;
      }
      clip.is_done = clip.done_yt && clip.done_tt && clip.done_ig;
      done++;
    } catch (e) {
      failed++;
      console.error(`Fehler bei ${clip.name}:`, e);
    }
  }

  isBulkScheduling.value = false;
  showToast(`✓ ${done} Clips geplant${failed ? `, ${failed} fehlgeschlagen` : ''}`);
}

// ─── AI Prompt ────────────────────────────────────────────────────────────────

const AI_PROMPT = `Du bist ein Social-Media-Experte fuer TikTok, Instagram Reels und YouTube Shorts. Ich sende dir ein vertikales Kurzvideo.

Aufgabe:
1. Bewerte das Video kurz (Inhalt, Stimmung, Zielgruppe, virales Potenzial) in 2-3 Saetzen.
2. Schreibe eine kurze, authentische deutsche Caption und genau 3-4 virale, zum Video passende Hashtags.
3. Empfiehl die beste Posting-Zeit mit kurzer Begruendung.

Gib Caption und Hashtags klar getrennt aus, damit ich sie direkt kopieren kann.`;

// ─── AI System Prompt (API-Aufrufe) ──────────────────────────────────────────

const AI_SYSTEM_PROMPT = `Du bist ein Caption- und Hashtag-Spezialist fuer vertikale Kurzvideos (TikTok, Instagram Reels, YouTube Shorts). Du erhaeltst ein vertikales Kurzvideo und gibst AUSSCHLIESSLICH das fertige Caption-/Hashtag-Paket zurueck - keine Analyse, keine Erklaerungen, keine Anfuehrungszeichen.

Regeln:
- Sprache: Deutsch (ausser der Clip ist klar englischsprachig).
- Caption: kurz, authentisch, neugierig machend. Kein Werbe-Sprech. Starker Hook in der ersten Zeile.
- Danach genau 3-4 virale, zum Videoinhalt passende Hashtags in einer Zeile.
- Maximal 4 Hashtags. Keine generischen Spam-Tags wie #fyp #viral #foryou #trending.
- Maximal 1-2 Emojis, nur wenn sie zum Inhalt passen.

Ausgabeformat (genau so, nichts weiter):
[Caption-Text]

[#hashtag1 #hashtag2 #hashtag3]`;

async function copyPrompt() {
  await navigator.clipboard.writeText(AI_PROMPT);
  showToast('Prompt kopiert!');
}

// ─── AI Hashtag Generation ───────────────────────────────────────────────────

const isGenerating = ref(false);

function hasKeyFor(provider: 'gemini' | 'groq'): boolean {
  return provider === 'gemini' ? !!geminiKey.value.trim() : !!groqKey.value.trim();
}

async function generateHashtagsForClip(clip: ClipInfo, provider: 'gemini' | 'groq'): Promise<string | null> {
  if (!hasKeyFor(provider)) {
    return `${provider === 'gemini' ? 'Gemini' : 'Groq'} API-Key fehlt`;
  }
  try {
    let result: string;
    if (provider === 'gemini') {
      result = await invoke<string>('generate_hashtags_gemini', {
        clipPath: clip.rel_path,
        apiKey: geminiKey.value,
        prompt: AI_SYSTEM_PROMPT,
      });
    } else {
      const existing = await invoke<string>('get_clip_description', { clipPath: clip.rel_path });
      result = await invoke<string>('generate_hashtags_groq', {
        clipPath: clip.rel_path,
        apiKey: groqKey.value,
        prompt: AI_SYSTEM_PROMPT,
        existingDesc: existing,
      });
    }
    await invoke('save_clip_description', { clipPath: clip.rel_path, text: result });
    clip.has_description = true;
    if (selectedClip.value?.rel_path === clip.rel_path) {
      description.value = result;
    }
    return null; // null = success
  } catch (e) {
    return String(e); // Fehlertext zurückgeben statt verschlucken
  }
}

async function generateForSelected(provider?: 'gemini' | 'groq') {
  if (!selectedClip.value || isGenerating.value) return;
  const p = provider || aiDefaultProvider.value;
  if (!hasKeyFor(p)) {
    showToast(`${p === 'gemini' ? 'Gemini' : 'Groq'} API-Key fehlt — in Einstellungen setzen`);
    return;
  }
  const clip = selectedClip.value;
  isGenerating.value = true;
  description.value = '';

  // Set up stream listener before invoking
  const unlisten = await listen<{ text: string; done: boolean; error?: string }>('ai-stream', (event) => {
    const { text, done, error } = event.payload;
    if (error) {
      showToast('Fehler: ' + error);
      isGenerating.value = false;
      unlisten();
      return;
    }
    if (!done) {
      description.value += text;
    } else {
      unlisten();
      isGenerating.value = false;
      if (description.value.trim()) {
        invoke('save_clip_description', { clipPath: clip.rel_path, text: description.value });
        const c = clips.value.find(c => c.rel_path === clip.rel_path);
        if (c) c.has_description = true;
        showToast('Caption gespeichert');
      }
    }
  });

  try {
    if (p === 'gemini') {
      invoke('generate_hashtags_gemini_stream', {
        clipPath: clip.rel_path,
        apiKey: geminiKey.value,
        prompt: AI_SYSTEM_PROMPT,
      }).catch((e: unknown) => { showToast('Fehler: ' + String(e)); unlisten(); isGenerating.value = false; });
    } else {
      const existing = await invoke<string>('get_clip_description', { clipPath: clip.rel_path });
      invoke('generate_hashtags_groq_stream', {
        clipPath: clip.rel_path,
        apiKey: groqKey.value,
        prompt: AI_SYSTEM_PROMPT,
        existingDesc: existing,
      }).catch((e: unknown) => { showToast('Fehler: ' + String(e)); unlisten(); isGenerating.value = false; });
    }
  } catch (e) {
    showToast('Fehler: ' + String(e));
    unlisten();
    isGenerating.value = false;
  }
}

async function generateForAll() {
  if (isGenerating.value) return;
  const p = aiDefaultProvider.value;
  if (!hasKeyFor(p)) {
    showToast(`${p === 'gemini' ? 'Gemini' : 'Groq'} API-Key fehlt — in Einstellungen setzen`);
    return;
  }
  const pending = clips.value.filter(c => !c.has_description);
  if (pending.length === 0) { showToast('Alle Clips haben bereits eine Beschreibung'); return; }
  isGenerating.value = true;
  let done = 0;
  const errors: { clip: string; msg: string }[] = [];

  for (const clip of pending) {
    showToast(`⏳ ${done + 1 + errors.length}/${pending.length}: ${clip.name}`);
    const err = await generateHashtagsForClip(clip, p);
    if (err === null) {
      done++;
    } else {
      errors.push({ clip: clip.name, msg: err });
    }
  }

  isGenerating.value = false;

  if (errors.length === 0) {
    showToast(`✓ Alle ${done} Clips generiert`);
  } else {
    showToast(`✓ ${done} generiert, ${errors.length} fehlgeschlagen`);
    // Kurz warten, dann jeden Fehler einzeln anzeigen
    for (const e of errors) {
      await new Promise(r => setTimeout(r, 3500));
      showToast(`❌ ${e.clip}: ${e.msg}`);
    }
  }
}

// ─── Add Clips (file dialog) ──────────────────────────────────────────────────

async function addClipsViaDialog() {
  if (!selectedFolder.value) { showToast('Bitte zuerst einen Ordner auswählen'); return; }
  const files = await open({
    multiple: true,
    filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'mkv', 'avi', 'webm'] }],
    title: 'Clips hinzufügen',
  });
  if (!files) return;
  const list = Array.isArray(files) ? files : [files];
  const destFolder = clipsRoot.value + '/' + selectedFolder.value;
  let count = 0;
  for (const src of list) {
    try {
      await invoke('copy_clip_to_folder', { src, destFolder });
      count++;
    } catch { /* ignore */ }
  }
  if (count > 0) {
    showToast(`${count} Clip(s) hinzugefügt`);
    await loadClips();
  }
}

// ─── Drag & Drop ──────────────────────────────────────────────────────────────

const isDragging = ref(false);

function onDragOver(e: DragEvent) {
  if (!selectedFolder.value) return;
  e.preventDefault();
  isDragging.value = true;
}
function onDragLeave() { isDragging.value = false; }

async function onDrop(e: DragEvent) {
  e.preventDefault();
  isDragging.value = false;
  if (!selectedFolder.value) { showToast('Bitte zuerst einen Ordner auswählen'); return; }
  const files = Array.from(e.dataTransfer?.files || []);
  if (files.length === 0) return;
  const destFolder = clipsRoot.value + '/' + selectedFolder.value;
  let count = 0;
  for (const file of files) {
    const path = (file as any).path as string | undefined;
    if (!path) continue;
    try {
      await invoke('copy_clip_to_folder', { src: path, destFolder });
      count++;
    } catch { /* ignore */ }
  }
  if (count > 0) {
    showToast(`${count} Clip(s) hinzugefügt`);
    await loadClips();
  }
}

// ─── Ctrl+V paste ─────────────────────────────────────────────────────────────

async function onPaste(e: Event) {
  const ce = e as ClipboardEvent;
  if (!selectedFolder.value) return;
  const files = Array.from(ce.clipboardData?.files || []);
  if (files.length === 0) return;
  const destFolder = clipsRoot.value + '/' + selectedFolder.value;
  let count = 0;
  for (const file of files) {
    const path = (file as any).path as string | undefined;
    if (!path) continue;
    try {
      await invoke('copy_clip_to_folder', { src: path, destFolder });
      count++;
    } catch { /* ignore */ }
  }
  if (count > 0) {
    showToast(`${count} Clip(s) eingefügt`);
    await loadClips();
  }
}

// ─── HTTP Server ──────────────────────────────────────────────────────────────

const serverRunning = ref(false);
const serverUrl = ref('');
const showQr = ref(false);
const qrDataUrl = ref('');
const localIp = ref('');

async function startServer() {
  if (!clipsRoot.value) { showToast('Bitte zuerst einen Clips-Ordner konfigurieren'); return; }
  try {
    const url = await invoke<string>('start_clips_server', {
      root: clipsRoot.value,
      port: serverPort.value,
    });
    serverUrl.value = url;
    serverRunning.value = true;
    await generateQr(url);
  } catch (e) {
    showToast('Server-Fehler: ' + String(e));
  }
}

async function stopServer() {
  await invoke('stop_clips_server');
  serverRunning.value = false;
  serverUrl.value = '';
  showQr.value = false;
}

async function generateQr(url: string) {
  qrDataUrl.value = await QRCode.toDataURL(url, { width: 200, margin: 1, color: { dark: '#ffffff', light: '#1e1e2e' } });
}

async function fetchLocalIp() {
  localIp.value = await invoke<string>('get_local_ip');
}

// ─── Token-Warnung ────────────────────────────────────────────────────────────

interface TokenWarn { platform: string; label: string; daysLeft: number; }
const tokenWarnings = ref<TokenWarn[]>([]);

async function checkTokenExpiry() {
  const { getStore } = await import('../store');
  const store = await getStore();
  const serverUrl = ((await store.get<string>('scheduler.server_url')) || '').replace(/\/$/, '');
  if (!serverUrl) return;

  const platforms = [
    { id: 'youtube',   label: 'YouTube' },
    { id: 'tiktok',    label: 'TikTok' },
    { id: 'instagram', label: 'Instagram' },
  ];
  const warns: TokenWarn[] = [];
  for (const p of platforms) {
    try {
      const res = await fetch(`${serverUrl}/api/auth-status/${p.id}`);
      if (!res.ok) continue;
      const data = await res.json();
      if (!data.connected || !data.expires_at) continue;
      const daysLeft = Math.floor((data.expires_at - Date.now() / 1000) / 86400);
      // Short-lived tokens (< 2 days) auto-refresh — don't warn
      if (daysLeft >= 2 && daysLeft < 60) warns.push({ platform: p.id, label: p.label, daysLeft });
    } catch { /* server offline */ }
  }
  tokenWarnings.value = warns;
}

// ─── Guide accordion ──────────────────────────────────────────────────────────

const showGuide = ref(false);

// ─── Dashboard Computed ────────────────────────────────────────────────────────
const ytCount = computed(() => clips.value.filter(c => c.done_yt).length);
const ttCount = computed(() => clips.value.filter(c => c.done_tt).length);
const igCount = computed(() => clips.value.filter(c => c.done_ig).length);
const uploadedCount = computed(() => ytCount.value + ttCount.value + igCount.value);
const allPlatformsDone = computed(() =>
  clips.value.length > 0 && uploadedCount.value === clips.value.length * 3
);
const missingUploads = computed(() => {
  return clips.value.flatMap(c => {
    const missing: string[] = [];
    if (!c.done_yt) missing.push(c.name + ' (YT)');
    if (!c.done_tt) missing.push(c.name + ' (TT)');
    if (!c.done_ig) missing.push(c.name + ' (IG)');
    return missing;
  }).slice(0, 5);
});

// ─── Lifecycle ────────────────────────────────────────────────────────────────

onMounted(async () => {
  await loadSettings();
  await fetchLocalIp();
  if (clipsRoot.value) await loadFolders();
  window.addEventListener('paste', onPaste);
  checkTokenExpiry();
});

onUnmounted(async () => {
  window.removeEventListener('paste', onPaste);
  if (serverRunning.value) await stopServer();
});
</script>

<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-white">Clips</h1>
        <p class="text-sm text-gray-500 mt-0.5">TikTok-Clips verwalten und aufs Handy übertragen</p>
      </div>
      <button @click="rootInputVisible = !rootInputVisible"
        class="flex items-center gap-2 px-4 py-2 bg-white/5 hover:bg-white/10 border border-white/10 rounded-xl text-sm text-gray-300 transition-colors">
        ⚙ Einstellungen
      </button>
    </div>

    <!-- Token-Warnungen -->
    <div v-if="tokenWarnings.length > 0" class="space-y-1.5">
      <div v-for="w in tokenWarnings" :key="w.platform"
        :class="['flex items-center justify-between px-4 py-2.5 rounded-xl text-sm border',
          w.daysLeft < 0  ? 'bg-red-900/30 border-red-600/40 text-red-300' :
          w.daysLeft < 14 ? 'bg-orange-900/30 border-orange-600/40 text-orange-300' :
                            'bg-yellow-900/20 border-yellow-600/30 text-yellow-300']">
        <span>
          {{ w.daysLeft < 0 ? '❌' : '⚠️' }}
          <strong>{{ w.label }}</strong>:
          {{ w.daysLeft < 0 ? 'Token abgelaufen!' : `Token läuft in ${w.daysLeft} Tagen ab` }}
        </span>
        <router-link to="/settings"
          class="text-xs underline opacity-70 hover:opacity-100 transition-opacity">
          Jetzt erneuern →
        </router-link>
      </div>
    </div>

    <!-- Settings panel -->
    <div v-if="rootInputVisible" class="p-4 bg-dark-900/60 border border-white/10 rounded-2xl space-y-3">
      <div class="flex gap-2">
        <input v-model="clipsRoot" placeholder="Clips-Root-Ordner..."
          class="flex-1 bg-dark-950/80 border border-white/10 rounded-xl px-3 py-2 text-sm text-gray-200 placeholder-gray-600 outline-none focus:border-primary/50" />
        <button @click="browseRoot"
          class="px-4 py-2 bg-primary/20 hover:bg-primary/30 border border-primary/30 text-primary rounded-xl text-sm transition-colors">
          Durchsuchen
        </button>
        <button @click="saveSettings(); loadFolders(); rootInputVisible = false"
          class="px-4 py-2 bg-green-600/20 hover:bg-green-600/30 border border-green-600/30 text-green-400 rounded-xl text-sm transition-colors">
          ✓ Speichern
        </button>
      </div>
      <div class="flex items-center gap-3">
        <span class="text-xs text-gray-500">Port:</span>
        <input v-model.number="serverPort" type="number" min="1024" max="65535"
          class="w-24 bg-dark-950/80 border border-white/10 rounded-lg px-3 py-1.5 text-sm text-gray-200 outline-none focus:border-primary/50" />
      </div>
    </div>

    <!-- Main layout -->
    <div class="flex gap-4" style="min-height: 500px">
      <!-- Folder sidebar -->
      <div class="w-52 flex-shrink-0 flex flex-col gap-2">
        <div class="text-xs font-semibold text-gray-500 uppercase tracking-wider px-1 mb-1">Ordner</div>

        <button v-for="f in folders" :key="f"
          @click="selectFolder(f)"
          :class="['w-full text-left px-3 py-2.5 rounded-xl text-sm font-medium transition-colors',
            selectedFolder === f
              ? 'bg-primary/20 border border-primary/30 text-primary'
              : 'bg-white/5 hover:bg-white/10 border border-white/8 text-gray-300']">
          🎬 {{ f }}
        </button>

        <div v-if="!folders.length && clipsRoot" class="text-xs text-gray-600 px-1">Noch keine Ordner</div>
        <div v-if="!clipsRoot" class="text-xs text-gray-600 px-1">Clips-Root konfigurieren</div>

        <!-- New folder -->
        <div v-if="showNewFolder" class="flex flex-col gap-1.5">
          <input v-model="newFolderName" placeholder="Folge 1..."
            @keydown.enter="createFolder"
            @keydown.escape="showNewFolder = false; newFolderName = ''"
            class="w-full bg-dark-950/80 border border-white/10 rounded-xl px-3 py-2 text-sm text-gray-200 placeholder-gray-600 outline-none focus:border-primary/50"
            autofocus />
          <div class="flex gap-1.5">
            <button @click="createFolder"
              class="flex-1 px-2 py-1.5 bg-primary/20 hover:bg-primary/30 border border-primary/30 text-primary text-xs rounded-lg transition-colors">
              Erstellen
            </button>
            <button @click="showNewFolder = false; newFolderName = ''"
              class="flex-1 px-2 py-1.5 bg-white/5 hover:bg-white/10 border border-white/10 text-gray-400 text-xs rounded-lg transition-colors">
              Abbrechen
            </button>
          </div>
        </div>

        <button v-if="!showNewFolder && clipsRoot" @click="showNewFolder = true"
          class="w-full px-3 py-2 bg-white/4 hover:bg-white/8 border border-dashed border-white/15 text-gray-500 hover:text-gray-400 text-sm rounded-xl transition-colors">
          + Neuer Ordner
        </button>
      </div>

      <!-- Clips area -->
      <div class="flex-1 flex flex-col gap-4">
        <!-- Video player + description (shown when clip selected) -->
        <div v-if="selectedClip" class="p-4 bg-dark-900/60 border border-white/10 rounded-2xl space-y-3">
          <video
            ref="videoEl"
            :src="convertFileSrc(selectedClip.rel_path)"
            :key="selectedClip.rel_path"
            controls
            preload="metadata"
            class="w-full max-h-72 rounded-xl bg-black object-contain"
          />


          <div class="flex items-start gap-3">
            <div class="flex-1 relative">
              <textarea v-model="description" @input="onDescriptionInput"
                placeholder="Beschreibung, Hashtags..."
                rows="3"
                class="w-full bg-dark-950/80 border border-white/10 rounded-xl px-3 py-2.5 text-sm text-gray-200 placeholder-gray-600 outline-none focus:border-primary/50 resize-none pr-10" />
              <button @click="copyDescription" title="Text kopieren"
                class="absolute top-2 right-2 p-1.5 rounded-lg bg-white/5 hover:bg-white/15 text-gray-400 hover:text-white transition-colors text-xs">
                📋
              </button>
            </div>
          </div>
          <!-- Schedule / Upload-Termin -->
          <div class="space-y-1.5">
            <div class="flex items-center gap-2">
              <span class="text-xs text-gray-500 w-28 flex-shrink-0">📅 Upload-Termin</span>
              <input
                type="datetime-local"
                v-model="scheduledAt"
                @input="onScheduleInput"
                class="flex-1 bg-dark-950/80 border border-white/10 rounded-xl px-3 py-1.5 text-sm text-gray-200 outline-none focus:border-primary/50 [color-scheme:dark]" />
              <button v-if="scheduledAt" @click="scheduledAt = ''; saveSchedule()"
                class="text-gray-500 hover:text-red-400 transition-colors text-sm px-1"
                title="Termin löschen">
                ✕
              </button>
            </div>
            <div v-if="scheduledAt" class="pl-30 text-xs font-medium text-primary pl-[7.5rem]">
              {{ formatScheduleLabel(scheduledAt) }}
            </div>
          </div>

          <!-- ⚠️ Konflikt-Warnung: anderer Clip am gleichen Tag geplant -->
          <div v-if="scheduleConflicts.length > 0"
            class="rounded-xl border border-orange-500/60 bg-orange-500/10 p-3 space-y-1">
            <div class="flex items-center gap-2 text-orange-400 font-semibold text-sm">
              ⚠️ {{ scheduleConflicts.length === 1 ? 'Ein anderer Clip' : scheduleConflicts.length + ' andere Clips' }} am gleichen Tag geplant!
            </div>
            <div v-for="c in scheduleConflicts" :key="c.rel_path" class="text-xs text-orange-300/80 pl-6">
              <span class="font-medium text-orange-300">{{ c.folder }}</span> / {{ c.clip }}
              <span class="text-orange-500/70 ml-1">· {{ formatScheduleLabel(c.scheduled_at) }}</span>
            </div>
          </div>

          <div class="flex gap-2 flex-wrap">
            <button @click="togglePlatformDone(selectedClip!, 'yt')"
              :class="['flex items-center gap-2 px-4 py-2 border rounded-xl text-sm transition-colors',
                selectedClip?.done_yt
                  ? 'bg-red-600/20 border-red-600/30 text-red-400'
                  : 'bg-white/6 hover:bg-white/12 border-white/10 text-gray-300']">
              {{ selectedClip?.done_yt ? '✅' : '☐' }} YouTube
            </button>
            <button @click="togglePlatformDone(selectedClip!, 'tt')"
              :class="['flex items-center gap-2 px-4 py-2 border rounded-xl text-sm transition-colors',
                selectedClip?.done_tt
                  ? 'bg-gray-200/20 border-gray-200/30 text-gray-200'
                  : 'bg-white/6 hover:bg-white/12 border-white/10 text-gray-300']">
              {{ selectedClip?.done_tt ? '✅' : '☐' }} TikTok
            </button>
            <button @click="togglePlatformDone(selectedClip!, 'ig')"
              :class="['flex items-center gap-2 px-4 py-2 border rounded-xl text-sm transition-colors',
                selectedClip?.done_ig
                  ? 'bg-pink-600/20 border-pink-600/30 text-pink-400'
                  : 'bg-white/6 hover:bg-white/12 border-white/10 text-gray-300']">
              {{ selectedClip?.done_ig ? '✅' : '☐' }} Instagram
            </button>
            <button @click="togglePlatformDone(selectedClip!, 'pt')"
              :class="['flex items-center gap-2 px-4 py-2 border rounded-xl text-sm transition-colors',
                selectedClip?.done_pt
                  ? 'bg-red-950/30 border-red-600/30 text-red-400'
                  : 'bg-white/6 hover:bg-white/12 border-white/10 text-gray-300']">
              {{ selectedClip?.done_pt ? '✅' : '☐' }} Pinterest
            </button>
            <button @click="togglePlatformDone(selectedClip!, 'sc')"
              :class="['flex items-center gap-2 px-4 py-2 border rounded-xl text-sm transition-colors',
                selectedClip?.done_sc
                  ? 'bg-yellow-600/20 border-yellow-500/30 text-yellow-400'
                  : 'bg-white/6 hover:bg-white/12 border-white/10 text-gray-300']">
              {{ selectedClip?.done_sc ? '✅' : '☐' }} Snapchat
            </button>
            <button @click="copyFileToClipboard"
              class="flex items-center gap-2 px-4 py-2 bg-white/6 hover:bg-white/12 border border-white/10 text-gray-300 rounded-xl text-sm transition-colors">
              📁 Datei kopieren (Strg+V)
            </button>
            <div class="flex rounded-xl overflow-hidden border border-pink-600/30">
              <button @click="generateForSelected()" :disabled="isGenerating"
                class="flex items-center gap-2 px-4 py-2 bg-pink-600/20 hover:bg-pink-600/30 disabled:opacity-50 disabled:cursor-not-allowed text-pink-400 text-sm transition-colors"
                :title="`Caption + Hashtags mit ${aiDefaultProvider === 'gemini' ? 'Gemini (Video)' : 'Groq (Text)'} generieren`">
                🏷️ {{ isGenerating ? 'Generiere…' : 'Hashtags generieren' }}
              </button>
              <button @click="generateForSelected(aiDefaultProvider === 'gemini' ? 'groq' : 'gemini')" :disabled="isGenerating"
                class="px-2 py-2 bg-pink-600/10 hover:bg-pink-600/25 disabled:opacity-50 disabled:cursor-not-allowed text-pink-400 text-xs border-l border-pink-600/30 transition-colors"
                :title="`Stattdessen mit ${aiDefaultProvider === 'gemini' ? 'Groq' : 'Gemini'} generieren`">
                ⇄ {{ aiDefaultProvider === 'gemini' ? 'Groq' : 'Gemini' }}
              </button>
            </div>
            <button @click="clearVideo(); selectedClip = null"
              class="px-3 py-2 bg-white/4 hover:bg-white/8 border border-white/8 text-gray-500 rounded-xl text-sm transition-colors">
              ✕ Schließen
            </button>
          </div>

          <!-- Platform Scheduling -->
          <div class="space-y-2 pt-1 border-t border-white/8">
            <div class="flex flex-wrap gap-2">
              <button @click="scheduleAll(); resetScheduleStatus()" :disabled="isScheduling || !scheduledAt"
                class="flex items-center gap-1.5 px-3 py-1.5 bg-indigo-600/30 hover:bg-indigo-600/50 border border-indigo-500/40 text-indigo-300 rounded-lg text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                🗓 Alle planen
              </button>
              <button @click="scheduleStatus.youtube = 'idle'; scheduleForPlatform('youtube')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.youtube === 'done'   ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.youtube === 'failed' ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.youtube === 'uploading' ? 'bg-red-900/40 border-red-700/40 text-red-300 animate-pulse' :
                  'bg-red-900/30 hover:bg-red-900/50 border-red-700/40 text-red-400']">
                {{ scheduleStatus.youtube === 'done' ? '✅' : scheduleStatus.youtube === 'failed' ? '❌' : scheduleStatus.youtube === 'uploading' ? '⏳' : '▶' }}
                YouTube
              </button>
              <button @click="scheduleStatus.tiktok = 'idle'; scheduleForPlatform('tiktok')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.tiktok === 'done'   ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.tiktok === 'failed' ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.tiktok === 'uploading' ? 'bg-neutral-700/60 border-white/20 text-gray-200 animate-pulse' :
                  'bg-neutral-800/50 hover:bg-neutral-700/60 border-white/15 text-gray-200']">
                {{ scheduleStatus.tiktok === 'done' ? '✅' : scheduleStatus.tiktok === 'failed' ? '❌' : scheduleStatus.tiktok === 'uploading' ? '⏳' : '♪' }}
                TikTok
              </button>
              <button @click="scheduleStatus.tiktok_draft = 'idle'; scheduleForPlatform('tiktok_draft')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.tiktok_draft === 'done'     ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.tiktok_draft === 'failed'   ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.tiktok_draft === 'uploading' ? 'bg-neutral-700/60 border-white/20 text-gray-200 animate-pulse' :
                  'bg-yellow-900/20 hover:bg-yellow-900/40 border-yellow-700/30 text-yellow-500']">
                {{ scheduleStatus.tiktok_draft === 'done' ? '✅' : scheduleStatus.tiktok_draft === 'failed' ? '❌' : scheduleStatus.tiktok_draft === 'uploading' ? '⏳' : '♪' }}
                TikTok Entwurf
              </button>
              <button @click="scheduleStatus.instagram = 'idle'; scheduleForPlatform('instagram')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.instagram === 'done'   ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.instagram === 'failed' ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.instagram === 'uploading' ? 'bg-pink-900/40 border-pink-700/40 text-pink-300 animate-pulse' :
                  'bg-pink-900/30 hover:bg-pink-900/50 border-pink-700/40 text-pink-400']">
                {{ scheduleStatus.instagram === 'done' ? '✅' : scheduleStatus.instagram === 'failed' ? '❌' : scheduleStatus.instagram === 'uploading' ? '⏳' : '◎' }}
                Instagram
              </button>
              <button @click="scheduleStatus.pinterest = 'idle'; scheduleForPlatform('pinterest')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.pinterest === 'done'   ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.pinterest === 'failed' ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.pinterest === 'uploading' ? 'bg-red-950/50 border-red-600/40 text-red-300 animate-pulse' :
                  'bg-red-950/30 hover:bg-red-950/50 border-red-600/40 text-red-300']">
                {{ scheduleStatus.pinterest === 'done' ? '✅' : scheduleStatus.pinterest === 'failed' ? '❌' : scheduleStatus.pinterest === 'uploading' ? '⏳' : '📌' }}
                Pinterest
              </button>
              <button @click="scheduleStatus.snapchat = 'idle'; scheduleForPlatform('snapchat')" :disabled="isScheduling || !scheduledAt"
                :class="['flex items-center gap-1.5 px-3 py-1.5 border rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed',
                  scheduleStatus.snapchat === 'done'   ? 'bg-green-900/40 border-green-600/40 text-green-400' :
                  scheduleStatus.snapchat === 'failed' ? 'bg-red-950/40 border-red-700/40 text-red-400' :
                  scheduleStatus.snapchat === 'uploading' ? 'bg-yellow-900/40 border-yellow-600/40 text-yellow-300 animate-pulse' :
                  'bg-yellow-900/20 hover:bg-yellow-900/40 border-yellow-600/30 text-yellow-400']">
                {{ scheduleStatus.snapchat === 'done' ? '✅' : scheduleStatus.snapchat === 'failed' ? '❌' : scheduleStatus.snapchat === 'uploading' ? '⏳' : '👻' }}
                Snapchat
              </button>
              <span v-if="!scheduledAt" class="text-xs text-gray-600 self-center">← Upload-Termin setzen</span>
            </div>

            <!-- Demo/Test: Sofort posten (für TikTok App-Review) — kein Termin nötig -->
            <div class="flex flex-wrap gap-2 items-center pt-1">
              <span class="text-xs text-gray-600">⚡ Sofort (Test):</span>
              <button @click="postNowForPlatform('tiktok')" :disabled="isPostingNow || !selectedClip"
                class="px-2.5 py-1 bg-neutral-800/50 hover:bg-neutral-700/60 border border-white/15 text-gray-200 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                ♪ TikTok jetzt
              </button>
              <button @click="postNowForPlatform('tiktok_draft')" :disabled="isPostingNow || !selectedClip"
                class="px-2.5 py-1 bg-yellow-900/20 hover:bg-yellow-900/40 border border-yellow-700/30 text-yellow-500 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                ♪ Entwurf jetzt
              </button>
              <button @click="postNowForPlatform('youtube')" :disabled="isPostingNow || !selectedClip"
                class="px-2.5 py-1 bg-red-900/30 hover:bg-red-900/50 border border-red-700/40 text-red-400 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                ▶ YouTube jetzt
              </button>
              <button @click="postNowForPlatform('instagram')" :disabled="isPostingNow || !selectedClip"
                class="px-2.5 py-1 bg-pink-900/30 hover:bg-pink-900/50 border border-pink-700/40 text-pink-400 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                ◎ Instagram jetzt
              </button>
            </div>

            <!-- Upload-Fortschrittsbalken -->
            <div v-if="isScheduling || isPostingNow" class="h-1 w-full bg-white/10 rounded-full overflow-hidden">
              <div class="h-full w-2/5 bg-indigo-500 rounded-full animate-indeterminate" />
            </div>
          </div>
        </div>

        <!-- Clip grid / drop zone -->
        <div
          @dragover="onDragOver"
          @dragleave="onDragLeave"
          @drop="onDrop"
          :class="['flex-1 rounded-2xl border-2 transition-colors p-3',
            isDragging
              ? 'border-primary bg-primary/10'
              : 'border-white/8 bg-dark-900/30']">

          <!-- Toolbar -->
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs text-gray-500">
              {{ selectedFolder ? selectedFolder + ' · ' + clips.length + ' Clips' : 'Keinen Ordner gewählt' }}
            </span>
            <div class="flex items-center gap-2">
              <button @click="copyPrompt"
                class="flex items-center gap-1.5 px-3 py-1.5 bg-purple-600/20 hover:bg-purple-600/30 border border-purple-600/30 text-purple-400 text-xs rounded-lg transition-colors"
                title="KI-Prompt für Clip-Bewertung, Caption & Hashtags kopieren">
                📋 Copy Prompt
              </button>
              <button v-if="clips.length > 0" @click="showDateSetter = !showDateSetter"
                class="flex items-center gap-1.5 px-3 py-1.5 bg-indigo-600/20 hover:bg-indigo-600/30 border border-indigo-600/30 text-indigo-400 text-xs rounded-lg transition-colors"
                title="Start-Datum setzen — berechnet optimale Uhrzeiten pro Tag automatisch">
                📅 Datum für alle
              </button>
              <button v-if="clips.length > 0" @click="generateForAll" :disabled="isGenerating"
                class="flex items-center gap-1.5 px-3 py-1.5 bg-pink-600/20 hover:bg-pink-600/30 disabled:opacity-50 disabled:cursor-not-allowed border border-pink-600/30 text-pink-400 text-xs rounded-lg transition-colors"
                :title="`Hashtags für alle Clips ohne Description generieren (${aiDefaultProvider === 'gemini' ? 'Gemini' : 'Groq'})`">
                🏷️ Alle generieren
              </button>
              <template v-if="clips.length > 0">
                <button @click="scheduleBulkForPlatform('youtube')" :disabled="isBulkScheduling"
                  class="flex items-center gap-1 px-3 py-1.5 bg-red-900/30 hover:bg-red-900/50 border border-red-700/40 text-red-400 text-xs rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Alle ungeplanten Clips auf YouTube planen">
                  ▶ Alle YT
                </button>
                <button @click="scheduleBulkForPlatform('tiktok')" :disabled="isBulkScheduling"
                  class="flex items-center gap-1 px-3 py-1.5 bg-neutral-800/50 hover:bg-neutral-700/60 border border-white/15 text-gray-200 text-xs rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Alle ungeplanten Clips auf TikTok planen">
                  ♪ Alle TT
                </button>
                <button @click="scheduleBulkForPlatform('instagram')" :disabled="isBulkScheduling"
                  class="flex items-center gap-1 px-3 py-1.5 bg-pink-900/30 hover:bg-pink-900/50 border border-pink-700/40 text-pink-400 text-xs rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Alle ungeplanten Clips auf Instagram planen">
                  ◎ Alle IG
                </button>
                <button @click="scheduleBulkForPlatform('pinterest')" :disabled="isBulkScheduling"
                  class="flex items-center gap-1 px-3 py-1.5 bg-red-950/30 hover:bg-red-950/50 border border-red-600/40 text-red-300 text-xs rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Alle Clips mit Datum auf Pinterest planen">
                  📌 Alle PT
                </button>
                <button @click="scheduleBulkForPlatform('snapchat')" :disabled="isBulkScheduling"
                  class="flex items-center gap-1 px-3 py-1.5 bg-yellow-900/20 hover:bg-yellow-900/40 border border-yellow-600/30 text-yellow-400 text-xs rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Alle Clips mit Datum auf Snapchat Spotlight planen">
                  👻 Alle SC
                </button>
              </template>
              <button v-if="selectedFolder" @click="addClipsViaDialog"
                class="flex items-center gap-1.5 px-3 py-1.5 bg-primary/20 hover:bg-primary/30 border border-primary/30 text-primary text-xs rounded-lg transition-colors">
                + Clips hinzufügen
              </button>
            </div>
          </div>

          <!-- Auto Date Setter Panel -->
          <div v-if="showDateSetter" class="p-3 bg-indigo-900/20 border border-indigo-500/30 rounded-xl space-y-2">
            <div class="text-xs text-indigo-300 font-medium">📅 Datum für alle Clips setzen</div>
            <div class="text-xs text-gray-500">Clip 1 = Startdatum, Clip 2 = +1 Tag usw. Uhrzeit wird automatisch nach Wochentag optimiert.</div>
            <div class="flex gap-2 items-center">
              <input type="date" v-model="dateSetterStart"
                class="bg-dark-950/80 border border-indigo-500/30 rounded-lg px-2 py-1 text-xs text-gray-200 outline-none [color-scheme:dark]" />
              <button @click="setDatesForAll" :disabled="!dateSetterStart || isSettingDates"
                class="px-3 py-1 bg-indigo-600/40 hover:bg-indigo-600/60 border border-indigo-500/40 text-indigo-300 rounded-lg text-xs transition-colors disabled:opacity-40">
                {{ isSettingDates ? '⏳ Setze…' : '✓ Anwenden' }}
              </button>
              <button @click="showDateSetter = false" class="text-gray-500 hover:text-gray-300 text-xs px-2">✕</button>
            </div>
            <div v-if="dateSetterStart" class="text-xs text-gray-600 space-y-0.5">
              <div v-for="(clip, i) in sortedClipsNumerically().slice(0, 5)" :key="clip.rel_path">
                {{ clip.name }} →
                {{ new Date(new Date(dateSetterStart + 'T12:00').setDate(new Date(dateSetterStart + 'T12:00').getDate() + i)).toLocaleDateString('de-DE', {weekday:'short', day:'2-digit', month:'2-digit'}) }}
                {{ OPTIMAL_TIMES[new Date(new Date(dateSetterStart + 'T12:00').setDate(new Date(dateSetterStart + 'T12:00').getDate() + i)).getDay()] }} Uhr
              </div>
              <div v-if="clips.length > 5" class="text-gray-700">… und {{ clips.length - 5 }} weitere</div>
            </div>
          </div>

          <!-- Main Video URL (YouTube-Verlinkung) -->
          <div v-if="selectedFolder" class="flex items-center gap-2 px-1">
            <span class="text-xs text-gray-600 flex-shrink-0">📺 Hauptvideo:</span>
            <input v-model="mainVideoUrl" @change="saveMainVideoUrl" @blur="saveMainVideoUrl"
              placeholder="https://youtu.be/... (für YouTube-Shorts-Verlinkung)"
              class="flex-1 bg-dark-950/60 border border-white/8 rounded-lg px-2 py-1 text-xs text-gray-400 placeholder-gray-700 outline-none focus:border-red-600/40 focus:text-gray-200" />
            <span v-if="mainVideoUrl && extractYouTubeId(mainVideoUrl)" class="text-xs text-green-500">✓</span>
            <span v-else-if="mainVideoUrl" class="text-xs text-orange-400">?</span>
          </div>

          <!-- Bulk Scheduling Progress -->
          <div v-if="isBulkScheduling" class="p-3 bg-indigo-900/20 border border-indigo-500/30 rounded-xl">
            <div class="flex items-center justify-between text-xs mb-2">
              <span class="text-indigo-300 font-medium">⏳ {{ bulkProgress.platform }} wird geplant…</span>
              <span class="text-indigo-400">{{ bulkProgress.current }} / {{ bulkProgress.total }}</span>
            </div>
            <div class="h-1.5 bg-white/10 rounded-full overflow-hidden mb-1">
              <div class="h-full bg-indigo-500 rounded-full transition-all"
                :style="{ width: (bulkProgress.total ? (bulkProgress.current / bulkProgress.total * 100) : 0) + '%' }" />
            </div>
            <div class="text-xs text-gray-500 truncate">{{ bulkProgress.clip }}</div>
          </div>

          <!-- Progress Dashboard -->
          <div v-if="selectedFolder && clips.length > 0"
            class="p-3 bg-dark-900/40 border border-white/10 rounded-xl mb-3">
            <div class="flex items-center justify-between text-xs mb-2">
              <span class="text-gray-400 font-medium">Upload-Status: {{ selectedFolder }}</span>
              <span :class="['font-bold', allPlatformsDone ? 'text-green-400' : 'text-orange-400']">
                {{ uploadedCount }} / {{ clips.length * 3 }} Uploads
              </span>
            </div>
            <div class="grid grid-cols-3 gap-2">
              <div class="flex items-center gap-2">
                <div class="flex-1 h-2 bg-white/10 rounded-full overflow-hidden">
                  <div class="h-full bg-red-500 transition-all"
                    :style="{ width: (clips.length ? (ytCount / clips.length * 100) : 0) + '%' }"></div>
                </div>
                <span class="text-xs text-gray-400 w-10 flex-shrink-0">YT {{ ytCount }}/{{ clips.length }}</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="flex-1 h-2 bg-white/10 rounded-full overflow-hidden">
                  <div class="h-full bg-gray-300 transition-all"
                    :style="{ width: (clips.length ? (ttCount / clips.length * 100) : 0) + '%' }"></div>
                </div>
                <span class="text-xs text-gray-400 w-10 flex-shrink-0">TT {{ ttCount }}/{{ clips.length }}</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="flex-1 h-2 bg-white/10 rounded-full overflow-hidden">
                  <div class="h-full bg-pink-500 transition-all"
                    :style="{ width: (clips.length ? (igCount / clips.length * 100) : 0) + '%' }"></div>
                </div>
                <span class="text-xs text-gray-400 w-10 flex-shrink-0">IG {{ igCount }}/{{ clips.length }}</span>
              </div>
            </div>
            <div v-if="!allPlatformsDone && clips.length > 0" class="text-xs text-gray-500 mt-2">
              Fehlende Uploads: {{ missingUploads.join(', ') || '—' }}
            </div>
          </div>

          <!-- Drop hint -->
          <div v-if="isDragging"
            class="flex items-center justify-center h-32 text-primary font-semibold text-lg">
            🎬 Hier loslassen
          </div>

          <!-- Empty states -->
          <div v-else-if="!selectedFolder"
            class="flex flex-col items-center justify-center h-40 text-gray-600 gap-2">
            <span class="text-3xl">📂</span>
            <span class="text-sm">Ordner auswählen oder erstellen</span>
          </div>

          <div v-else-if="clips.length === 0"
            class="flex flex-col items-center justify-center h-40 text-gray-600 gap-2">
            <span class="text-3xl">🎞️</span>
            <span class="text-sm">Clips hier reinziehen oder über "+ Clips hinzufügen"</span>
            <span class="text-xs text-gray-700">Auch Strg+V funktioniert</span>
          </div>

          <!-- Clip grid -->
          <div v-else class="grid grid-cols-3 gap-2 lg:grid-cols-4">
            <button
              v-for="clip in clips"
              :key="clip.rel_path"
              @click="selectClip(clip)"
              :class="['group relative p-2 rounded-xl border text-left transition-colors',
                selectedClip?.rel_path === clip.rel_path
                  ? 'bg-primary/15 border-primary/40'
                  : clip.is_done
                    ? 'bg-green-600/10 hover:bg-green-600/15 border-green-600/30'
                    : 'bg-white/4 hover:bg-white/8 border-white/8']">
              <div class="text-center text-2xl mb-1">{{ clip.done_yt && clip.done_tt && clip.done_ig ? '✅' : '▶️' }}</div>
              <div :class="['text-xs font-medium truncate leading-tight', clip.done_yt && clip.done_tt && clip.done_ig ? 'text-green-400/70' : 'text-gray-300']">{{ clip.name }}</div>
              <div class="text-xs text-gray-600 mt-0.5">{{ formatSize(clip.size_bytes) }}</div>
              <div v-if="clip.has_description" class="absolute top-1.5 right-1.5 text-xs">📝</div>
              <div v-if="clip.has_schedule" class="absolute top-1.5 left-1.5 text-xs">📅</div>
              <!-- Platform Badges -->
              <div class="absolute bottom-1.5 right-1.5 flex gap-1">
                <span @click.stop="togglePlatformDone(clip, 'yt', $event)"
                  :class="['text-xs w-5 h-5 flex items-center justify-center rounded cursor-pointer transition-colors',
                    clip.done_yt ? 'bg-red-600 text-white' : 'bg-white/10 text-gray-500 hover:bg-white/20']"
                  title="YouTube">Y</span>
                <span @click.stop="togglePlatformDone(clip, 'tt', $event)"
                  :class="['text-xs w-5 h-5 flex items-center justify-center rounded cursor-pointer transition-colors',
                    clip.done_tt ? 'bg-gray-200 text-black' : 'bg-white/10 text-gray-500 hover:bg-white/20']"
                  title="TikTok">T</span>
                <span @click.stop="togglePlatformDone(clip, 'ig', $event)"
                  :class="['text-xs w-5 h-5 flex items-center justify-center rounded cursor-pointer transition-colors',
                    clip.done_ig ? 'bg-pink-600 text-white' : 'bg-white/10 text-gray-500 hover:bg-white/20']"
                  title="Instagram">I</span>
              </div>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Server panel -->
    <div class="p-4 bg-dark-900/60 border border-white/10 rounded-2xl">
      <div class="flex items-center justify-between flex-wrap gap-3">
        <div class="flex items-center gap-3">
          <div :class="['w-2.5 h-2.5 rounded-full flex-shrink-0', serverRunning ? 'bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.6)]' : 'bg-gray-600']" />
          <div>
            <div class="text-sm font-semibold text-white">📱 Handy-Zugriff</div>
            <div v-if="serverRunning" class="text-xs text-primary mt-0.5">{{ serverUrl }}</div>
            <div v-else class="text-xs text-gray-500">Server gestoppt</div>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <button v-if="!serverRunning" @click="startServer"
            class="px-4 py-2 bg-green-600/20 hover:bg-green-600/30 border border-green-600/30 text-green-400 rounded-xl text-sm font-medium transition-colors">
            ▶ Server starten
          </button>
          <button v-else @click="stopServer"
            class="px-4 py-2 bg-red-600/20 hover:bg-red-600/30 border border-red-600/30 text-red-400 rounded-xl text-sm font-medium transition-colors">
            ■ Stop
          </button>
          <button v-if="serverRunning" @click="showQr = !showQr"
            class="px-3 py-2 bg-white/6 hover:bg-white/12 border border-white/10 text-gray-300 rounded-xl text-sm transition-colors">
            QR {{ showQr ? '▲' : '▼' }}
          </button>
        </div>
      </div>

      <!-- QR Code -->
      <div v-if="showQr && qrDataUrl" class="mt-4 flex justify-center">
        <img :src="qrDataUrl" alt="QR Code" class="rounded-xl" />
      </div>
    </div>

    <!-- Guide accordion -->
    <div class="border border-white/8 rounded-2xl overflow-hidden">
      <button @click="showGuide = !showGuide"
        class="w-full flex items-center justify-between px-4 py-3 bg-white/4 hover:bg-white/7 transition-colors text-left">
        <span class="text-sm font-semibold text-gray-300">📱 Anleitung: Clips auf dem Handy öffnen</span>
        <span class="text-gray-500 text-sm">{{ showGuide ? '▲' : '▼' }}</span>
      </button>
      <div v-if="showGuide" class="px-5 py-4 bg-dark-900/40 text-sm text-gray-400 space-y-3">
        <div class="space-y-2.5">
          <p class="flex gap-3"><span class="text-primary font-bold">1.</span><span><strong class="text-gray-300">Windows-Hotspot aktivieren</strong><br>Einstellungen → Netzwerk &amp; Internet → Mobile Hotspot → Einschalten</span></p>
          <p class="flex gap-3"><span class="text-primary font-bold">2.</span><span><strong class="text-gray-300">Server starten</strong><br>Oben auf „▶ Server starten" klicken — die URL wird angezeigt.</span></p>
          <p class="flex gap-3"><span class="text-primary font-bold">3.</span><span><strong class="text-gray-300">Handy verbinden</strong><br>Handy mit dem Windows-Hotspot verbinden (WLAN-Einstellungen am Handy).</span></p>
          <p class="flex gap-3"><span class="text-primary font-bold">4.</span><span><strong class="text-gray-300">Browser öffnen</strong><br>QR-Code scannen oder folgende URL eintippen:<br>
            <code class="text-primary bg-dark-950/60 px-2 py-0.5 rounded text-xs">http://{{ localIp || '192.168.137.1' }}:{{ serverPort }}/</code>
          </span></p>
          <p class="flex gap-3"><span class="text-primary font-bold">5.</span><span><strong class="text-gray-300">Clips herunterladen</strong><br>Ordner auswählen → Clip antippen → „⬇ Herunterladen" oder „⬇ Alle herunterladen".<br><span class="text-gray-600 text-xs">Downloads landen in der Downloads-App / Galerie.</span></span></p>
        </div>
      </div>
    </div>

    <!-- Toast notifications -->
    <div class="fixed bottom-6 left-1/2 -translate-x-1/2 flex flex-col gap-2 items-center z-50" style="min-width:320px;max-width:90vw">
      <TransitionGroup enter-active-class="transition ease-out duration-200" enter-from-class="opacity-0 translate-y-2" enter-to-class="opacity-100 translate-y-0" leave-active-class="transition ease-in duration-150" leave-from-class="opacity-100" leave-to-class="opacity-0">
        <div v-for="t in toasts" :key="t.id"
          :class="['flex items-start gap-3 px-4 py-3 rounded-2xl text-sm text-white shadow-2xl border',
            t.isError
              ? 'bg-red-950 border-red-700/60 text-red-200'
              : 'bg-dark-800 border-white/15']">
          <span class="flex-1">{{ t.msg }}</span>
          <button v-if="t.isError" @click="dismissToast(t.id)"
            class="text-red-400 hover:text-white flex-shrink-0 leading-none text-base font-bold">✕</button>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>
