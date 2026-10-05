<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { AppState, MadeFor, ModRow, QueueItem } from "$lib/types";
  import { loadConfigNative } from "$lib/config";
  import DataList, {
    type ClickModifiers,
  } from "$lib/components/data-list.svelte";
  import Button from "$lib/components/button.svelte";
  import SelectionToolbar from "$lib/components/selection-toolbar.svelte";
  import {
    ArrowUp,
    AtSign,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Copy,
    EqualNot,
    ExternalLink,
    Flag,
    FolderOpen,
    GitMerge,
    List,
    Pencil,
    RefreshCw,
    Trash2,
    TriangleAlert,
    X,
  } from "lucide-svelte";

  let appState: AppState | null = $state(null);
  let clashMap: Record<string, string[]> = $state({});
  let annotMap: Record<string, string[]> = $state({});
  let madeMap: Record<string, MadeFor> = $state({});
  let infoMap: Record<
    string,
    {
      scripts: { file: string; with: string[] }[];
      xmls: { file: string; with: string[] }[];
      lost: number;
    }
  > = $state({});
  let unmanaged: string[] = $state([]);
  let hits: { id: string; name: string; local: string; remote: string }[] =
    $state([]);

  let error: string = $state("");
  let status: string = $state("");
  let busy: string = $state("");
  let checkingIds: Set<string> = $state(new Set());
  let updatingIds: Set<string> = $state(new Set());
  let premiumCache: boolean | null = $state(null);
  let updatesOpen: boolean = $state(false);
  let updatesDismissed: boolean = $state(false);
  let updatesBanner: HTMLDivElement | null = $state(null);
  let gameDir: string = $state("");
  let prefix: string = $state("");
  let nexusKey: string = $state("");
  let filter: string = $state("");
  let menuOpen: boolean = $state(false);
  let collapsed: Record<string, boolean> = $state({});
  let selectedIds: Set<string> = $state(new Set());
  let lastSelected: string | null = $state(null);
  let ctx: { id: string; x: number; y: number } | null = $state(null);
  let sepCtx: { id: string; x: number; y: number } | null = $state(null);
  let hoverTip: { id: string; x: number; y: number } | null = $state(null);

  // downloads panel
  let dlOpen: boolean = $state(false);
  let queue: QueueItem[] = $state([]);
  let quotaText: string = $state("");
  let quotaTip: string = $state("");
  let nxm: string = $state("");
  let archPath: string = $state("");
  let dlCollapsed: Record<string, boolean> = $state({});

  const filtering = $derived(filter.trim().length > 0);
  const q = $derived(filter.trim().toLowerCase());
  const groupedQueue = $derived(
    queue.reduce(
      (acc, qq) => {
        const key = qq.mod_id || qq.file_id || qq.filename;
        if (!acc[key])
          acc[key] = {
            key,
            mod_id: qq.mod_id,
            mod_name: qq.mod_name || qq.filename,
            rows: [],
          };
        acc[key].rows.push(qq);
        if (!acc[key].mod_name && qq.mod_name) acc[key].mod_name = qq.mod_name;
        return acc;
      },
      {} as Record<
        string,
        { key: string; mod_id: string; mod_name: string; rows: QueueItem[] }
      >,
    ),
  );

  function humanSize(n: number | null | undefined): string {
    if (n === null || n === undefined) return "?";
    let v = n;
    for (const unit of ["B", "KB", "MB", "GB"] as const) {
      if (v < 1024 || unit === "GB")
        return unit === "B" || unit === "KB"
          ? `${Math.round(v)} ${unit}`
          : `${(Math.round(v * 10) / 10).toFixed(1)} ${unit}`;
      v /= 1024;
    }
    return `${v} B`;
  }

  const dlSummary = $derived(
    queue.length
      ? (() => {
          const bytes = queue.reduce(
            (a, qq) => a + (qq.total || qq.done || 0),
            0,
          );
          const n = Object.keys(groupedQueue).length;
          return `${n} mod${n === 1 ? "" : "s"}  ·  ${humanSize(bytes)}`;
        })()
      : "",
  );

  function cleanVer(v: string): string {
    return (v ?? "")
      .replace(/^(?:version|ver\.?|v)\s*\.?\s*(?=\d)/i, "")
      .trim();
  }
  function verTuple(v: string): number[] {
    const m = cleanVer(v).match(/\d+(?:\.\d+)*/);
    if (!m) return [];
    return m[0].split(".").map((x) => parseInt(x, 10) || 0);
  }
  function verVerdict(nw: string, old: string): string {
    const a = (nw ?? "").trim(),
      b = (old ?? "").trim();
    if (!a || !b) return "";
    if (a.toLowerCase() === b.toLowerCase()) return "same";
    const ta = verTuple(a),
      tb = verTuple(b);
    if (ta.length && tb.length) {
      const n = Math.max(ta.length, tb.length);
      for (let i = 0; i < n; i++) {
        const x = ta[i] ?? 0,
          y = tb[i] ?? 0;
        if (x !== y) return x > y ? "newer" : "older";
      }
      return "same";
    }
    return "";
  }
  /** What the main button says for a finished row, like DownloadsPanel.match. */
  function dlAction(qq: QueueItem): string {
    if (!appState) return "Install";
    const nid = qq.mod_id || "";
    const fname = (qq.filename || "").toLowerCase();
    let hit = appState.mods.find(
      (m) =>
        !m.sep &&
        m.archive &&
        m.archive.split("/").pop()?.toLowerCase() === fname,
    );
    if (!hit && nid) {
      const samePage = appState.mods.filter(
        (m) => !m.sep && (m.nexus || "") === nid,
      );
      if (samePage.length === 1) hit = samePage[0];
    }
    if (!hit) return "Install";
    const v = verVerdict(qq.version, hit.version);
    if (v === "newer") return "Update";
    if (v === "same") return "Reinstall";
    if (v === "older") return "Downgrade";
    // Unknown verdict: same archive name → harmless re-download of the same
    // file, anything else → a different file worth deciding about.
    const sameName =
      !!fname &&
      !!appState?.mods.some(
        (m) =>
          !m.sep &&
          m.archive &&
          m.archive.split("/").pop()?.toLowerCase() === fname,
      );
    return sameName ? "Reinstall" : "Replace";
  }
  /** File identity with an installed mod (original already_installed: same
  archive name). Only this suppresses the auto Install window and earns the
  tick — version verdicts alone must not, or genuinely different files (e.g.
  unknown versions) would never offer install. */
  function hitFor(id: string) {
    return hits.find((hh) => hh.id === id);
  }

  function dlSameFile(qq: QueueItem): boolean {    if (!appState || qq.status !== "done" || !qq.filename) return false;
    const fname = qq.filename.toLowerCase();
    return appState.mods.some(
      (m) =>
        !m.sep &&
        m.archive &&
        m.archive.split("/").pop()?.toLowerCase() === fname,
    );
  }
  function dlMetaLine(qq: QueueItem): string {
    const bits: string[] = [];
    const v = cleanVer(qq.version);
    if (v) bits.push(v[0] && /\d/.test(v[0]) ? `v${v}` : v);
    if (qq.category && qq.category.toUpperCase() !== "MAIN") {
      const c = qq.category.toLowerCase();
      bits.push(c[0].toUpperCase() + c.slice(1));
    }
    return bits.join("  ·  ");
  }
  function dlStatus(qq: QueueItem): { text: string; color: string } {
    if (qq.status === "done")
      return {
        text: `${dlSameFile(qq) ? "Installed" : "Downloaded"}  ·  ${humanSize(qq.total || qq.done)}`,
        color: "#7fbf8a",
      };
    if (qq.status === "error" || qq.status === "failed")
      return { text: qq.error || "Download failed", color: "#e3735f" };
    if (qq.status === "cancelled")
      return { text: "Cancelled", color: "#8c96a1" };
    if (qq.status === "paused")
      return {
        text: `Paused  ·  ${humanSize(qq.done)} of ${humanSize(qq.total)}`,
        color: "#8c96a1",
      };
    if (qq.status === "queued") return { text: "Queued…", color: "#8c96a1" };
    if (qq.status === "starting")
      return { text: "Asking Nexus…", color: "#8c96a1" };
    // active
    const tot = qq.total ? ` of ${humanSize(qq.total)}` : "";
    const spd = qq.speed ? `  ·  ${humanSize(qq.speed)}/s` : "";
    return { text: `${humanSize(qq.done)}${tot}${spd}`, color: "#8c96a1" };
  }

  function modById(id: string) {
    return appState?.mods.find((m) => m.id === id);
  }

  function clashCount(id: string): number {
    return Object.values(clashMap).filter(
      (ids) => ids.includes(id) && ids.length > 1,
    ).length;
  }
  function annotCount(name: string): number {
    return Object.values(annotMap).filter((mods) => mods.includes(name)).length;
  }
  function sharedScripts(id: string) {
    return infoMap[id]?.scripts ?? [];
  }
  function sharedXmls(id: string) {
    return infoMap[id]?.xmls ?? [];
  }
  function lostCount(id: string) {
    return infoMap[id]?.lost ?? 0;
  }
  function otherClashes(id: string): string[] {
    const mine = new Set<string>();
    for (const [path, ids] of Object.entries(clashMap)) {
      if (!ids.includes(id) || ids.length < 2) continue;
      const low = path.toLowerCase();
      if (low.endsWith(".ws") || low.endsWith(".wss")) continue;
      if (low.endsWith(".xml") && low.includes("bin/")) continue;
      mine.add(path);
    }
    return [...mine];
  }

  function prioOf(id: string): number {
    if (!appState) return 0;
    const i = appState.priority.indexOf(id);
    return i < 0 ? 0 : i + 1;
  }

  function fmtDate(ts: number): string {
    if (!ts) return "";
    try {
      return new Date(ts * 1000).toLocaleDateString("en-GB", {
        day: "2-digit",
        month: "short",
        year: "numeric",
      });
    } catch {
      return "";
    }
  }

  function enabledCounts(): [number, number] {
    if (!appState) return [0, 0];
    const mods = appState.mods.filter((m) => !m.sep);
    return [mods.filter((m) => m.enabled).length, mods.length];
  }

  async function boot() {
    try {
      const cfg = await loadConfigNative();
      gameDir = cfg.gameDir;
      prefix = cfg.prefix;
      nexusKey = cfg.nexusKey;
      if (!gameDir) {
        const found = await invoke<string | null>("detect_game").catch(
          () => null,
        );
        if (found) {
          gameDir = found;
          const pfx = await invoke<string | null>("default_prefix", {
            gameDir,
          }).catch(() => null);
          if (pfx) prefix = pfx;
        }
      }
      if (gameDir) await open();
      else status = "Set the game folder in Settings…";
    } catch (e) {
      error = String(e);
    }
  }

  async function open() {
    error = "";
    try {
      await invoke("open_manager", { gameDir, prefix });
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function refresh() {
    console.debug(`[w3mm] refresh: start`);
    appState = await invoke<AppState>("list_mods");
    // Drop selections for rows that no longer exist (uninstall / replace).
    if (selectedIds.size && appState) {
      const live = new Set(appState.mods.map((m) => m.id));
      const kept = new Set([...selectedIds].filter((id) => live.has(id)));
      if (kept.size !== selectedIds.size) {
        selectedIds = kept;
        if (lastSelected && !kept.has(lastSelected)) lastSelected = null;
      }
    }
    queue = await invoke<QueueItem[]>("downloads_history").catch(() => queue);
    clashMap = await invoke<Record<string, string[]>>("clashes").catch(
      () => ({}),
    );
    annotMap = await invoke<Record<string, string[]>>(
      "annotation_clashes",
    ).catch(() => ({}));
    infoMap = await invoke<typeof infoMap>("analysis_summary").catch(
      () => ({}),
    );
    unmanaged = await invoke<string[]>("unmanaged_mods").catch(() => []);
    madeMap = {};
    if (appState) {
      for (const m of appState.mods.filter((x) => !x.sep)) {
        invoke<MadeFor>("made_for", { id: m.id })
          .then((r) => {
            madeMap[m.id] = r;
          })
          .catch(() => {});
      }
    }
    const [qt, qtip] = await invoke<[string, string, number]>("quota").catch(
      () => ["", "", 0] as [string, string, number],
    );
    void qtip;
    quotaText = qt;
    // Regrow hits from the persisted version cache (no network): downgrades
    // and restarts instantly show their known updates again.
    try {
      const cached = await invoke<typeof hits>("cached_updates");
      if (cached.length) {
        const seen = new Set(hits.map((h) => h.id));
        const fresh = cached.filter((h) => !seen.has(h.id));
        if (fresh.length) hits = [...hits, ...fresh];
      }
    } catch {}
    pruneHits();
    console.debug(`[w3mm] refresh: done`);
  }

  /** Drop update hits that no longer apply after install/change/update:
   *  mod gone, or installed version caught up to the remote version. */
  function pruneHits() {
    if (!hits.length || !appState) return;
    const before = hits.length;
    hits = hits.filter((h) => {
      const m = appState!.mods.find((x) => x.id === h.id && !x.sep);
      if (!m) return false;
      return verVerdict(h.remote, m.version) === "newer";
    });
    if (hits.length !== before) {
      flash(
        hits.length
          ? `${hits.length} update${hits.length === 1 ? "" : "s"} available`
          : "All tracked mods are current",
      );
    }
  }

  function flash(msg: string) {
    status = msg;
  }

  async function toggle(id: string, on: boolean) {
    error = "";
    await invoke("set_enabled", { ids: [id], on: !on });
    await refresh();
    await deploy(true);
  }

  async function deploy(quiet = false) {
    if (!quiet) {
      busy = "Deploying…";
      error = "";
    }
    console.debug(`[w3mm] deploy: invoking`);
    try {
      const running = await invoke<boolean>("game_running");
      if (running) {
        error = "Close the game before deploying";
        busy = "";
        return;
      }
      const files = await invoke<string[]>("deploy");
      console.debug(`[w3mm] deploy: done ${files.length} files`);
      busy = "";
      flash(`Deployed ${files.length} file${files.length === 1 ? "" : "s"}`);
      await refresh();
      try {
        const { sendNotification } = await import(
          "@tauri-apps/plugin-notification"
        );
        sendNotification({
          title: "W3MM",
          body: `Deployed ${files.length} files`,
        });
      } catch {}
    } catch (e) {
      error = String(e);
      busy = "";
    }
  }

  async function setPrio(id: string, ev: Event) {
    const n = Number((ev.target as HTMLInputElement).value);
    if (!n) return;
    await invoke("set_priority", { id, number: n });
    await refresh();
    await deploy(true);
  }

  async function removeMod(id: string) {
    const m = modById(id);
    if (!m || !confirm(`Uninstall “${m.name}” and its staged files?`)) return;
    await invoke("remove_mods", { ids: [id] });
    const n = new Set(selectedIds);
    n.delete(id);
    selectedIds = n;
    if (lastSelected === id) lastSelected = null;
    await refresh();
    await deploy(true);
  }

  function clearSelection() {
    selectedIds = new Set();
    lastSelected = null;
  }

  /** NMM-style multi-select: plain = single, Ctrl = toggle, Shift = range. */
  function handleSelect(id: string, mods: ClickModifiers) {
    if (!appState) return;
    const row = appState.mods.find((m) => m.id === id);
    if (!row || row.sep) return;
    const isCtrl = mods.ctrlKey || mods.metaKey;
    const isShift = mods.shiftKey;
    // Range uses visible mod order (separators skipped, like NMM slice).
    const order = appState.mods.filter((m) => !m.sep).map((m) => m.id);
    if (isShift && lastSelected && order.includes(lastSelected)) {
      const s = order.indexOf(lastSelected);
      const t = order.indexOf(id);
      if (s !== -1 && t !== -1) {
        const [a, b] = s < t ? [s, t] : [t, s];
        selectedIds = new Set(order.slice(a, b + 1));
        return;
      }
    }
    if (isCtrl) {
      const n = new Set(selectedIds);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      selectedIds = n;
      lastSelected = n.has(id) ? id : n.size ? [...n][n.size - 1] : null;
      return;
    }
    if (selectedIds.has(id) && selectedIds.size === 1) {
      selectedIds = new Set();
      lastSelected = null;
      return;
    }
    selectedIds = new Set([id]);
    lastSelected = id;
  }

  async function enableSelected() {
    if (!selectedIds.size) return;
    error = "";
    await invoke("set_enabled", { ids: [...selectedIds], on: true });
    await refresh();
    await deploy(true);
  }

  async function disableSelected() {
    if (!selectedIds.size) return;
    error = "";
    await invoke("set_enabled", { ids: [...selectedIds], on: false });
    await refresh();
    await deploy(true);
  }

  async function removeSelected() {
    if (!selectedIds.size) return;
    const names = [...selectedIds].map((id) => modById(id)?.name ?? id);
    const preview = names.slice(0, 5).join(", ") + (names.length > 5 ? ` (+${names.length - 5} more)` : "");
    if (!confirm(`Uninstall ${names.length} mod${names.length === 1 ? "" : "s"} (${preview}) and their staged files?`)) return;
    await invoke("remove_mods", { ids: [...selectedIds] });
    clearSelection();
    await refresh();
    await deploy(true);
  }

  async function addSection() {
    menuOpen = false;
    const name = prompt("Section name", "New section");
    if (!name || !appState) return;
    await invoke("add_separator", { index: appState.mods.length, name });
    await refresh();
  }

  async function addSectionAbove(sepId: string) {
    sepCtx = null;
    if (!appState) return;
    const name = prompt("Section name", "New section");
    if (!name) return;
    const idx = appState.mods.findIndex((m) => m.id === sepId);
    await invoke("add_separator", { index: idx === -1 ? 0 : idx, name });
    await refresh();
  }

  async function removeSection(sepId: string) {
    sepCtx = null;
    const m = appState?.mods.find((r) => r.id === sepId);
    if (!m || !confirm(`Remove section “${m.name}”? Its mods stay in the list.`)) return;
    await invoke("remove_section_cmd", { sepId });
    await refresh();
  }

  function onSepContext(id: string, ev: MouseEvent) {
    ev.preventDefault();
    ev.stopPropagation();
    ctx = null;
    sepCtx = { id, x: ev.clientX, y: ev.clientY };
  }

  function openEdit(id: string) {
    ctx = null;
    console.debug("[w3mm] open_tool_window edit", id);
    invoke("open_tool_window", {
      kind: "edit",
      query: `id=${encodeURIComponent(id)}`,
      path: id,
    }).catch((e) => {
      console.error("[w3mm] open_tool_window edit failed", e);
      error = String(e);
    });
  }

  function openResolver() {
    menuOpen = false;
    console.debug("[w3mm] open_tool_window resolver");
    invoke("open_tool_window", { kind: "resolver", query: "", path: "" }).catch(
      (e) => {
        console.error("[w3mm] open_tool_window resolver failed", e);
        error = String(e);
      },
    );
  }
  function openSettings() {
    menuOpen = false;
    console.debug("[w3mm] open_tool_window settings");
    invoke("open_tool_window", { kind: "settings", query: "", path: "" }).catch(
      (e) => {
        console.error("[w3mm] open_tool_window settings failed", e);
        error = String(e);
      },
    );
  }

  async function openNexusPage(id: string) {
    ctx = null;
    const m = modById(id);
    const nid = m?.nexus?.trim();
    if (!nid) {
      flash("No Nexus id on this mod — set one in Edit…");
      return;
    }
    try {
      await invoke("open_path", { target: `https://www.nexusmods.com/witcher3/mods/${nid}` });
    } catch (e) {
      console.error(`[w3mm] open nexus page failed: ${String(e)}`);
      error = String(e);
    }
  }

  async function openModFolder(id: string) {
    ctx = null;
    try {
      const dir = await invoke<string>("mod_dir", { id });
      await invoke("open_path", { target: dir });
    } catch (e) {
      console.error(`[w3mm] open mod folder failed: ${String(e)}`);
      error = String(e);
    }
  }

  /**
   * Single entry point for every update check. `ids === null` scans the
   * whole list; an array scopes to those mods. The command returns at once —
   * results arrive on `updates-done` (full replace vs partial merge).
   */
  async function checkForUpdates(ids: string[] | null) {
    menuOpen = false;
    error = "";
    if (ids === null) {
      busy = "Checking Nexus…";
    } else {
      if (!ids.length) return;
      checkingIds = new Set([...checkingIds, ...ids]);
      flash(
        ids.length === 1
          ? `Checking ${modById(ids[0])?.name ?? "mod"}…`
          : `Checking ${ids.length} mods…`,
      );
    }
    try {
      const cfg = await loadConfigNative();
      await invoke("check_updates", { apiKey: cfg.nexusKey, ...(ids === null ? {} : { ids }) });
    } catch (e) {
      if (ids === null) busy = "";
      else checkingIds = new Set([...checkingIds].filter((x) => !ids.includes(x)));
      error = String(e);
    }
  }

  async function checkOne(id: string) {
    ctx = null;
    await checkForUpdates([id]);
  }

  async function checkSelected() {
    await checkForUpdates([...selectedIds]);
  }

  async function updateSelected(premium?: boolean | null) {
    const targets = hits.filter((h) => selectedIds.has(h.id));
    if (!targets.length) return;
    const isPrem = premium ?? premiumCache;
    let i = 0;
    for (const h of targets) {
      i++;
      flash(`Updating ${i}/${targets.length}: ${h.name}…`);
      await updateMod(h.id, isPrem);
    }
  }

  async function reinstall(id: string) {
    ctx = null;
    const m = modById(id);
    if (!m?.archive) {
      flash("No archive recorded — pick the file again.");
      return;
    }
    console.debug("[w3mm] open_tool_window install (reinstall)", m.archive);
    const qp = new URLSearchParams({
      path: m.archive,
      name: m.name ?? "",
      version: m.version ?? "",
      nexus: (m.nexus ?? "").replace(/\D/g, ""),
    });
    await invoke("open_tool_window", {
      kind: "install",
      query: qp.toString(),
      path: m.archive,
    }).catch((e) => {
      console.error("[w3mm] open_tool_window install failed", e);
      error = String(e);
    });
  }

  async function importThem() {
    if (!unmanaged.length) return;
    busy = "Importing…";
    try {
      await invoke("import_unmanaged", { rels: unmanaged });
      await refresh();
      await deploy(true);
      flash("Imported unmanaged mods.");
    } catch (e) {
      error = String(e);
    }
    busy = "";
  }

  type UpdateResolution = {
    id: string;
    premium?: boolean;
    row_id?: string;
    file_id?: string;
    version?: string;
    local_path?: string;
    error?: string;
  };
  // Promises waiting on `update-resolved` for one mod id.
  const pendingUpdates = new Map<string, ((v: UpdateResolution) => void)[]>();

  /** Fire a background resolve; settles when `update-resolved` arrives. */
  async function requestUpdate(id: string, premium: boolean | null): Promise<UpdateResolution> {
    const cfg = await loadConfigNative();
    if (!cfg.nexusKey) throw new Error("Set Nexus API key in Settings first");
    return new Promise<UpdateResolution>((resolve) => {
      const arr = pendingUpdates.get(id) ?? [];
      arr.push(resolve);
      pendingUpdates.set(id, arr);
      invoke("update_mod", { id, apiKey: cfg.nexusKey, premium }).catch((e) => {
        // Synchronous validation failure (unknown mod / no Nexus ID):
        // drop our waiter so a later retry isn't double-settled.
        const cur = (pendingUpdates.get(id) ?? []).filter((w) => w !== resolve);
        if (cur.length) pendingUpdates.set(id, cur);
        else pendingUpdates.delete(id);
        resolve({ id, error: String(e) });
      });
    });
  }

  async function finishUpdate(p: UpdateResolution) {
    if (typeof p.premium === "boolean") premiumCache = p.premium;
    if (p.error) {
      error = p.error;
      return;
    }
    // The update file is already in Downloads: install from it directly —
    // no redownload, no Nexus page. The hit stays until the install lands
    // (pruneHits clears it afterwards).
    if (p.local_path) {
      const m = modById(p.id);
      const qp = new URLSearchParams({
        path: p.local_path,
        name: m?.name ?? "",
        version: p.version ?? "",
        nexus: (m?.nexus ?? "").replace(/\D/g, ""),
      });
      flash(`Update file already downloaded — installing ${m?.name ?? "mod"} from local copy…`);
      await invoke("open_tool_window", {
        kind: "install",
        query: qp.toString(),
        path: p.local_path,
      }).catch((e) => {
        error = String(e);
      });
      return;
    }
    if (p.premium === false) {
      // Free accounts: open the exact Nexus file page.
      if (!p.file_id) {
        flash("Already on the newest version.");
        return;
      }
      const nid = modById(p.id)?.nexus?.trim();
      if (!nid) {
        error = "No Nexus ID on this mod — set one in Edit…";
        return;
      }
      await invoke("open_path", {
        target: `https://www.nexusmods.com/witcher3/mods/${nid}?tab=files&file_id=${p.file_id}&nmm=1`,
      });
      flash("Pick Slow Download on the Nexus page, then Install mods → select the file.");
      return;
    }
    if (!p.row_id) {
      flash("Already on the newest version.");
      return;
    }
    // Optimistically drop from the banner (pruneHits() keeps it honest).
    hits = hits.filter((h) => h.id !== p.id);
    dlOpen = true;
    try {
      queue = await invoke<QueueItem[]>("queue_list");
      await invoke("queue_start", {
        id: p.row_id,
        destDir: await downloadsDir(),
        apiKey: (await loadConfigNative()).nexusKey,
      });
      queue = await invoke<QueueItem[]>("queue_list");
    } catch (e) {
      error = String(e);
    }
  }

  async function updateMod(id: string, premium?: boolean | null) {
    ctx = null;
    error = "";
    if (updatingIds.has(id)) return;
    // Instant feedback; the resolve (including the premium lookup when still
    // unknown) happens on a worker thread, so this never awaits network.
    updatingIds = new Set([...updatingIds, id]);
    flash(`Resolving update for ${modById(id)?.name ?? "mod"}…`);
    try {
      const p = await requestUpdate(id, premium ?? premiumCache);
      await finishUpdate(p);
    } catch (e) {
      console.error(`[w3mm] update failed: ${String(e)}`);
      error = String(e);
    } finally {
      updatingIds = new Set([...updatingIds].filter((x) => x !== id));
    }
  }

  async function updateAll() {
    menuOpen = false;
    if (!hits.length) return;
    const total = hits.length;
    let i = 0;
    for (const h of [...hits]) {
      i++;
      flash(`Updating ${i}/${total}: ${h.name}…`);
      await updateMod(h.id, premiumCache);
    }
  }

  async function checkUpdates() {
    await checkForUpdates(null);
  }

  async function play() {
    if (gameDir.toLowerCase().includes("steamapps")) {
      try {
        await invoke("open_path", { target: "steam://rungameid/292030" });
      } catch (e) {
        error = String(e);
      }
    } else {
      flash("Not a Steam install — start it from your launcher.");
    }
  }

  async function openPath(kind: string) {
    menuOpen = false;
    try {
      const target =
        kind === "game"
          ? gameDir
          : kind === "settings"
            ? await invoke<string>("settings_dir_path")
            : `${await invoke<string>("settings_dir_path")}/${kind}`;
      console.debug(`[w3mm] opening ${kind}: ${target}`);
      await invoke("open_path", { target });
    } catch (e) {
      console.error(`[w3mm] open ${kind} failed: ${String(e)}`);
      error = String(e);
    }
  }

  async function handleDataListReorder(
    from: string | number,
    to: string | number,
    pos: "before" | "after",
  ) {
    if (!appState) return;
    const fromId = String(from);
    const toId = String(to);
    if (fromId === toId) return;
    const fromIdx = appState.mods.findIndex((m) => m.id === fromId);
    const toIdx = appState.mods.findIndex((m) => m.id === toId);
    if (fromIdx === -1 || toIdx === -1) return;
    // Build the list as it would be after removing the dragged row.
    const withoutFrom = appState.mods.filter((m) => m.id !== fromId);
    // The index at which we want the dragged row to sit in that list.
    let targetIdx = pos === "before" ? toIdx : toIdx + 1;
    if (fromIdx < targetIdx) targetIdx--;
    if (targetIdx < 0) targetIdx = 0;
    if (targetIdx > withoutFrom.length) targetIdx = withoutFrom.length;
    const beforeId = withoutFrom[targetIdx]?.id ?? "";
    try {
      await invoke("move_mod", { id: fromId, before: beforeId });
      await refresh();
      await deploy(true);
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleCollapse(id: string) {
    collapsed[id] = !collapsed[id];
  }

  // ---- drag-drop reorder (original drags rows; priority follows list order)
  let dragId: string | null = $state(null);
  let dropBefore: string | null = $state(null);

  function onDragStart(id: string, ev: DragEvent) {
    dragId = id;
    dropBefore = null;
    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = "move";
      try {
        ev.dataTransfer.setData("text/plain", id);
      } catch {}
    }
  }

  function onDragOverRow(id: string, ev: DragEvent) {
    if (!dragId || dragId === id) return;
    ev.preventDefault();
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = "move";
    dropBefore = id;
  }

  async function onDropRow(id: string, ev: DragEvent) {
    ev.preventDefault();
    if (!dragId || dragId === id) {
      dragId = null;
      dropBefore = null;
      return;
    }
    const moving = dragId;
    dragId = null;
    dropBefore = null;
    try {
      await invoke("move_mod", { id: moving, before: id });
      await refresh();
      await deploy(true);
    } catch (e) {
      error = String(e);
    }
  }

  async function onDropSection(sepId: string, ev: DragEvent) {
    ev.preventDefault();
    ev.stopPropagation();
    if (!dragId) return;
    const moving = dragId;
    dragId = null;
    dropBefore = null;
    try {
      await invoke("move_to_section", { id: moving, sepId });
      await refresh();
      await deploy(true);
    } catch (e) {
      error = String(e);
    }
  }

  async function onDropEnd(ev: DragEvent) {
    ev.preventDefault();
    if (!dragId) return;
    const moving = dragId;
    dragId = null;
    dropBefore = null;
    try {
      await invoke("move_mod", { id: moving, before: "" });
      await refresh();
      await deploy(true);
    } catch (e) {
      error = String(e);
    }
  }

  function onDragEnd() {
    dragId = null;
    dropBefore = null;
  }

  async function pickArchives() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const sel = await open({
        multiple: true,
        filters: [
          {
            name: "Mod archive",
            extensions: ["zip", "7z", "rar", "tar", "gz", "tgz"],
          },
        ],
      });
      const paths: string[] = Array.isArray(sel)
        ? (sel as string[])
        : sel
          ? [sel as string]
          : [];
      for (const p of paths) {
        console.debug("[w3mm] open_tool_window install (picked)", p);
        await invoke("open_tool_window", {
          kind: "install",
          query: `path=${encodeURIComponent(p)}`,
          path: p,
        }).catch((e) => {
          console.error("[w3mm] open_tool_window install failed", e);
          error = String(e);
        });
      }
    } catch (e) {
      console.error("[w3mm] pickArchives failed", e);
    }
  }

  function onRowContext(id: string, ev: MouseEvent) {
    ev.preventDefault();
    ev.stopPropagation();
    // Right-clicking inside the current selection keeps it (bulk menu target);
    // otherwise select just this row.
    if (!selectedIds.has(id)) {
      selectedIds = new Set([id]);
      lastSelected = id;
    }
    sepCtx = null;
    ctx = { id, x: ev.clientX, y: ev.clientY };
  }

  type Chip = { text: string; tip: string; cls: string; icon?: any };

  function chipsFor(m: { id: string; name: string }): Chip[] {
    const chips: Chip[] = [];
    const bad = "bg-[#e3735f]/15 text-[#e3735f]";
    const warn = "bg-[#c9a45c]/15 text-[#c9a45c]";
    const nw = "bg-[#b5d95a]/15 text-[#b5d95a]";
    const files = "bg-[#86b0cf]/15 text-[#86b0cf]";
    const sc = sharedScripts(m.id);
    if (sc.length)
      chips.push({
        text: `${sc.length}`,
        icon: Flag,
        tip: sc.map((s) => `${s.file} — with ${s.with.join(", ")}`).join("\n"),
        cls: bad,
      });
    const xm = sharedXmls(m.id);
    if (xm.length)
      chips.push({
        text: `${xm.length}`,
        icon: List,
        tip: xm.map((s) => `${s.file} — with ${s.with.join(", ")}`).join("\n"),
        cls: bad,
      });
    const oc = otherClashes(m.id);
    if (oc.length)
      chips.push({ text: `${oc.length}`, icon: EqualNot, tip: oc.join("\n"), cls: warn });
    const an = annotCount(m.name);
    if (an)
      chips.push({
        text: `${an}`,
        icon: AtSign,
        tip: "Same RedKit symbol added by two mods",
        cls: bad,
      });
    const lost = lostCount(m.id);
    if (lost)
      chips.push({
        text: `${lost}`,
        icon: Copy,
        tip: `${lost} file${lost === 1 ? "" : "s"} overridden by higher mods`,
        cls: files,
      });
    const made = madeMap[m.id];
    if (made?.short)
      chips.push({
        text: made.short,
        tip: made.label || made.short,
        cls: made.status === "classic" ? bad : warn,
      });
    const h = hits.find((hh) => hh.id === m.id);
    if (h)
      chips.push({
        text: "",
        icon: ArrowUp,
        tip: `${h.local} → ${h.remote}`,
        cls: nw,
      });
    return chips;
  }

  function sepOf(id: string): string | null {
    let cur: string | null = null;
    if (!appState) return null;
    for (const r of appState.mods) {
      if (r.sep) cur = r.id;
      else if (r.id === id) return cur;
    }
    return cur;
  }

  function isHiddenByCollapse(id: string): boolean {
    if (filtering) return false;
    const s = sepOf(id);
    return s !== null && !!collapsed[s];
  }

  function countMembers(sepId: string): number {
    if (!appState) return 0;
    let counting = false;
    let n = 0;
    for (const r of appState.mods) {
      if (r.sep) {
        if (counting) break;
        if (r.id === sepId) counting = true;
      } else if (counting) {
        if (!filtering || r.name.toLowerCase().includes(q)) n++;
      }
    }
    return n;
  }

  function targetsOf(id: string): string[] {
    return modById(id)?.targets ?? [];
  }

  // ---- downloads panel ----
  async function notify(title: string, body: string) {
    try {
      const { sendNotification } = await import(
        "@tauri-apps/plugin-notification"
      );
      sendNotification({ title, body });
    } catch {}
  }

  async function downloadsDir(): Promise<string> {
    try {
      return await invoke<string>("downloads_dir_path");
    } catch {
      return "/tmp";
    }
  }

  async function openDownloadsFolder() {
    try {
      const d = await downloadsDir();
      console.debug(`[w3mm] opening downloads folder: ${d}`);
      await invoke("open_path", { target: d });
    } catch (e) {
      console.error(`[w3mm] open downloads folder failed: ${String(e)}`);
      error = String(e);
    }
  }

  /** Display name for the Install window (original _list_name, simplified). */
  function dlListName(qq: QueueItem): string {
    const modName = (qq.mod_name || "").trim();
    if ((qq.category || "").toUpperCase() === "MAIN" && modName) return modName;
    const title = (qq.file_title || "").trim();
    if (!title) return modName || qq.filename;
    if (!modName) return title;
    const key = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "");
    if (key(modName) && key(title).includes(key(modName))) return title;
    const base = modName.split(/\s+[-\u2013\u2014:|]\s+/)[0].trim();
    if (base && key(base).length >= 4 && key(title).includes(key(base)))
      return title;
    return `${base || modName} - ${title}`;
  }

  /** Open the Install window for a finished row (original offer_install). */
  async function offerInstall(qq: QueueItem, destOverride?: string) {
    const dest = destOverride ?? (await downloadsDir()) + "/" + qq.filename;
    archPath = dest;
    const qp = new URLSearchParams({
      path: dest,
      name: dlListName(qq),
      version: qq.version ?? "",
      nexus: (qq.mod_id ?? "").replace(/\D/g, ""),
    });
    await invoke("open_tool_window", {
      kind: "install",
      query: qp.toString(),
      path: dest,
    }).catch((e) => {
      error = String(e);
    });
  }

  async function dlNxm(preset?: string) {
    const url = preset ?? nxm;
    if (!url) return;
    nxm = url;
    console.debug(`[w3mm] nxm received: ${url}`);
    error = "";
    try {
      console.debug(`[w3mm] dlNxm: loading config`);
      const cfg = await loadConfigNative();
      console.debug(
        `[w3mm] dlNxm: config ok, nexusKey=${cfg.nexusKey ? "set" : "MISSING"}`,
      );
      if (!cfg.nexusKey) {
        error = "Set Nexus API key in Settings first";
        return;
      }
      // enqueue is instant (no network); the worker thread resolves metadata
      // and streams the file — progress/completion arrive as events.
      console.debug(`[w3mm] dlNxm: invoking queue_enqueue`);
      const id = await invoke<string>("queue_enqueue", { url });
      console.debug(`[w3mm] dlNxm: enqueued id=${id}`);
      dlOpen = true;
      queue = await invoke<QueueItem[]>("queue_list");
      const row = queue.find((qq) => qq.id === id);
      if (
        row &&
        (row.status === "active" ||
          row.status === "starting" ||
          row.status === "paused")
      ) {
        return; // already fetching this file
      }
      if (row && row.status === "done") {
        // already here: nothing to download again — offer install
        if (!dlSameFile(row)) await offerInstall(row);
        return;
      }
      console.debug(`[w3mm] dlNxm: invoking queue_start id=${id}`);
      await invoke("queue_start", {
        id,
        destDir: await downloadsDir(),
        apiKey: cfg.nexusKey,
      });
      console.debug(`[w3mm] dlNxm: queue_start acked id=${id}`);
      queue = await invoke<QueueItem[]>("queue_list");
    } catch (e) {
      console.error(`[w3mm] dlNxm FAILED: ${String(e)}`);
      error = String(e);
    }
  }

  async function dlRemove(qq: QueueItem) {
    try {
      await invoke("queue_remove", { id: qq.id });
      queue = await invoke<QueueItem[]>("queue_list");
    } catch (e) {
      error = String(e);
    }
  }

  async function dlTrash(qq: QueueItem) {
    if (!confirm(`Move ${qq.filename || "this download"} to the Trash?`))
      return;
    try {
      await invoke("queue_trash", { id: qq.id, destDir: await downloadsDir() });
      queue = await invoke<QueueItem[]>("queue_list");
    } catch (e) {
      error = String(e);
    }
  }

  async function dlMain(qq: QueueItem) {
    const running =
      qq.status === "active" ||
      qq.status === "starting" ||
      qq.status === "queued";
    if (running) {
      try {
        await invoke("queue_cancel", { id: qq.id });
        queue = await invoke<QueueItem[]>("queue_list");
      } catch (e) {
        error = String(e);
      }
      return;
    }
    if (
      qq.status === "error" ||
      qq.status === "failed" ||
      qq.status === "paused" ||
      qq.status === "cancelled"
    ) {
      try {
        await invoke("queue_start", {
          id: qq.id,
          destDir: await downloadsDir(),
          apiKey: (await loadConfigNative()).nexusKey,
        });
        queue = await invoke<QueueItem[]>("queue_list");
      } catch (e) {
        error = String(e);
        queue = await invoke<QueueItem[]>("queue_list").catch(() => queue);
      }
      return;
    }
    if (qq.status === "done") {
      await offerInstall(qq);
    }
  }

  onMount(() => {
    boot();
    let unlisten: (() => void) | undefined;
    let unlistenP: (() => void) | undefined;
    let unlistenD: (() => void) | undefined;
    let unlistenMeta: (() => void) | undefined;
    let unlistenM: (() => void) | undefined;
    let unlistenU: (() => void) | undefined;
    let unlistenUR: (() => void) | undefined;
    (async () => {
      try {
        queue = await invoke<QueueItem[]>("queue_list").catch(() => []);
        const { getCurrent, onOpenUrl } = await import(
          "@tauri-apps/plugin-deep-link"
        );
        const cur = await getCurrent().catch(() => []);
        console.debug(`[w3mm] deep-link getCurrent: ${JSON.stringify(cur)}`);
        if (cur?.length) {
          dlOpen = true;
          await dlNxm(cur[0]);
        }
        unlisten = await onOpenUrl(async (urls) => {
          console.debug(`[w3mm] deep-link onOpenUrl: ${JSON.stringify(urls)}`);
          if (urls?.length) {
            dlOpen = true;
            await dlNxm(urls[0]);
          }
        });
        unlistenP = await listen<{
          id: string;
          done: number;
          total: number;
          speed?: number;
        }>("download-progress", (e) => {
          queue = queue.map((qq) =>
            qq.id === e.payload.id
              ? {
                  ...qq,
                  done: e.payload.done,
                  total: e.payload.total,
                  speed: e.payload.speed ?? qq.speed,
                  status: "active",
                }
              : qq,
          );
        });
        unlistenD = await listen<{ id: string; path: string; error?: string }>(
          "download-done",
          async (e) => {
            queue = await invoke<QueueItem[]>("queue_list");
            const row = queue.find((qq) => qq.id === e.payload.id);
            if (e.payload.error) {
              error = row?.error || e.payload.error;
              return;
            }
            flash(`Downloaded → ${e.payload.path.split("/").pop()}`);
            await notify(
              "W3MM",
              e.payload.path.split("/").pop() ?? "download done",
            );
            // original offer_install: open Install unless it's the exact file installed
            if (row && row.status === "done") {
              if (!dlSameFile(row)) {
                await offerInstall(row, e.payload.path);
              }
            } else {
              archPath = e.payload.path;
            }
          },
        );
        unlistenMeta = await listen<{ id: string }>(
          "download-meta",
          async () => {
            queue = await invoke<QueueItem[]>("queue_list").catch(() => queue);
          },
        );
        unlistenM = await listen("mods-changed", async () => {
          clearSelection();
          await refresh();
          await deploy(true);
        });
        unlistenU = await listen<{
          hits: typeof hits;
          ids?: string[];
          partial?: boolean;
        }>(
          "updates-done",
          async (e) => {
            if (e.payload.partial && e.payload.ids) {
              // Scoped re-check: merge these ids into the existing banner.
              const fresh = new Map(e.payload.hits.map((h) => [h.id, h]));
              const checked = new Set(e.payload.ids);
              const known = new Set(hits.map((h) => h.id));
              // Genuinely new finds pop a dismissed banner back up.
              if ([...fresh.keys()].some((id) => !known.has(id))) {
                updatesDismissed = false;
              }
              hits = [
                ...hits.filter((h) => !checked.has(h.id)),
                ...[...checked].filter((id) => fresh.has(id)).map((id) => fresh.get(id)!),
              ];
              checkingIds = new Set([...checkingIds].filter((id) => !checked.has(id)));
              if (e.payload.ids.length === 1) {
                const id = e.payload.ids[0];
                const h = fresh.get(id);
                const m = modById(id);
                flash(
                  h
                    ? `Update available for ${m?.name ?? "mod"}: ${h.local} → ${h.remote}`
                    : `No update for ${m?.name ?? "this mod"}.`,
                );
              } else {
                const n = e.payload.hits.length;
                flash(
                  n
                    ? `${n} update${n === 1 ? "" : "s"} found in selection`
                    : "No updates in selection.",
                );
              }
            } else {
              // Fresh full check: a dismissed banner shows again.
              updatesDismissed = false;
              hits = e.payload.hits;
              busy = "";
              flash(
                hits.length
                  ? `${hits.length} update${hits.length === 1 ? "" : "s"} available`
                  : "All tracked mods are current",
              );
            }
            const [qt] = await invoke<[string, string, number]>("quota").catch(
              () => ["", "", 0] as [string, string, number],
            );
            quotaText = qt;
          },
        );
        unlistenUR = await listen<UpdateResolution>(
          "update-resolved",
          async (e) => {
            const waiters = pendingUpdates.get(e.payload.id) ?? [];
            pendingUpdates.delete(e.payload.id);
            for (const w of waiters) w(e.payload);
          },
        );
      } catch {}
    })();
    function onDocClick() {
      menuOpen = false;
      ctx = null;
      sepCtx = null;
      hoverTip = null;
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        menuOpen = false;
        ctx = null;
        sepCtx = null;
      }
    }
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      unlisten?.();
      unlistenP?.();
      unlistenD?.();
      unlistenMeta?.();
      unlistenM?.();
      unlistenU?.();
      unlistenUR?.();
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  });

  const [enCount, totalCount] = $derived(enabledCounts());
</script>

<div class="flex h-full min-h-0">
  <div
    class="flex min-h-0 min-w-0 flex-1 flex-col gap-[14px] px-[22px] pt-[18px] pb-[12px]"
  >
    <div class="flex items-center gap-[10px]">
      <div class="min-w-0 flex-1 leading-tight">
        <div class="text-[19pt] font-semibold tracking-tight">
          The Witcher 3
        </div>
        <div class="truncate text-[13px] text-muted-foreground">
          {enCount} of {totalCount} mods enabled
        </div>
      </div>
      <input
        bind:value={filter}
        placeholder={dlOpen && nexusKey
          ? "Filter mods and downloads"
          : "Filter mods"}
        class="w-[230px] rounded-[7px] border border-input bg-card px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground/70 focus:border-primary"
      />
      <Button variant="secondary" size="md" onclick={play}>Play</Button>
      <Button variant="primary" size="md" onclick={pickArchives} class="px-[18px]">Install mods</Button>
      <div class="relative">
        <button
          onclick={(e) => {
            e.stopPropagation();
            menuOpen = !menuOpen;
          }}
          class="relative rounded-[7px] border border-border bg-popover px-3 py-[7px] text-sm hover:bg-accent"
          aria-label="More">•••{#if hits.length && updatesDismissed && !menuOpen}<span
              class="absolute -top-1.5 -right-1.5 flex h-5 min-w-5 items-center justify-center rounded-full bg-[#b5d95a] px-1 text-[11px] font-bold text-[#1c2127]"
              >{hits.length}</span
            >{/if}</button
        >
        {#if menuOpen}
          <div
            role="menu"
            tabindex="-1"
            class="absolute right-0 z-30 mt-1 w-56 rounded-[7px] border border-border bg-popover py-1 shadow-xl"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
          >
            {#if hits.length && updatesDismissed}
              <button
                onclick={async () => {
                  menuOpen = false;
                  updatesDismissed = false;
                  updatesOpen = true;
                  await tick();
                  updatesBanner?.scrollIntoView({ block: "nearest" });
                }}
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
                ><span class="flex-1">Updates</span><span
                  class="flex h-5 min-w-5 items-center justify-center rounded-full bg-[#b5d95a] px-1 text-[11px] font-bold text-[#1c2127]"
                  >{hits.length}</span
                ></button
              >
            {/if}
            <button
              onclick={addSection}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >New section</button
            >
            <button
              onclick={checkUpdates}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Check Nexus for updates</button
            >
            <button
              onclick={openResolver}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Script decisions…</button
            >
            <button
              onclick={importThem}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              disabled={!unmanaged.length}
              >Import existing mods{#if unmanaged.length}
                ({unmanaged.length}){/if}</button
            >
            <div class="my-1 border-t border-border"></div>
            <button
              onclick={() => openPath("game")}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Open: Game folder</button
            >
            <button
              onclick={() => openPath("settings")}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Open: Settings folder</button
            >
            <button
              onclick={() => openPath("mods.settings")}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Open: mods.settings</button
            >
            <button
              onclick={() => openPath("input.settings")}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Open: input.settings</button
            >
            <div class="my-1 border-t border-border"></div>
            <button
              onclick={openSettings}
              class="block w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
              >Settings…</button
            >
          </div>
        {/if}
      </div>
    </div>

    {#if unmanaged.length}
      <div
        class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-[14px] py-2 text-sm"
      >
        <span class="flex-1"
          >{unmanaged.length} mod folder{unmanaged.length === 1 ? "" : "s"} in the
          game {unmanaged.length === 1 ? "is" : "are"} not managed yet.</span
        >
        <button
          onclick={importThem}
          class="rounded-[7px] border border-border bg-popover px-3 py-1 text-sm hover:bg-accent"
          >Import them</button
        >
      </div>
    {/if}
    {#if hits.length && !updatesDismissed}
      <div
        bind:this={updatesBanner}
        class="flex items-center gap-2 rounded-[7px] border border-border bg-card px-[14px] py-2 text-sm"
      >
        <button
          onclick={() => (updatesOpen = !updatesOpen)}
          title={updatesOpen ? "Hide update list" : "Show update list"}
          class="flex min-w-0 flex-1 items-center gap-1.5 text-left text-[#b5d95a]"
        >
          {#if updatesOpen}<ChevronDown class="size-4 shrink-0" />{:else}<ChevronRight class="size-4 shrink-0" />{/if}
          <span class="flex-1 truncate"
            >{hits.length} update{hits.length === 1 ? "" : "s"} on Nexus</span
          >
        </button>
        <button
          onclick={updateAll}
          disabled={updatingIds.size > 0}
          class="shrink-0 rounded-[7px] bg-[#b5d95a] px-3 py-1 text-sm font-semibold text-[#1c2127] hover:brightness-110 disabled:opacity-50"
          >{updatingIds.size > 0 ? `Updating… (${updatingIds.size})` : "Update all"}</button
        >
        {#if hits.some((h) => selectedIds.has(h.id))}
          <button
            onclick={() => updateSelected()}
            disabled={updatingIds.size > 0}
            class="shrink-0 rounded-[7px] border border-[#b5d95a] px-3 py-1 text-sm font-semibold text-[#b5d95a] hover:bg-[#b5d95a]/10 disabled:opacity-50"
            >Update selected ({hits.filter((h) => selectedIds.has(h.id)).length})</button
          >
        {/if}
        <button
          onclick={() => {
            updatesDismissed = true;
            updatesOpen = false;
          }}
          title="Dismiss"
          class="shrink-0 rounded-[6px] p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          ><X class="size-4" /></button
        >
      </div>
      {#if updatesOpen}
        <div class="flex max-h-56 flex-col gap-0.5 overflow-auto rounded-[7px] border border-border bg-card px-2 py-1.5">
          {#each hits as h}
            <div class="flex items-center gap-2 rounded-[6px] px-2 py-1.5 text-sm hover:bg-accent/40">
              <span class="min-w-0 flex-1 truncate">{h.name}</span>
              <span class="shrink-0 font-mono text-[12px] text-muted-foreground">{h.local} → <span class="text-[#b5d95a]">{h.remote}</span></span>
              <button
                onclick={() => updateMod(h.id)}
                disabled={updatingIds.has(h.id)}
                class="shrink-0 rounded-[6px] bg-[#b5d95a] px-2.5 py-0.5 text-[13px] font-semibold text-[#1c2127] hover:brightness-110 disabled:opacity-50"
                >{updatingIds.has(h.id) ? "Updating…" : "Update"}</button
              >
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- One cell renderer per column. Declared before <DataList> so the
         `columns` array below can reference them. -->
    {#snippet cellPriority({ item: m }: { item: ModRow })}
      {#if m.enabled}
        <input
          type="number"
          min="1"
          value={prioOf(m.id)}
          onchange={(e) => setPrio(m.id, e)}
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
          title="Priority — 1 wins"
          class="w-[40px] bg-transparent px-1 py-[3px] text-left text-[13px] font-semibold text-foreground outline-none"
          style="-moz-appearance:textfield;-webkit-appearance:none;"
        />
      {:else}
        <span class="text-muted-foreground/50">–</span>
      {/if}
    {/snippet}
    {#snippet cellName({ item: m }: { item: ModRow })}
      <div class="flex min-w-0 items-center gap-2">
        <input
          type="checkbox"
          checked={m.enabled}
          onchange={() => toggle(m.id, m.enabled)}
          onclick={(e) => e.stopPropagation()}
          class="h-[18px] w-[18px] shrink-0 cursor-pointer appearance-none rounded-[4px] border-[1.5px] border-[#4a535e] bg-transparent checked:border-[#c9a45c] checked:bg-[#c9a45c] checked:bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 18 18%22><path d=%22M5.2 9.3l2.5 2.5 5.1-5.3%22 fill=%22none%22 stroke=%22%231c2127%22 stroke-width=%222.1%22 stroke-linecap=%22round%22 stroke-linejoin=%22round%22/></svg>')] checked:bg-center checked:bg-no-repeat"
        />
        <span
          role="button"
          tabindex="0"
          class="relative min-w-0"
          onmouseenter={(e) => {
            hoverTip = { id: m.id, x: e.clientX, y: e.clientY };
          }}
          onmouseleave={() => {
            hoverTip = null;
          }}
          onfocus={() => {
            hoverTip = null;
          }}
          onkeydown={(e) => {
            if (e.key === "Enter") openEdit(m.id);
          }}
        >
          <span class="block truncate text-sm">{m.name}</span>
          {#if hoverTip && hoverTip.id === m.id && targetsOf(m.id).length}
            {@const tip = hoverTip}
            <span
              class="pointer-events-none fixed z-50 max-w-[420px] rounded-[6px] border border-[#5d6773] bg-[#2f3740] px-3 py-2 shadow-xl"
              style="left:{Math.min(tip.x + 12, window.innerWidth - 440)}px;top:{tip.y + 14}px"
            >
              <span class="block border-b border-[#5d6773] pb-1 text-[13px] font-semibold"
                >Installs to</span
              >
              {#each targetsOf(m.id).slice(0, 12) as t}
                <span class="block truncate font-mono text-[12px] text-[#86b0cf]"
                  >{t}</span
                >
              {/each}
              {#if targetsOf(m.id).length > 12}<span
                  class="block text-[11px] text-muted-foreground"
                  >… {targetsOf(m.id).length - 12} more</span
                >{/if}
            </span>
          {/if}
        </span>
      </div>
    {/snippet}
    {#snippet cellVersion({ item: m }: { item: ModRow })}
      <span class="truncate text-[13px]">{m.version}</span>
    {/snippet}
    {#snippet cellInstalled({ item: m }: { item: ModRow })}
      <span class="truncate text-[13px]">{fmtDate(m.updated)}</span>
    {/snippet}
    {#snippet cellStatus({ item: m }: { item: ModRow })}
      <div class="flex flex-wrap gap-1 py-1">
        {#each chipsFor(m) as c}
          {@const Icon = c.icon}
          <span
            title={c.tip}
            class="inline-flex items-center gap-1 rounded-full px-2 py-[1px] text-[11px] font-semibold {c.cls}"
          >
            {#if Icon}<Icon class="size-3" />{/if}
            {#if c.text}<span>{c.text}</span>{/if}
          </span>
        {/each}
      </div>
    {/snippet}

    <!-- List + selection toolbar share one wrapper so the page's flex gap
         doesn't separate them; the toolbar sits flush against the list. -->
    <div class="flex min-h-0 flex-1 flex-col">
      <div
        class="flex min-h-0 flex-1 flex-col overflow-hidden border border-border bg-card {selectedIds.size
          ? 'rounded-t-[7px]'
          : 'rounded-[7px]'}"
      >
        <DataList
          flexColumn="name"
          columns={[
            {
              id: "priority",
              label: "Priority",
              width: 76,
              cell: cellPriority,
            },
            { id: "name", label: "Mod", width: 340, cell: cellName },
            { id: "version", label: "Version", width: 100, cell: cellVersion },
            {
              id: "installed",
              label: "Installed",
              width: 110,
              cell: cellInstalled,
            },
            { id: "status", label: "Status", width: 118, cell: cellStatus },
          ]}
          items={appState ? appState.mods : []}
          keyOf={(m) => m.id}
          isSelected={(m) => selectedIds.has(m.id)}
          onSelect={(m, mods) => {
            handleSelect(m.id, mods);
          }}
          onContextMenu={(m, e) => {
            if (m.sep) onSepContext(m.id, e);
            else onRowContext(m.id, e);
          }}
          onBackgroundClear={() => {
            clearSelection();
          }}
          onActivate={(m) => openEdit(m.id)}
          isDetail={(m) => !!m.sep}
          canReorder={!filtering}
          onReorder={(from, to, pos) => handleDataListReorder(from, to, pos)}
        >
          {#snippet detail({ row: m }: { row: ModRow })}
            <button
              class="flex w-full items-center gap-2 px-3 py-2 text-left text-[13px] font-semibold text-muted-foreground"
              onclick={() => toggleCollapse(m.id)}
            >
              <span class="flex h-4 w-4 items-center justify-center">
                {#if collapsed[m.id]}<ChevronRight class="size-4" />{:else}<ChevronDown class="size-4" />{/if}
              </span>
              {m.name}
              <span class="font-normal">({countMembers(m.id)})</span>
            </button>
          {/snippet}
          {#snippet empty()}
            <div
              class="flex h-full min-h-0 items-center justify-center p-8 text-center text-[12pt] text-muted-foreground"
            >
              {#if !appState}
                Set the game folder in Settings…
              {:else}
                {"No mods yet\n\nClick Install mods, or drop .zip / .7z / .rar files here"}
              {/if}
            </div>
          {/snippet}
        </DataList>
      </div>

      <SelectionToolbar
        attached
        count={selectedIds.size}
        onEnable={enableSelected}
        onDisable={disableSelected}
        onUninstall={removeSelected}
        onClear={clearSelection}
        onCheckUpdates={checkSelected}
        checkingUpdates={[...selectedIds].some((id) => checkingIds.has(id))}
      />
    </div>

    <div class="flex items-center gap-2">
      <span
        class="flex min-w-0 flex-1 items-center gap-1 truncate font-mono text-[12px] text-muted-foreground"
        title={error || status || gameDir}
      >
        {#if error}<TriangleAlert class="size-3.5 shrink-0" />{/if}
        <span class="truncate">{error ? error : status || gameDir}</span>
      </span>
      {#if quotaText}<span class="shrink-0 text-[12px] text-muted-foreground"
          >Nexus API: <span class="text-primary"
            >{quotaText.replace("Nexus API:", "").trim()}</span
          ></span
        >{/if}
      {#if busy}
        <span class="shrink-0 text-[12px] text-muted-foreground">{busy}</span>
        <span
          class="h-[6px] w-[180px] shrink-0 overflow-hidden rounded bg-muted"
          ><span class="block h-full w-1/3 animate-pulse rounded bg-primary"
          ></span></span
        >
      {/if}
      <button
        onclick={() => {
          dlOpen = !dlOpen;
        }}
        title={dlOpen ? "Hide downloads" : "Show downloads"}
        class="flex shrink-0 items-center gap-2 rounded-[7px] border px-3 py-1.5 text-sm {dlOpen
          ? 'border-primary bg-primary/15 text-primary'
          : 'border-border bg-popover text-muted-foreground hover:text-foreground hover:bg-accent'}"
        ><span>Downloads</span
        >{#if queue.some((qq) => qq.status === "active" || qq.status === "starting" || qq.status === "queued")}<span
            class="flex h-5 min-w-5 items-center justify-center rounded-full bg-[#c9a45c] px-1 text-[11px] font-bold text-[#1c2127]"
            >{queue.filter(
              (qq) =>
                qq.status === "active" ||
                qq.status === "starting" ||
                qq.status === "queued",
            ).length}</span
          >{/if}<span class="flex items-center">{#if dlOpen}<ChevronLeft class="size-4" />{:else}<ChevronRight class="size-4" />{/if}</span></button
      >
    </div>
  </div>

  {#if ctx && modById(ctx.id)}
    {@const cm = modById(ctx.id)!}
    {@const cid = ctx.id}
    <div
      role="menu"
      tabindex="-1"
      class="fixed z-50 w-56 rounded-lg border border-border bg-popover py-1 shadow-2xl"
      style="left:{Math.min(ctx.x, window.innerWidth - 240)}px;top:{Math.min(
        ctx.y,
        window.innerHeight - 260,
      )}px"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <button
        onclick={() => openEdit(cid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><Pencil class="size-4" /> Edit…</button
      >
      <button
        onclick={() => openNexusPage(cid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><ExternalLink class="size-4" /> Open Nexus page</button
      >
      <button
        onclick={() => openModFolder(cid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><FolderOpen class="size-4" /> Open folder</button
      >
      <button
        onclick={() => checkOne(cid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><RefreshCw class="size-4" /> Check for update</button
      >
      {#if hitFor(cid)}
        <button
          onclick={() => updateMod(cid)}
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm text-[#b5d95a] hover:bg-accent"
          ><ArrowUp class="size-4" /> Update to {hitFor(cid)?.remote}</button
        >
      {/if}
      <button
        onclick={() => reinstall(cid)}
        disabled={!cm.archive}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent disabled:opacity-50"
        ><Copy class="size-4" /> Reinstall from archive</button
      >
      <button
        onclick={() => openResolver()}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><GitMerge class="size-4" /> Script decisions…</button
      >
      <div class="my-1 border-t border-border"></div>
      <button
        onclick={() => removeMod(cid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><Trash2 class="size-4" /> Uninstall…</button
      >
    </div>
  {/if}

  {#if sepCtx}
    {@const sid = sepCtx.id}
    {@const sname = appState?.mods.find((r) => r.id === sid)?.name ?? ""}
    <div
      role="menu"
      tabindex="-1"
      class="fixed z-50 w-56 rounded-lg border border-border bg-popover py-1 shadow-2xl"
      style="left:{Math.min(sepCtx.x, window.innerWidth - 240)}px;top:{Math.min(
        sepCtx.y,
        window.innerHeight - 160,
      )}px"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <button
        onclick={() => {
          sepCtx = null;
          toggleCollapse(sid);
        }}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        >{collapsed[sid] ? "Unfold" : "Fold"}</button
      >
      <button
        onclick={() => addSectionAbove(sid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        >New section above</button
      >
      <div class="my-1 border-t border-border"></div>
      <button
        onclick={() => removeSection(sid)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-accent"
        ><Trash2 class="size-4" /> Remove section</button
      >
    </div>
    <span class="hidden">{sname}</span>
  {/if}

  {#if dlOpen}
    <div
      class="flex w-[400px] max-w-[80vw] shrink-0 flex-col gap-3 overflow-y-auto border-l border-[#363e48] bg-[#232930] px-4 py-[18px]"
    >
      <div class="flex items-start gap-1.5">
        <div class="flex-1 leading-tight">
          <div class="text-[13pt] font-semibold text-[#d9dee4]">Downloads</div>
          {#if dlSummary}
            <div class="mt-0.5 text-[12px] text-[#8c96a1]">{dlSummary}</div>
          {/if}
        </div>
        <button
          onclick={openDownloadsFolder}
          title="Open the downloads folder"
          class="rounded px-2 py-1 text-[13px] text-[#8c96a1] hover:bg-[#2b323a] hover:text-[#d9dee4]"
          >Open folder</button
        >
      </div>
      {#each Object.values(groupedQueue) as group}
        {@const rows = group.rows}
        {@const nested = rows.length > 1}
        {@const settled = rows.some((r) => r.status === "done" && dlSameFile(r))}
        {@const collapsed =
          dlCollapsed[group.key] ??
          rows.every(
            (r) =>
              (r.status === "done" && dlSameFile(r)) ||
              (settled && r.status === "done"),
          )}
        {@const single = rows.length === 1 ? rows[0] : null}
        {@const running = rows.some(
          (r) =>
            r.status === "active" ||
            r.status === "starting" ||
            r.status === "queued" ||
            r.status === "paused",
        )}
        <div
          class="rounded-[8px] border border-[#363e48] bg-[#2b323a] p-[14px]"
        >
          <div class="flex items-start gap-1.5">
            <button
              onclick={() => {
                dlCollapsed[group.key] = !collapsed;
              }}
              title={collapsed ? "Expand" : "Collapse"}
              class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-[13px] text-[#8c96a1] hover:text-[#dbb977]"
              >{#if collapsed}<ChevronRight class="size-4" />{:else}<ChevronDown class="size-4" />{/if}</button
            >
            <button
              onclick={() => {
                dlCollapsed[group.key] = !collapsed;
              }}
              class="min-w-0 flex-1 text-left text-[13.5px] font-semibold leading-snug text-[#d9dee4]"
              >{group.mod_name ||
                (group.mod_id ? `Nexus mod ${group.mod_id}` : "Mod")}</button
            >
            {#if collapsed && rows.length && settled}
              <span
                title="Installed: this exact version"
                class="mt-0.5 shrink-0 text-[#7fbf8a]"
                ><Check class="size-4" /></span
              >
            {/if}
            {#if single && !collapsed && !running && single.status === "done"}
              <button
                onclick={() => dlTrash(single)}
                title="Move the downloaded file to the Trash"
                class="mt-0.5 shrink-0 rounded p-1 text-[#8c96a1] hover:bg-[#e3735f]/15 hover:text-[#e3735f]"
                ><Trash2 class="size-4" /></button
              >
            {/if}
          </div>
          {#if !collapsed}
            <div
              class={nested
                ? "mt-3 flex flex-col gap-1 divide-y divide-[#363e48]/60 pl-4"
                : "mt-3 flex flex-col gap-2"}
            >
              {#each rows as qq}
                {@const st = dlStatus(qq)}
                {@const meta = dlMetaLine(qq)}
                {@const action = qq.status === "done" ? dlAction(qq) : ""}
                {@const isRunning =
                  qq.status === "active" ||
                  qq.status === "starting" ||
                  qq.status === "queued"}
                {@const mainLabel = isRunning
                  ? "Cancel"
                  : qq.status === "error" || qq.status === "failed"
                    ? "Retry"
                    : qq.status === "paused" || qq.status === "cancelled"
                      ? "Retry"
                      : action}
                <div
                  class={nested
                    ? "px-2 py-2"
                    : ""}
                >
                  {#if nested}
                    <div
                      class="mb-2 truncate text-[13px] text-[#d9dee4]"
                      title={qq.filename ?? ""}
                    >
                      {qq.filename || "Downloading…"}
                    </div>
                  {/if}
                  {#if isRunning}
                    <div
                      class="mb-2 h-[6px] overflow-hidden rounded bg-[#13171b]"
                    >
                      <div
                        class="h-full rounded bg-[#c9a45c]"
                        style="width:{qq.total
                          ? Math.round((100 * qq.done) / Math.max(1, qq.total))
                          : 0}%"
                      ></div>
                    </div>
                  {/if}
                  <div class="text-[13px]" style="color:{st.color}">
                    {st.text}
                  </div>
                  <div class="mt-2 flex items-center gap-1.5">
                    <span
                      class="min-w-0 flex-1 truncate text-[12.5px] text-[#8c96a1]"
                      title={meta}>{meta}</span
                    >
                    {#if !isRunning}
                      <button
                        onclick={() => dlRemove(qq)}
                        title="Take it off this list. A downloaded file stays in the downloads folder."
                        class="shrink-0 rounded-[6px] border border-[#363e48] bg-[#2b323a] px-3 py-1.5 text-[13px] text-[#d9dee4] hover:bg-[#363e48]"
                        >Remove</button
                      >
                    {/if}
                    {#if mainLabel}
                      <button
                        onclick={() => dlMain(qq)}
                        class="shrink-0 rounded-[6px] px-3.5 py-1.5 text-[13px] font-semibold {action ===
                        'Downgrade'
                          ? 'border border-[#363e48] bg-[#2b323a] text-[#d9dee4] hover:bg-[#363e48]'
                          : 'bg-[#c9a45c] text-[#1c2127] hover:bg-[#dbb977]'}"
                        >{mainLabel}</button
                      >
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
      {#if !queue.length}
        <p class="text-[13px] leading-relaxed text-[#8c96a1]">
          Click “Mod Manager Download” on a Witcher 3 mod's Nexus page. It
          downloads here, then the Install window opens.
        </p>
      {/if}
    </div>
  {/if}
</div>
