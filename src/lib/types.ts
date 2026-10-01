export interface AppConfig {
  theme: string;
  font: string;
  gameDir: string;
  prefix: string;
  nexusKey: string;
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

export interface AppState {
  mods: ModRow[];
  priority: string[];
  deployed: Record<string, string>;
  filelist_added: Record<string, string[]>;
  resolutions: Record<string, number[]>;
  merge_kept: Record<string, string>;
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
}
