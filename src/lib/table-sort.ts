import { LazyStore } from "@tauri-apps/plugin-store";

// Generic persisted table sort, shared by the Mods and Bases lists.
// Stored in the shared settings.json under per-list keys so it never
// interferes with the main AppConfig shape.
const store = new LazyStore("settings.json");

export type SortDir = "asc" | "desc";

export interface TableSort<T extends string> {
  key: T;
  dir: SortDir;
}

export async function loadTableSort<T extends string>(
  storeKey: string,
  validKeys: readonly T[],
  fallback: TableSort<T>,
): Promise<TableSort<T>> {
  try {
    const raw = await store.get<TableSort<T>>(storeKey);
    if (raw && (validKeys as readonly string[]).includes(raw.key)) {
      return { key: raw.key, dir: raw.dir === "desc" ? "desc" : "asc" };
    }
  } catch {}
  return { ...fallback };
}

export async function saveTableSort<T extends string>(
  storeKey: string,
  sort: TableSort<T>,
): Promise<void> {
  try {
    await store.set(storeKey, sort);
    await store.save();
  } catch {}
}
