<script setup lang="ts">
import { ref, onMounted } from 'vue';
import InfoTip from '../components/InfoTip.vue';
import { useStoreField } from '../store';

const geminiKey = useStoreField<string>('ai.gemini_key', '');
const groqKey = useStoreField<string>('ai.groq_key', '');
const aiDefaultProvider = useStoreField<string>('ai.default_provider', 'gemini');

// Platform Scheduling
const schedulerUrl    = useStoreField<string>('scheduler.server_url', '');
const schedulerUploadUrl = useStoreField<string>('scheduler.upload_url', '');
const schedulerApiKey = useStoreField<string>('scheduler.api_key', '');

const ptBoardId = useStoreField<string>('platform.pt.board_id', '');

// ─── Snapchat Session ─────────────────────────────────────────────────────────
const scCookieJson  = ref('');
const scImporting   = ref(false);
const scImportMsg   = ref('');
const scSessionStatus = ref<'valid' | 'expired' | 'none'>('none');
const scExpiresAt   = ref<number | null>(null);
const scHasScAt     = ref(false);

async function loadScSessionStatus() {
  const base = schedulerUrl.value.replace(/\/$/, '');
  const key  = schedulerApiKey.value;
  if (!base || !key) return;
  try {
    const res = await fetch(`${base}/api/snapchat/session-status`, { headers: { 'x-api-key': key } });
    if (!res.ok) return;
    const data = await res.json();
    if (!data.exists) { scSessionStatus.value = 'none'; scExpiresAt.value = null; scHasScAt.value = false; return; }
    scExpiresAt.value   = data.expires_at;
    scHasScAt.value     = data.has_sc_at ?? false;
    scSessionStatus.value = data.valid ? 'valid' : 'expired';
  } catch { /* server offline */ }
}

async function importScCookies() {
  const base = schedulerUrl.value.replace(/\/$/, '');
  const key  = schedulerApiKey.value;
  if (!base || !key) { scImportMsg.value = 'Server-URL oder API-Key fehlt'; return; }
  let parsed: unknown;
  try { parsed = JSON.parse(scCookieJson.value); } catch { scImportMsg.value = '❌ Ungültiges JSON'; return; }
  scImporting.value = true;
  scImportMsg.value = '';
  try {
    const res = await fetch(`${base}/api/snapchat/import-cookies`, {
      method: 'POST',
      headers: { 'x-api-key': key, 'Content-Type': 'application/json' },
      body: JSON.stringify(parsed),
    });
    const data = await res.json();
    if (res.ok) {
      scImportMsg.value = `✓ ${data.cookies_imported} Cookies importiert`;
      scCookieJson.value = '';
      await loadScSessionStatus();
    } else {
      scImportMsg.value = `❌ ${data.error}`;
    }
  } catch (e) { scImportMsg.value = '❌ ' + String(e); }
  finally { scImporting.value = false; }
}

interface TokenStatus { connected: boolean; expires_at: number | null; }
const tokenStatus = ref<Record<string, TokenStatus>>({
  youtube:   { connected: false, expires_at: null },
  tiktok:    { connected: false, expires_at: null },
  instagram: { connected: false, expires_at: null },
  pinterest: { connected: false, expires_at: null },
});

const tiktokUser = ref<{ display_name?: string; avatar_url?: string } | null>(null);
const tiktokUserLoading = ref(false);

async function loadTokenStatus() {
  const base = schedulerUrl.value.replace(/\/$/, '');
  if (!base) return;
  for (const p of ['youtube', 'tiktok', 'instagram', 'pinterest']) {
    try {
      const res = await fetch(`${base}/api/auth-status/${p}`);
      if (res.ok) tokenStatus.value[p] = await res.json();
    } catch { /* server offline */ }
  }
  if (tokenStatus.value.tiktok.connected) loadTiktokUser();
}

// Lädt das TikTok-Profil — demonstriert den user.info.basic Scope sichtbar
async function loadTiktokUser() {
  const base = schedulerUrl.value.replace(/\/$/, '');
  if (!base) return;
  tiktokUserLoading.value = true;
  try {
    const res = await fetch(`${base}/api/tiktok/userinfo`);
    if (res.ok) {
      const data = await res.json();
      tiktokUser.value = data.connected ? data.user : null;
    }
  } catch { tiktokUser.value = null; }
  finally { tiktokUserLoading.value = false; }
}

function tokenLabel(p: string): string {
  const s = tokenStatus.value[p];
  if (!s.connected) return '';
  if (!s.expires_at) return '✅ Verbunden';
  const days = Math.floor((s.expires_at - Date.now() / 1000) / 86400);
  // Tokens < 2 days are short-lived access tokens that auto-refresh — don't warn
  if (days < 2)  return '✅ Verbunden (auto-refresh)';
  if (days < 60) return `⚠️ Läuft in ${days} Tagen ab`;
  return `✅ Verbunden (noch ${days} Tage)`;
}

function tokenClass(p: string): string {
  const s = tokenStatus.value[p];
  if (!s.connected) return 'text-gray-500';
  if (!s.expires_at) return 'text-green-400';
  const days = Math.floor((s.expires_at - Date.now() / 1000) / 86400);
  if (days < 2)  return 'text-green-400';
  if (days < 60) return 'text-orange-400 font-semibold';
  return 'text-green-400';
}

async function openOAuth(platform: 'youtube' | 'tiktok' | 'instagram' | 'pinterest') {
  const base = schedulerUrl.value.replace(/\/$/, '');
  if (!base) { alert('Bitte zuerst VPS-Server-URL eintragen.'); return; }
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(`${base}/auth/${platform}/start`);
  // Refresh status after a short delay
  setTimeout(loadTokenStatus, 5000);
}

onMounted(() => { loadTokenStatus(); loadScSessionStatus(); });
</script>

<template>
  <div class="space-y-6">
    <div class="flex flex-col gap-2">
      <h1 class="text-3xl font-bold text-white tracking-tight">Einstellungen</h1>
      <p class="text-gray-400">Server-Verbindung, KI-Keys und Plattform-Logins.</p>
    </div>

    <div class="grid gap-6 lg:grid-cols-2">
      <!-- AI Hashtag Generator -->
      <div class="card space-y-6">
        <h2 class="text-xl font-semibold text-white border-b border-white/5 pb-4">KI-Hashtag-Generator</h2>

        <div class="space-y-4">
          <p class="text-sm text-gray-400">
            Generiert Caption + Hashtags pro Clip direkt aus dem Clips-Tab. Beide Anbieter haben
            einen kostenlosen Tier — trage mindestens einen Key ein.
          </p>

          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <label class="text-sm font-medium text-gray-300">Google Gemini API-Key</label>
              <InfoTip text="Gemini 2.5 Flash analysiert das Video direkt. Kostenloser Key auf aistudio.google.com (15 Req/min, 1500/Tag)."/>
            </div>
            <input type="password" class="input w-full font-mono text-sm" v-model="geminiKey" placeholder="AIza…" autocomplete="off" />
            <p class="text-xs text-gray-500">
              Kostenlosen Key holen: <a href="https://aistudio.google.com/apikey" target="_blank" class="text-primary hover:underline">aistudio.google.com/apikey</a>
            </p>
          </div>

          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <label class="text-sm font-medium text-gray-300">Groq API-Key</label>
              <InfoTip text="Groq (Llama 3.3 70B) ist sehr schnell, sieht aber kein Video — nutzt nur Dateiname + vorhandene Description."/>
            </div>
            <input type="password" class="input w-full font-mono text-sm" v-model="groqKey" placeholder="gsk_…" autocomplete="off" />
            <p class="text-xs text-gray-500">
              Kostenlosen Key holen: <a href="https://console.groq.com/keys" target="_blank" class="text-primary hover:underline">console.groq.com/keys</a>
            </p>
          </div>

          <div class="space-y-2">
            <label class="text-sm font-medium text-gray-300">Standard-Anbieter</label>
            <div class="flex gap-4 text-sm">
              <label class="flex items-center gap-2 cursor-pointer">
                <input type="radio" value="gemini" v-model="aiDefaultProvider" />
                <span>Gemini (Video-aware, empfohlen)</span>
              </label>
              <label class="flex items-center gap-2 cursor-pointer">
                <input type="radio" value="groq" v-model="aiDefaultProvider" />
                <span>Groq (nur Text)</span>
              </label>
            </div>
          </div>
        </div>
      </div>

      <!-- Platform Scheduling -->
      <div class="card space-y-6 lg:col-span-2">
        <h2 class="text-xl font-semibold text-white border-b border-white/5 pb-4">🗓 Plattform-Scheduling (VPS)</h2>
        <p class="text-sm text-gray-400">
          Verbinde den Scheduler-Server (Docker auf VPS) und trage deine OAuth-Credentials ein.
          Klicke dann „Verbinden" für jede Plattform — der Browser öffnet sich zur Autorisierung.
        </p>

        <!-- Server connection -->
        <div class="grid gap-4 md:grid-cols-2">
          <div class="space-y-2">
            <label class="text-sm font-medium text-gray-300">VPS Server-URL</label>
            <InfoTip text="Für Status-Seite, OAuth-Callbacks und Token-Checks. Z.B. https://your-server.com"/>
            <input class="input w-full font-mono text-sm" v-model="schedulerUrl" placeholder="https://your-server.com" />
          </div>
          <div class="space-y-2">
            <label class="text-sm font-medium text-gray-300">Upload-URL <span class="text-gray-600 text-xs">(optional)</span></label>
            <InfoTip text="Separate URL für Video-Uploads — nutze direkte IP um Cloudflare-Limits zu umgehen. Leer lassen = Server-URL wird verwendet."/>
            <input class="input w-full font-mono text-sm" v-model="schedulerUploadUrl" placeholder="https://your-server.com" />
          </div>
          <div class="space-y-2">
            <label class="text-sm font-medium text-gray-300">Shared API-Key</label>
            <InfoTip text="Gemeinsamer geheimer Schlüssel zwischen Desktop und VPS-Server."/>
            <input type="password" class="input w-full font-mono text-sm" v-model="schedulerApiKey" placeholder="sk-…" autocomplete="off" />
          </div>
        </div>

        <!-- YouTube -->
        <div class="p-4 bg-red-900/10 border border-red-700/20 rounded-xl flex items-center justify-between">
          <div>
            <h3 class="text-sm font-semibold text-red-300">▶ YouTube Data API v3</h3>
            <p v-if="tokenStatus.youtube.connected" :class="['text-xs mt-1', tokenClass('youtube')]">{{ tokenLabel('youtube') }}</p>
            <p v-else class="text-xs text-gray-600 mt-1">Nicht verbunden</p>
          </div>
          <button @click="openOAuth('youtube')" :disabled="!schedulerUrl"
            class="px-3 py-1.5 bg-red-700/30 hover:bg-red-700/50 border border-red-600/40 text-red-300 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
            🔗 {{ tokenStatus.youtube.connected ? 'Neu verbinden' : 'YouTube verbinden' }}
          </button>
        </div>

        <!-- TikTok -->
        <div class="p-4 bg-neutral-800/30 border border-white/10 rounded-xl flex items-center justify-between">
          <div class="flex items-center gap-3">
            <img v-if="tiktokUser?.avatar_url" :src="tiktokUser.avatar_url"
              class="w-9 h-9 rounded-full border border-white/20" alt="TikTok Avatar" />
            <div>
              <h3 class="text-sm font-semibold text-gray-200">♪ TikTok Content Posting API</h3>
              <p v-if="tiktokUser?.display_name" class="text-xs mt-1 text-green-400">
                ✅ Verbunden als <span class="font-semibold">@{{ tiktokUser.display_name }}</span>
              </p>
              <p v-else-if="tiktokUserLoading" class="text-xs mt-1 text-gray-500">Lade Profil…</p>
              <p v-else-if="tokenStatus.tiktok.connected" :class="['text-xs mt-1', tokenClass('tiktok')]">{{ tokenLabel('tiktok') }}</p>
              <p v-else class="text-xs text-gray-600 mt-1">Nicht verbunden</p>
            </div>
          </div>
          <button @click="openOAuth('tiktok')" :disabled="!schedulerUrl"
            class="px-3 py-1.5 bg-white/10 hover:bg-white/20 border border-white/15 text-gray-200 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
            🔗 {{ tokenStatus.tiktok.connected ? 'Neu verbinden' : 'TikTok verbinden' }}
          </button>
        </div>

        <!-- Instagram -->
        <div class="p-4 bg-pink-900/10 border border-pink-700/20 rounded-xl flex items-center justify-between">
          <div>
            <h3 class="text-sm font-semibold text-pink-300">◎ Instagram Graph API (Reels)</h3>
            <p v-if="tokenStatus.instagram.connected" :class="['text-xs mt-1', tokenClass('instagram')]">{{ tokenLabel('instagram') }}</p>
            <p v-else class="text-xs text-gray-600 mt-1">Nicht verbunden</p>
          </div>
          <button @click="openOAuth('instagram')" :disabled="!schedulerUrl"
            class="px-3 py-1.5 bg-pink-700/30 hover:bg-pink-700/50 border border-pink-600/40 text-pink-300 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
            🔗 {{ tokenStatus.instagram.connected ? 'Neu verbinden' : 'Instagram verbinden' }}
          </button>
        </div>

        <!-- Pinterest -->
        <div class="p-4 bg-red-900/10 border border-red-800/20 rounded-xl space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-semibold text-red-300">📌 Pinterest API v5 (Video Pins)</h3>
              <p v-if="tokenStatus.pinterest.connected" :class="['text-xs mt-1', tokenClass('pinterest')]">{{ tokenLabel('pinterest') }}</p>
              <p v-else class="text-xs text-gray-600 mt-1">Nicht verbunden</p>
            </div>
            <button @click="openOAuth('pinterest')" :disabled="!schedulerUrl"
              class="px-3 py-1.5 bg-red-800/30 hover:bg-red-800/50 border border-red-700/40 text-red-300 rounded-lg text-xs transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
              🔗 {{ tokenStatus.pinterest.connected ? 'Neu verbinden' : 'Pinterest verbinden' }}
            </button>
          </div>
          <div class="space-y-2">
            <label class="text-xs font-medium text-gray-400">Board-ID</label>
            <InfoTip text="Die ID des Pinterest-Boards, in das Video-Pins erstellt werden. Zu finden in der Board-URL: pinterest.com/user/board-name/"/>
            <input class="input w-full font-mono text-sm" v-model="ptBoardId" placeholder="123456789012345678" />
            <p class="text-xs text-gray-600">Board-ID aus der URL deines Pinterest-Boards kopieren.</p>
          </div>
        </div>

        <!-- Snapchat -->
        <div class="p-4 bg-yellow-900/10 border border-yellow-700/20 rounded-xl space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-semibold text-yellow-300">👻 Snapchat Spotlight <span class="text-xs font-normal text-yellow-700 ml-1">(Experimentell)</span></h3>
              <p class="text-xs text-yellow-700 mt-0.5">Session-Cookies aus Cookie-Editor importieren</p>
            </div>
            <div :class="['text-xs px-2 py-1 rounded-lg font-medium', scSessionStatus === 'valid' ? 'bg-green-900/40 text-green-400' : scSessionStatus === 'expired' ? 'bg-red-900/40 text-red-400' : 'bg-white/5 text-gray-500']">
              {{ scSessionStatus === 'valid' ? '✓ Session aktiv' : scSessionStatus === 'expired' ? '⚠ Abgelaufen' : '— Keine Session' }}
            </div>
          </div>
          <div v-if="scExpiresAt" class="text-xs text-yellow-700">
            Läuft ab: {{ new Date(scExpiresAt * 1000).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }) }}
          </div>
          <div v-if="scSessionStatus !== 'none' && !scHasScAt" class="text-xs text-red-400">
            ⚠ sc_at Cookie fehlt — du warst beim Export nicht eingeloggt. Logge dich erst auf profile.snapchat.com ein, dann exportieren.
          </div>
          <div class="space-y-2">
            <label class="text-xs font-medium text-gray-400">Cookie-Editor JSON einfügen</label>
            <textarea v-model="scCookieJson" rows="4" placeholder='[{"name":"sc_at","value":"...","domain":".snapchat.com",...}]'
              class="w-full bg-dark-950/80 border border-white/10 rounded-xl px-3 py-2 text-xs text-gray-300 placeholder-gray-700 outline-none focus:border-yellow-500/40 font-mono resize-none" />
            <div class="flex gap-2">
              <button @click="importScCookies" :disabled="!scCookieJson.trim() || scImporting"
                class="px-3 py-1.5 bg-yellow-700/30 hover:bg-yellow-700/50 border border-yellow-600/30 text-yellow-300 rounded-lg text-xs font-medium transition-colors disabled:opacity-40">
                {{ scImporting ? 'Importiere…' : '📥 Session importieren' }}
              </button>
              <button @click="loadScSessionStatus"
                class="px-3 py-1.5 bg-white/5 hover:bg-white/10 border border-white/10 text-gray-400 rounded-lg text-xs transition-colors">
                ↻ Status prüfen
              </button>
            </div>
            <p v-if="scImportMsg" :class="['text-xs', scImportMsg.startsWith('✓') ? 'text-green-400' : 'text-red-400']">{{ scImportMsg }}</p>
          </div>
          <div class="bg-dark-900/50 rounded-lg p-3 text-xs text-yellow-800/70 border border-yellow-700/10">
            Cookie-Editor Chrome Extension → auf profile.snapchat.com einloggen → Export → JSON hier einfügen
          </div>
        </div>
      </div>

      <!-- Info Panel -->
      <div class="space-y-6">
        <div class="card bg-gradient-to-br from-blue-900/20 to-transparent border-blue-500/20">
          <div class="flex items-start gap-4">
            <div class="p-3 rounded-xl bg-blue-500/20 text-blue-400">
              <span class="text-2xl">🌐</span>
            </div>
            <div>
              <h3 class="text-lg font-semibold text-blue-100 mb-2">FTP-Konfiguration</h3>
              <p class="text-sm text-blue-200/70 mb-4">
                Der Szenen-Generator nutzt eine vordefinierte FTP-Verbindung für automatische Uploads.
              </p>
              
              <div class="bg-dark-950/50 rounded-lg p-4 space-y-2 font-mono text-xs text-blue-200/60 border border-blue-500/10">
                <div class="flex justify-between"><span>Host:</span> <span class="text-blue-100">w0175180.kasserver.com</span></div>
                <div class="flex justify-between"><span>Port:</span> <span class="text-blue-100">21</span></div>
                <div class="flex justify-between"><span>User:</span> <span class="text-blue-100">***</span></div>
                <div class="flex justify-between"><span>Path:</span> <span class="text-blue-100">/uploads/</span></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

