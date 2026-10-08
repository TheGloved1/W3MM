export interface AppConfig {
  theme: string;
  font: string;
  gameDir: string;
  prefix: string;
  stagingDir: string;
  stagingNoticeDismissed: boolean;
  nexusKey: string;
  codeFont: string;
  codeSize: number;
  mergerPath: string;
}

export interface ModRow {
  id: string;
  sep: boolean;
  name: string;
  enabled: boolean;
  version: string;
  nexus: string;
  archive: string;
  section: string;
  updated: number;
  collapsed: boolean;
  targets: string[];
  nexus_cat: string;
  main_of: string;
}

export interface ModsView {
  mods: ModRow[];
  priority: string[];
  deployed: Record<string, string>;
  filelist_added: Record<string, string[]>;
  resolutions: Record<string, number[]>;
  merge_kept: Record<string, string>;
  profiles: ProfileView[];
  active_profile: string | null;
}

export interface AppState extends ModsView {}

export interface ModSet {
  mods: ModRow[];
  priority: string[];
  deployed: Record<string, string>;
  filelist_added: Record<string, string[]>;
  resolutions: Record<string, number[]>;
  merge_kept: Record<string, string>;
}

/** Profile metadata; the full set lives in profiles/<id>/ on the backend. */
export interface Profile {
  id: string;
  name: string;
  updated: number;
}

/** Profile metadata with live mod counts (list_mods fills them in). */
export interface ProfileView {
  id: string;
  name: string;
  updated: number;
  mods: number;
  enabled: number;
}

export interface InstallPlan {
  moves: [string, string][];
  docs: string[];
}

export interface ScriptMergeResult {
  merged: string[];
  conflicts: { base_lo: number; base_hi: number; variants: string[][] }[];
  needs_resolution: boolean;
}

export interface XmlMergeResult {
  merged: string;
  conflicts: { path: string; kind: string; why: string; base_lines: string[]; variants: string[][]; proposed: string[] }[];
  needs_resolution: boolean;
}

export interface NxmLink {
  game: string;
  mod_id: string;
  file_id: string;
  key: string;
  expires: string;
}

export interface Snippets {
  user: Record<string, string[]>;
  input_xml: string[];
  filelist: string[];
  optional: string[];
}

export interface MadeFor {
  label: string;
  short: string;
  status: string;
}

export interface QueueItem {
  id: string;
  url: string;
  filename: string;
  total: number;
  done: number;
  status: string;
  error: string;
  mod_id: string;
  file_id: string;
  mod_name: string;
  file_title: string;
  version: string;
  category: string;
  speed: number;
  added: number;
}
