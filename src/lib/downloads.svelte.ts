import { invoke } from '@tauri-apps/api/core';
import { goto } from '$app/navigation';
import { loadConfigNative } from '$lib/config';
import type { AppState, QueueItem } from '$lib/types';

// Shared Nexus download queue. The sidebar layout owns the event
// listeners (deep-link, download-progress/done/meta) and both the
// Mods page (update flow) and the Downloads page read/write here,
// so the queue survives route changes instead of living in Mods state.
export const queueState = $state<{ queue: QueueItem[]; lastError: string }>({
  queue: [],
  lastError: '',
});

export function setQueue(q: QueueItem[]) {
  queueState.queue = q;
}

export async function refreshQueue(): Promise<void> {
  try {
    queueState.queue = await invoke<QueueItem[]>('queue_list');
  } catch {
    /* backend not up yet — keep stale list */
  }
}

export async function downloadsDir(): Promise<string> {
  try {
    return await invoke<string>('downloads_dir_path');
  } catch {
    return '/tmp';
  }
}

export function humanSize(n: number | null | undefined): string {
  if (n === null || n === undefined) return '?';
  let v = n;
  for (const unit of ['B', 'KB', 'MB', 'GB'] as const) {
    if (v < 1024 || unit === 'GB')
      return unit === 'B' || unit === 'KB'
        ? `${Math.round(v)} ${unit}`
        : `${(Math.round(v * 10) / 10).toFixed(1)} ${unit}`;
    v /= 1024;
  }
  return `${v} B`;
}

export function cleanVer(v: string): string {
  return (v ?? '')
    .replace(/^(?:version|ver\.?|v)\s*\.?\s*(?=\d)/i, '')
    .trim();
}

/** Display name for the Install window (original _list_name, simplified). */
export function dlListName(qq: QueueItem): string {
  const modName = (qq.mod_name || '').trim();
  if ((qq.category || '').toUpperCase() === 'MAIN' && modName) return modName;
  const title = (qq.file_title || '').trim();
  if (!title) return modName || qq.filename;
  if (!modName) return title;
  const key = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '');
  if (key(modName) && key(title).includes(key(modName))) return title;
  const base = modName.split(/\s+[-\u2013\u2014:|]\s+/)[0].trim();
  if (base && key(base).length >= 4 && key(title).includes(key(base))) return title;
  return `${base || modName} - ${title}`;
}

/** File identity with an installed mod: same archive name means installed. */
export function dlSameFile(mods: AppState['mods'] | undefined, qq: QueueItem): boolean {
  if (!mods || qq.status !== 'done' || !qq.filename) return false;
  const fname = qq.filename.toLowerCase();
  return mods.some(
    (m) =>
      !m.sep && m.archive && m.archive.split('/').pop()?.toLowerCase() === fname,
  );
}

function verTuple(v: string): number[] {
  const m = cleanVer(v).match(/\d+(?:\.\d+)*/);
  if (!m) return [];
  return m[0].split('.').map((x) => parseInt(x, 10) || 0);
}

function verVerdict(nw: string, old: string): string {
  const a = (nw ?? '').trim(),
    b = (old ?? '').trim();
  if (!a || !b) return '';
  if (a.toLowerCase() === b.toLowerCase()) return 'same';
  const ta = verTuple(a),
    tb = verTuple(b);
  if (ta.length && tb.length) {
    for (let i = 0; i < Math.max(ta.length, tb.length); i++) {
      const x = ta[i] ?? 0,
        y = tb[i] ?? 0;
      if (x !== y) return x > y ? 'newer' : 'older';
    }
    return 'same';
  }
  return '';
}

/** What the main button says for a finished row, like DownloadsPanel.match. */
export function dlAction(mods: AppState['mods'] | undefined, qq: QueueItem): string {
  if (!mods) return 'Install';
  const nid = qq.mod_id || '';
  const fname = (qq.filename || '').toLowerCase();
  let hit = mods.find(
    (m) => !m.sep && m.archive && m.archive.split('/').pop()?.toLowerCase() === fname,
  );
  if (!hit && nid) {
    const samePage = mods.filter((m) => !m.sep && (m.nexus || '') === nid);
    if (samePage.length === 1) hit = samePage[0];
  }
  if (!hit) return 'Install';
  const v = verVerdict(qq.version, hit.version);
  if (v === 'newer') return 'Update';
  if (v === 'same') return 'Reinstall';
  if (v === 'older') return 'Downgrade';
  const sameName =
    !!fname &&
    !!mods.some(
      (m) => !m.sep && m.archive && m.archive.split('/').pop()?.toLowerCase() === fname,
    );
  return sameName ? 'Reinstall' : 'Replace';
}

export function dlMetaLine(qq: QueueItem): string {
  const bits: string[] = [];
  const v = cleanVer(qq.version);
  if (v) bits.push(v[0] && /\d/.test(v[0]) ? `v${v}` : v);
  if (qq.category && qq.category.toUpperCase() !== 'MAIN') {
    const c = qq.category.toLowerCase();
    bits.push(c[0].toUpperCase() + c.slice(1));
  }
  return bits.join('  ·  ');
}

export function dlStatus(
  mods: AppState['mods'] | undefined,
  qq: QueueItem,
): { text: string; color: string } {
  if (qq.status === 'done')
    return {
      text: `${dlSameFile(mods, qq) ? 'Installed' : 'Downloaded'}  ·  ${humanSize(qq.total || qq.done)}`,
      color: '#7fbf8a',
    };
  if (qq.status === 'error' || qq.status === 'failed')
    return { text: qq.error || 'Download failed', color: '#e3735f' };
  if (qq.status === 'cancelled') return { text: 'Cancelled', color: '#8c96a1' };
  if (qq.status === 'paused')
    return {
      text: `Paused  ·  ${humanSize(qq.done)} of ${humanSize(qq.total)}`,
      color: '#8c96a1',
    };
  if (qq.status === 'queued') return { text: 'Queued…', color: '#8c96a1' };
  if (qq.status === 'starting') return { text: 'Asking Nexus…', color: '#8c96a1' };
  const tot = qq.total ? ` of ${humanSize(qq.total)}` : '';
  const spd = qq.speed ? `  ·  ${humanSize(qq.speed)}/s` : '';
  return { text: `${humanSize(qq.done)}${tot}${spd}`, color: '#8c96a1' };
}

/** Open the Install tool window for a finished row (original offer_install). */
export async function offerInstall(qq: QueueItem, destOverride?: string): Promise<void> {
  const dest = destOverride ?? ((await downloadsDir()) + '/' + qq.filename);
  const qp = new URLSearchParams({
    path: dest,
    name: dlListName(qq),
    version: qq.version ?? '',
    nexus: (qq.mod_id ?? '').replace(/\D/g, ''),
  });
  await invoke('open_tool_window', {
    kind: 'install',
    query: qp.toString(),
    path: dest,
  });
}

export async function openDownloadsFolder(): Promise<void> {
  const d = await downloadsDir();
  await invoke('open_path', { target: d });
}

/** Enqueue an nxm:// URL and start it; returns the queue id. */
export async function dlNxm(url: string): Promise<string> {
  if (!url) return '';
  const cfg = await loadConfigNative();
  if (!cfg.nexusKey) throw new Error('Set Nexus API key in Settings first');
  const id = await invoke<string>('queue_enqueue', { url });
  await refreshQueue();
  const row = queueState.queue.find((qq) => qq.id === id);
  if (row && (row.status === 'active' || row.status === 'starting' || row.status === 'paused')) {
    return id; // already fetching this file
  }
  if (row && row.status === 'done') return id; // caller decides install
  await invoke('queue_start', { id, destDir: await downloadsDir(), apiKey: cfg.nexusKey });
  await refreshQueue();
  // A download just started — take the user to the Downloads page.
  goto('/downloads').catch(() => {});
  return id;
}

export async function dlRemove(qq: QueueItem): Promise<void> {
  await invoke('queue_remove', { id: qq.id });
  await refreshQueue();
}

export async function dlTrash(qq: QueueItem): Promise<void> {
  await invoke('queue_trash', { id: qq.id, destDir: await downloadsDir() });
  await refreshQueue();
}

export async function dlMain(mods: AppState['mods'] | undefined, qq: QueueItem): Promise<void> {
  const running = qq.status === 'active' || qq.status === 'starting' || qq.status === 'queued';
  if (running) {
    await invoke('queue_cancel', { id: qq.id });
    await refreshQueue();
    return;
  }
  if (qq.status === 'error' || qq.status === 'failed' || qq.status === 'paused' || qq.status === 'cancelled') {
    await invoke('queue_start', {
      id: qq.id,
      destDir: await downloadsDir(),
      apiKey: (await loadConfigNative()).nexusKey,
    });
    await refreshQueue();
    // Retry started a download — take the user to the Downloads page.
    goto('/downloads').catch(() => {});
    return;
  }
  if (qq.status === 'done') {
    // Genuinely different file (not the installed copy) → install window.
    if (!dlSameFile(mods, qq)) await offerInstall(qq);
  }
}
