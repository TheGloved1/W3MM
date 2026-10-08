#!/usr/bin/env bun

import { execFileSync, execFile as execFileCb, execSync } from 'node:child_process';
import { existsSync, fstatSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { stdin as input, stdout as output } from 'node:process';
import * as readline from 'node:readline/promises';

const rl = readline.createInterface({ input, output });
let pipedLines: string[] | null = null;
let pipedIdx = 0;
function isPipedInput(): boolean {
  try {
    const stat = fstatSync(0);
    return stat.isFIFO() || stat.isFile();
  } catch {
    return input.isTTY === false;
  }
}
function getPipedLines(): string[] {
  if (pipedLines !== null) return pipedLines;
  try {
    if (isPipedInput()) {
      const data = readFileSync(0, 'utf-8');
      pipedLines = data.split(/\r?\n/);
      // A trailing newline terminates the last answer; it is not an answer.
      // Without this, an exhausted pipe looks like endless blank lines.
      if (pipedLines.length > 0 && pipedLines[pipedLines.length - 1] === '') pipedLines.pop();
    } else {
      pipedLines = [];
    }
  } catch {
    pipedLines = [];
  }
  return pipedLines;
}
const ask = async (q: string): Promise<string> => {
  if (isPipedInput()) {
    const lines = getPipedLines();
    const ans = pipedIdx < lines.length ? lines[pipedIdx++] : undefined;
    if (ans === undefined) {
      // No more piped answers: ABORT, never assume one. A blank default
      // here once auto-confirmed every remaining prompt (commit+tag+push).
      const err = new Error('Piped input exhausted — refusing to assume an answer');
      err.name = 'AbortError';
      throw err;
    }
    output.write(q);
    output.write(ans + '\n');
    return ans;
  }
  return rl.question(q);
};
const checkYesOrNo = (ans: string) => ans.trim().toLowerCase() === 'n';

const RED = '\x1b[0;31m';
const GREEN = '\x1b[0;32m';
const YELLOW = '\x1b[1;33m';
const BLUE = '\x1b[0;34m';
const DIM = '\x1b[2m';
const NC = '\x1b[0m';

function step(msg: string) {
  console.log(`\n${BLUE}==>${NC} ${msg}`);
}
function ok(msg: string) {
  console.log(`  ${GREEN}ok${NC} ${msg}`);
}
function fail(msg: string): never {
  // A failure after the version bump must not leave bumped files behind:
  // best-effort revert the working tree before exiting.
  revertPendingRelease();
  console.error(`  ${RED}error${NC} ${msg}`);
  process.exit(1);
}

// Set once the release starts mutating the working tree (version bump
// onward) and cleared after the release commit lands. Any abort past that
// point — 'n' answer, Ctrl+C/EOF AbortError, or fail() — must attempt the
// same working-tree revert instead of leaving junk behind.
let pendingRevert: { snapshot: Record<string, string | null>; next: string; didAutoStash: boolean } | null = null;

function revertPendingRelease(): void {
  const pending = pendingRevert;
  pendingRevert = null;
  if (pending === null) return;
  revertWorkingTree(pending.snapshot, pending.next, pending.didAutoStash);
}

/// Restore snapshotted files and delete the per-version changelog, then
/// pop any autostash. Deletion is VERIFIED: a silent `rmSync` miss here is
/// what once left a stray `changelogs/v*.md` behind after a cancel.
function revertWorkingTree(
  fileSnapshot: Record<string, string | null>,
  next: string,
  didAutoStash: boolean,
): void {
  console.log(`\n${YELLOW}Reverting changes...${NC}`);
  for (const [file, content] of Object.entries(fileSnapshot)) {
    try {
      if (content === null) rmSync(file);
      else writeFileSync(file, content);
    } catch {
      console.log(`  ${YELLOW}warning${NC} Could not restore ${file} — check 'git diff'`);
    }
  }
  const changelog = `changelogs/v${next}.md`;
  if (existsSync(changelog)) {
    try {
      rmSync(changelog);
    } catch {}
    if (existsSync(changelog)) {
      console.log(`  ${YELLOW}warning${NC} Could not remove ${changelog} — delete it manually`);
    }
  }
  if (didAutoStash) {
    try {
      execSync('git stash pop', { stdio: 'inherit' });
      console.log(`  ${GREEN}ok${NC} Restored stashed changes`);
    } catch {
      console.log(`  ${YELLOW}warning${NC} Could not pop stash — check 'git stash list'`);
    }
  }
  console.log('Reverted to state before release.');
}

// ---------------------------------------------------------------------------
// Version parsing
// ---------------------------------------------------------------------------

interface VersionParsed {
  year: number;
  month: number;
  patch: number;
  prerelease: string | null;
  prereleaseNum: number;
}

function tryParseVersion(v: string): VersionParsed | null {
  const match = v.match(/^(\d{2})\.(\d{1,2})\.(\d+)(?:-(.+?)\.(\d+))?$/);
  if (!match) return null;
  return {
    year: parseInt(match[1], 10),
    month: parseInt(match[2], 10),
    patch: parseInt(match[3], 10),
    prerelease: match[4] ?? null,
    prereleaseNum: match[5] ? parseInt(match[5], 10) : 0,
  };
}

function parseVersion(v: string): VersionParsed {
  const parsed = tryParseVersion(v);
  if (!parsed) fail(`Invalid version: "${v}"`);
  return parsed;
}

function getLastNonBetaTag(): string {
  execSync('git fetch --tags --force', { stdio: 'ignore' });
  try {
    const tags = execSync('git tag --list "v*" --sort=-creatordate', { encoding: 'utf-8' })
      .trim()
      .split('\n')
      .filter(Boolean);
    for (const tag of tags) {
      const ver = tag.replace(/^v/, '');
      if (!/-/.test(ver)) return tag;
    }
    return '';
  } catch {
    return '';
  }
}

// ---------------------------------------------------------------------------
// Version resolution
// ---------------------------------------------------------------------------

function resolveNextVersion(current: string, bump: string, betaModifier: boolean): string {
  const parsed = parseVersion(current);
  const now = new Date();
  const yy = now.getFullYear() % 100;
  const mm = now.getMonth() + 1;
  const yStr = String(yy).padStart(2, '0');
  const mStr = String(mm);

  switch (bump) {
    case 'patch':
      if (parsed.year === yy && parsed.month === mm) {
        return betaModifier ? `${yStr}.${mStr}.${parsed.patch + 1}-beta.1` : `${yStr}.${mStr}.${parsed.patch + 1}`;
      }
      return betaModifier ? `${yStr}.${mStr}.0-beta.1` : `${yStr}.${mStr}.0`;
    case 'beta':
      if (parsed.prerelease === null) fail('Not a beta version. Use "patch beta" to start a beta series.');
      return `${String(parsed.year).padStart(2, '0')}.${parsed.month}.${parsed.patch}-beta.${parsed.prereleaseNum + 1}`;
    default:
      return bump;
  }
}

// ---------------------------------------------------------------------------
// Changelog generation
// ---------------------------------------------------------------------------

function generateChangelog(next: string, baseTag?: string, endTag = 'HEAD'): { changelogEntry: string; releaseEntry: string } {
  let rangeStart = baseTag;
  if (rangeStart === undefined && endTag === 'HEAD') {
    try {
      rangeStart = execSync('git tag --list "v*" --sort=-creatordate', { encoding: 'utf-8' }).trim().split('\n')[0] ?? '';
    } catch {
      rangeStart = '';
    }
  }

  const range = rangeStart ? `${rangeStart}..${endTag}` : endTag;

  const log = execSync(`git log ${range} --pretty=format:"%s%n" --reverse`, { encoding: 'utf-8' });
  // Release commits ("chore: release vX") belong to the tagging process,
  // not the release content — post-tag regeneration would otherwise list
  // every version under its own Other section.
  const lines = log.split('\n').filter(Boolean).filter((l) => !/^chore:\s*release\s+v/i.test(l));

  interface ScopedEntry {
    scope: string | null;
    msg: string;
  }

  const addedRaw: ScopedEntry[] = [];
  const fixedRaw: ScopedEntry[] = [];
  const changedRaw: ScopedEntry[] = [];
  const choreRaw: ScopedEntry[] = [];
  const otherRaw: ScopedEntry[] = [];

  const pattern = /^(\w+)(\(.*?\))?!?:\s(.+)$/;
  const revertPattern = /^[Rr]evert\s+"(.+)"$/;

  // `Revert "subject"` lines quote the reverted commit: parse the inner
  // subject and cancel the pair so a commit and its revert don't both show.
  const reverted: string[] = [];
  function parseEntry(line: string): { type: string; entry: ScopedEntry } | null {
    const m = line.match(pattern);
    if (!m) return null;
    const [, type, scope, msg] = m;
    return { type, entry: { scope: scope ? scope.slice(1, -1) : null, msg } };
  }

  // Collapse near-duplicate messages ("Update README" x4, "fix typo" vs
  // "Fix typo."): normalize case/punctuation/whitespace, then within each
  // section keep the longest entry among ones that match exactly or where
  // one contains the other.
  function normalize(s: string): string {
    return s
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s]/gu, '')
      .replace(/\s+/g, ' ')
      .trim();
  }
  function dedupe(entries: string[]): string[] {
    const kept: { raw: string; norm: string }[] = [];
    for (const entry of entries) {
      const norm = normalize(entry);
      if (!norm) continue;
      let dominated = false;
      for (const k of kept) {
        if (k.norm === norm || k.norm.includes(norm) || norm.includes(k.norm)) {
          dominated = true;
          if (norm.length > k.norm.length) {
            k.raw = entry;
            k.norm = norm;
          }
          break;
        }
      }
      if (!dominated) kept.push({ raw: entry, norm });
    }
    return kept.map((k) => k.raw);
  }

  for (const line of lines) {
    const rm = line.match(revertPattern);
    if (rm) {
      const inner = parseEntry(rm[1]);
      if (inner) {
        // Cancel the original wherever it landed; a lone revert (target
        // outside this range) is listed once under its own type.
        const norm = normalize(`${inner.entry.scope ?? ''} ${inner.entry.msg}`);
        reverted.push(norm);
        const stillThere = [addedRaw, fixedRaw, changedRaw, choreRaw, otherRaw].some((list) =>
          list.some((e) => normalize(`${e.scope ?? ''} ${e.msg}`) === norm),
        );
        if (!stillThere) {
          const entry = { scope: inner.entry.scope, msg: `Revert: ${inner.entry.msg}` };
          (inner.type === 'feat' ? addedRaw : inner.type === 'fix' ? fixedRaw : inner.type === 'chore' ? choreRaw : 'refactor,perf,style'.includes(inner.type) ? changedRaw : otherRaw).push(entry);
        }
        continue;
      }
    }
    const parsed = parseEntry(line);
    if (parsed) {
      const { type, entry } = parsed;
      switch (type) {
        case 'feat':
          addedRaw.push(entry);
          break;
        case 'fix':
          fixedRaw.push(entry);
          break;
        case 'refactor':
        case 'perf':
        case 'style':
          changedRaw.push(entry);
          break;
        case 'chore':
          choreRaw.push(entry);
          break;
        default:
          otherRaw.push(entry);
          break;
      }
    } else {
      // Non-conventional commit: treat as Other
      otherRaw.push({ scope: null, msg: line });
    }
  }

  // Cancel reverted commits: drop originals wherever they landed. (Lone
  // reverts were already filed above as `Revert: …` and don't match.)
  for (const list of [addedRaw, fixedRaw, changedRaw, choreRaw, otherRaw]) {
    for (let i = list.length - 1; i >= 0; i--) {
      if (reverted.includes(normalize(`${list[i].scope ?? ''} ${list[i].msg}`))) {
        list.splice(i, 1);
      }
    }
  }

  const capitalize = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

  // Render one section: unscoped bullets first, then single-entry scopes
  // inline (`- **scope**: ```msg```), then duplicate scopes as a labeled
  // fenced block (`- scope:` + ``` list ```).
  function renderSection(entries: ScopedEntry[]): string {
    // Dedupe messages within each scope (plus the unscoped bucket) so
    // identical messages under different scopes don't eat each other.
    const byScope = new Map<string | null, string[]>();
    for (const e of entries) {
      const list = byScope.get(e.scope) ?? [];
      list.push(e.msg);
      byScope.set(e.scope, list);
    }
    const deduped = new Map<string | null, string[]>();
    for (const [scope, msgs] of byScope) {
      deduped.set(scope, dedupe(msgs));
    }
    const lines: string[] = [];
    // 1. Unscoped plain bullets.
    for (const msg of deduped.get(null) ?? []) {
      lines.push(`- ${msg}`);
    }
    // 2. Single-entry scopes stay inline.
    for (const [scope, msgs] of deduped) {
      if (scope !== null && msgs.length === 1) {
        lines.push(`- **${scope}**: \`\`\`${msgs[0]}\`\`\``);
      }
    }
    // 3. Duplicate scopes grouped as a fenced block.
    for (const [scope, msgs] of deduped) {
      if (scope !== null && msgs.length > 1) {
        if (lines.length) lines.push('');
        lines.push(`- ${scope}:`);
        lines.push('```');
        for (const msg of msgs) {
          lines.push(`- ${msg}`);
        }
        lines.push('```');
      }
    }
    return lines.join('\n');
  }

  const added = renderSection(addedRaw);
  const fixed = renderSection(fixedRaw);
  const changed = renderSection(changedRaw);
  const chores = renderSection(choreRaw);
  const other = renderSection(otherRaw);

  let body = '';
  if (added) body += '\n\n## Added\n\n' + added;
  if (fixed) body += '\n\n## Fixed\n\n' + fixed;
  if (changed) body += '\n\n## Changed\n\n' + changed;
  if (chores) body += '\n\n## Chores\n\n' + chores;
  if (other) body += '\n\n## Other\n\n' + other;
  if (!body) body = '\n\nMaintenance release.';

  const today = new Date().toISOString().slice(0, 10);
  const changelogEntry = `# [${next}] - ${today}${body}`;
  const releaseEntry = body.trimStart();

  return { changelogEntry, releaseEntry };
}

// ---------------------------------------------------------------------------
// Undo log types and helpers
// ---------------------------------------------------------------------------

interface UndoLog {
  version: { from: string; to: string };
  timestamp: string;
  branch: string;
  commit: string;
  tag: string;
  files: Record<string, string | null>;
  created: string[];
}

const UNDO_FILE = '.release-undo.json';

function captureFileSnapshot(files: string[]): Record<string, string | null> {
  const snapshot: Record<string, string | null> = {};
  for (const file of files) {
    snapshot[file] = existsSync(file) ? readFileSync(file, 'utf-8') : null;
  }
  return snapshot;
}

function saveUndoLog(log: UndoLog) {
  // One slot only: each release overwrites the previous log, so undo always
  // targets the most recent release and there is never a menu to pick from.
  writeFileSync(UNDO_FILE, JSON.stringify(log, null, 2) + '\n');
}

function loadUndoLog(): UndoLog | null {
  if (!existsSync(UNDO_FILE)) return null;
  try {
    return JSON.parse(readFileSync(UNDO_FILE, 'utf-8')) as UndoLog;
  } catch {
    return null;
  }
}

async function undoRelease(dryRun = false, force = false) {
  const log = loadUndoLog();
  if (log === null) fail('No undo log found — nothing to undo.');

  // Undo targets the release commit itself: refuse when later work sits on
  // top, otherwise rewinding (or reasoning about) the release is ambiguous.
  // --force skips this check (tags/release are still only deleted, never
  // history rewritten, so later work is never destroyed).
  const headRev = execSync('git rev-parse HEAD', { encoding: 'utf-8' }).trim();
  if (headRev !== log.commit && !force) {
    fail(
      `HEAD (${headRev.substring(0, 7)}) is past the release commit (${log.commit.substring(0, 7)}). ` +
        'Undo only works with nothing committed after the release (or pass --force).',
    );
  }
  console.log(`\nWill undo release v${log.version.to}:`);
  console.log(`  commit : ${log.commit.substring(0, 7)}`);
  console.log(`  tag    : ${log.tag}`);
  console.log(`  branch : ${log.branch}`);

  const proceed = await ask(`\nProceed? This will delete the local/remote tag and the GitHub release. (y/N) `);
  if (proceed.toLowerCase() !== 'y') {
    console.log('Aborted.');
    process.exit(0);
  }

  const branch = execSync('git branch --show-current', { encoding: 'utf-8' }).trim();
  if (branch !== log.branch) {
    const sw = await ask(`  Not on '${log.branch}' (on '${branch}'). Switch? (y/N) `);
    if (sw.toLowerCase() !== 'y') {
      console.log('Aborted.');
      process.exit(0);
    }
    if (!dryRun) execSync(`git checkout ${log.branch}`, { encoding: 'utf-8' });
  }

  if (dryRun) {
    console.log(`\n${YELLOW}DRY RUN${NC} — no changes will be made\n`);
  }

  step('Deleting local tag');
  if (!dryRun) execSync(`git tag -d "${log.tag}"`, { stdio: 'ignore', encoding: 'utf-8' });
  ok(`Deleted local tag ${log.tag}`);

  step('Deleting remote tag');
  if (!dryRun) {
    try {
      execSync(`git push origin :refs/tags/${log.tag}`, { stdio: 'ignore', encoding: 'utf-8' });
    } catch {
      console.log(`  ${YELLOW}warning${NC} Could not delete remote tag (it may never have been pushed).`);
    }
  }
  ok(`Deleted remote tag ${log.tag}`);

  step('Deleting GitHub release');
  if (!dryRun) {
    try {
      execSync(`gh release delete "v${log.version.to}" --yes`, { stdio: 'ignore', encoding: 'utf-8' });
    } catch {
      console.log(`  ${YELLOW}warning${NC} Could not delete GitHub release.`);
    }
  }
  ok('GitHub release deleted');

  if (!dryRun) {
    rmSync(UNDO_FILE);
  }
  ok(`Cleaned up ${UNDO_FILE}`);

  console.log(`\n${GREEN}${'='.repeat(40)}${NC}`);
  console.log(`${GREEN}  Undid release v${log.version.to}${NC}`);
  console.log(`${GREEN}${'='.repeat(40)}${NC}\n`);
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

function showUsage() {
  console.log(`
Usage: ./scripts/release.ts [<bump> [beta]] [flags]

Bump commands:
  patch [beta]       Bump patch version. (e.g. 25.05.3 -> 25.05.4 or 25.05.4-beta.1)
  beta               Increment beta number (must already be beta)
  YY.MM.PATCH        Explicit version
  YY.MM.PATCH-beta.N Explicit beta version

  Append "beta" to start a beta:  patch beta
  No arguments on a beta version strips it to stable.

Flags:
  --no-changelog     Skip changelog generation
  --no-push          Commit and tag locally, skip git push
  --undo             Revert the most recent release (also: \`undo\`)
  --force, -f        With undo: skip the no-commits-after-release check
  --dry-run          Show what would be done without making changes
  --help, -h, help   Show this help

  Uncommitted changes are handled interactively: [s] stash & continue, [c] continue anyway, [a] abort.
  In non-interactive environments (CI) the script still exits with an error.

Changelog preview:
  changelog                      Preview changelog for current version
  changelog patch                Preview changelog for next patch
  rechangelog                    Regenerate all changelogs/ + CHANGELOG.md from tags

Release status:
  status [--run <id>] [--workflow <name>] [--interval <s>] [--once] [--logs]
                                 Watch the Release workflow (live progress UI)

Examples:
  ./scripts/release.ts patch              # 25.05.3 -> 25.05.4
  ./scripts/release.ts patch beta         # 25.05.3 -> 25.05.4-beta.1
  ./scripts/release.ts beta               # 25.05.4-beta.1 -> 25.05.4-beta.2
  ./scripts/release.ts                    # 25.05.4-beta.2 -> 25.05.4 (stable)
  ./scripts/release.ts 25.05.4            # exact version
  ./scripts/release.ts changelog patch    # preview changelog for next patch
  ./scripts/release.ts --undo             # revert the most recent release ('undo' works too)
   ./scripts/release.ts patch --no-push    # bump locally without pushing
   ./scripts/release.ts --dry-run          # dry run the next release
   ./scripts/release.ts patch --dry-run    # dry run a patch bump
   ./scripts/release.ts --undo --dry-run   # dry run an undo
   ./scripts/release.ts status --once    # snapshot of the Release workflow
`);
}

// ---------------------------------------------------------------------------
// Release status (merged from scripts/release-status.ts)
// ---------------------------------------------------------------------------


import { execFileSync, execFile as execFileCb } from 'node:child_process';

function execFileAsync(file: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFileCb(file, args, { encoding: 'utf-8', maxBuffer: 64 * 1024 * 1024 }, (err, stdout) => {
      if (err) reject(err);
      else resolve(stdout as string);
    });
  });
}

const POLL_DEFAULT_S = 10;
const ACTIVE = new Set(['queued', 'in_progress', 'waiting', 'requested', 'pending']);
const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const C = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  gray: '\x1b[90m',
};

interface RunInfo {
  databaseId: number;
  number: number;
  displayTitle: string;
  headBranch: string;
  headSha: string;
  event: string;
  status: string;
  conclusion: string;
  createdAt: string;
  updatedAt: string;
  url: string;
  workflowName: string;
}

interface StepInfo {
  name: string;
  number: number;
  status: string;
  conclusion: string;
  startedAt: string;
  completedAt: string;
}

interface JobInfo {
  databaseId: number;
  name: string;
  status: string;
  conclusion: string;
  startedAt: string;
  completedAt: string;
  url: string;
  steps: StepInfo[];
}

function statusUsage(): void {
  console.log(`usage: bun scripts/release.ts status [options]

Watch the Release workflow with a live progress UI.

options:
  --run <id>        watch a specific run id (default: newest active run)
  --workflow <name> workflow name (default: Release)
  --interval <s>    poll interval in seconds (default: ${POLL_DEFAULT_S})
  --once            print a single snapshot instead of watching
  --logs            on failure, print the failed logs at the end
  --help            show this help

exit codes: 0 when the watched run succeeds (or a snapshot was printed),
1 on failure/cancel, 2 on usage or lookup errors.`);
}

function parseStatusArgs(argv: string[]) {
  const opts = { run: 0, workflow: 'Release', interval: POLL_DEFAULT_S, once: false, logs: false, help: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--run') opts.run = Number(argv[++i] ?? '');
    else if (a === '--workflow') opts.workflow = argv[++i] ?? opts.workflow;
    else if (a === '--interval') opts.interval = Math.max(2, Number(argv[++i] ?? '') || POLL_DEFAULT_S);
    else if (a === '--once') opts.once = true;
    else if (a === '--logs') opts.logs = true;
    else if (a === '--help' || a === '-h') opts.help = true;
    else {
      console.error(`unknown argument: ${a}`);
      statusUsage();
      process.exit(2);
    }
  }
  if (!opts.run && Number.isNaN(opts.run)) {
    console.error('--run needs a numeric run id');
    process.exit(2);
  }
  return opts;
}

function sh(args: string[]): string {
  try {
    return execFileSync('gh', args, { encoding: 'utf-8', maxBuffer: 64 * 1024 * 1024 });
  } catch (e: any) {
    const err = e?.stderr?.toString() ?? e?.message ?? String(e);
    console.error(`gh failed: ${err.trim().split('\n').slice(-3).join('\n')}`);
    process.exit(2);
  }
}

function repoSlug(): string {
  try {
    const url = execFileSync('git', ['remote', 'get-url', 'origin'], { encoding: 'utf-8' }).trim();
    const m = url.match(/github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/);
    if (m) return m[1];
  } catch {}
  console.error('could not determine the GitHub repo from the origin remote');
  process.exit(2);
}

function fmtDur(ms: number): string {
  if (ms < 0) ms = 0;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m${String(s % 60).padStart(2, '0')}s`;
  return `${Math.floor(m / 60)}h${String(m % 60).padStart(2, '0')}m`;
}

function ts(t: string): number {
  const ms = Date.parse(t);
  // GitHub uses 0001-01-01T00:00:00Z for "not started"; treat as absent.
  if (Number.isNaN(ms) || ms < 0) return 0;
  return ms;
}

function jobElapsed(j: JobInfo, now: number): number {
  const start = ts(j.startedAt);
  if (!start) return 0;
  const end = j.status === 'completed' ? ts(j.completedAt) || now : now;
  return end - start;
}

function stepElapsed(s: StepInfo, now: number): number {
  const start = ts(s.startedAt);
  if (!start) return 0;
  const end = s.status === 'completed' ? ts(s.completedAt) || now : now;
  return end - start;
}

function statusIcon(status: string, conclusion: string, tick: number): string {
  if (status === 'completed') {
    if (conclusion === 'success') return `${C.green}✓${C.reset}`;
    if (conclusion === 'skipped') return `${C.gray}✓${C.reset}`;
    if (conclusion === 'cancelled') return `${C.gray}⊘${C.reset}`;
    return `${C.red}✗${C.reset}`;
  }
  if (status === 'in_progress') return `${C.yellow}${SPINNER[tick % SPINNER.length]}${C.reset}`;
  return `${C.gray}○${C.reset}`;
}

function shortJobName(name: string): string {
  // "build (ubuntu-latest, linux-x86_64, deb appimage rpm)" -> "build · linux-x86_64"
  const m = name.match(/^(\S+)\s*\(([^)]+)\)$/);
  if (!m) return name;
  const parts = m[2].split(',').map((p) => p.trim());
  const plat = parts.find((p) => /linux|windows|macos|darwin/i.test(p)) ?? parts[0] ?? '';
  return `${m[1]} · ${plat}`;
}

function isPostStep(name: string): boolean {
  return /^(Post|Set up job)\b/i.test(name);
}

function render(run: RunInfo, jobs: JobInfo[], tick: number): string {
  const now = Date.now();
  const lines: string[] = [];
  const active = run.status !== 'completed';
  const stateColor = active ? C.yellow : run.conclusion === 'success' ? C.green : C.red;
  const state = active ? run.status.replace('_', ' ') : (run.conclusion || run.status);

  lines.push(
    `${C.bold}${run.workflowName} · ${run.displayTitle}${C.reset}  ${stateColor}${state}${C.reset}  ${C.dim}${fmtDur(now - ts(run.createdAt))} elapsed${C.reset}`,
  );
  lines.push(`${C.dim}${run.url}  ·  ${run.event} ${run.headBranch} @ ${run.headSha.slice(0, 7)}${C.reset}`);
  lines.push('');

  let done = 0;
  for (const j of jobs) {
    const icon = statusIcon(j.status, j.conclusion, tick);
    const dt = j.status === 'completed' || ts(j.startedAt) ? `  ${C.dim}${fmtDur(jobElapsed(j, now))}${C.reset}` : '';
    const conc = j.status === 'completed' && j.conclusion && j.conclusion !== 'success' ? `  ${C.dim}${j.conclusion}${C.reset}` : '';
    lines.push(`${icon} ${shortJobName(j.name)}${dt}${conc}`);
    if (j.status === 'in_progress' || (j.status === 'completed' && j.conclusion !== 'success')) {
      for (const s of j.steps) {
        if (isPostStep(s.name)) continue;
        const si = statusIcon(s.status, s.conclusion, tick);
        const sd = s.status === 'completed' || ts(s.startedAt) ? ` ${C.dim}${fmtDur(stepElapsed(s, now))}${C.reset}` : '';
        const sc = s.status === 'completed' && s.conclusion && s.conclusion !== 'success' && s.conclusion !== 'skipped'
          ? ` ${C.red}${s.conclusion}${C.reset}`
          : '';
        const dim = s.status === 'pending' || s.conclusion === 'skipped' ? C.dim : '';
        lines.push(`    ${si} ${dim}${s.name}${C.reset}${sd}${sc}`);
      }
    }
    if (j.status === 'completed') done++;
  }

  lines.push('');
  if (active) {
    lines.push(`${C.dim}checked just now · ${done}/${jobs.length} jobs done · Ctrl+C to stop watching${C.reset}`);
  } else {
    const ok = run.conclusion === 'success';
    lines.push(
      ok
        ? `${C.green}${C.bold}release ${run.headBranch} succeeded in ${fmtDur(ts(run.updatedAt) - ts(run.createdAt))}${C.reset}`
        : `${C.red}${C.bold}release ${run.headBranch} ${run.conclusion || 'ended'} after ${fmtDur(ts(run.updatedAt) - ts(run.createdAt))}${C.reset}`,
    );
  }
  return lines.join('\n');
}

async function ghJson(cmd: string[], signal?: AbortSignal): Promise<any> {
  void signal;
  const stdout = await execFileAsync('gh', cmd);
  return JSON.parse(stdout);
}

function fetchRun(repo: string, id: number): RunInfo {
  const out = sh([
    'run', 'view', String(id), '-R', repo, '--json',
    'databaseId,number,displayTitle,headBranch,headSha,event,status,conclusion,createdAt,updatedAt,url,workflowName',
  ]);
  return JSON.parse(out) as RunInfo;
}

function fetchJobs(repo: string, id: number): JobInfo[] {
  const out = sh(['run', 'view', String(id), '-R', repo, '--json', 'jobs']);
  const jobs = (JSON.parse(out) as { jobs: JobInfo[] }).jobs ?? [];
  jobs.sort((a, b) => (ts(a.startedAt) || ts(a.databaseId)) - (ts(b.startedAt) || ts(b.databaseId)));
  return jobs;
}

async function fetchRunAsync(repo: string, id: number): Promise<RunInfo> {  return (await ghJson([
    'run', 'view', String(id), '-R', repo, '--json',
    'databaseId,number,displayTitle,headBranch,headSha,event,status,conclusion,createdAt,updatedAt,url,workflowName',
  ])) as RunInfo;
}

async function fetchJobsAsync(repo: string, id: number): Promise<JobInfo[]> {
  const out = (await ghJson(['run', 'view', String(id), '-R', repo, '--json', 'jobs'])) as { jobs: JobInfo[] };
  const jobs = out.jobs ?? [];
  jobs.sort((a, b) => (ts(a.startedAt) || ts(a.databaseId)) - (ts(b.startedAt) || ts(b.databaseId)));
  return jobs;
}

async function statusMain(argv: string[]): Promise<void> {
  const opts = parseStatusArgs(argv);
  if (opts.help) {
    statusUsage();
    return;
  }
  const repo = repoSlug();

  let runId = opts.run;
  if (!runId) {
    const out = sh([
      'run', 'list', '--workflow', opts.workflow, '-R', repo, '--limit', '10', '--json',
      'databaseId,number,displayTitle,headBranch,status,conclusion,createdAt,url',
    ]);
    const runs = JSON.parse(out) as { databaseId: number; status: string }[];
    const activeRun = runs.find((r) => ACTIVE.has(r.status));
    if (!activeRun) {
      const last = runs[0] as any;
      if (!last) {
        console.log(`no ${opts.workflow} runs found for ${repo}`);
        return;
      }
      console.log(`no ongoing ${opts.workflow} workflow (last: #${last.number} ${last.displayTitle} — ${last.conclusion || last.status})`);
      console.log(last.url);
      return;
    }
    runId = activeRun.databaseId;
  }

  const interactive = process.stdout.isTTY && !opts.once;
  if (interactive) process.stdout.write('\x1b[?25l');
  const showCursor = () => {
    if (interactive) process.stdout.write('\x1b[?25h');
  };
  process.on('SIGINT', () => {
    showCursor();
    console.log('\nstopped watching');
    process.exit(0);
  });

  const paint = (text: string) => {
    if (interactive) process.stdout.write('\x1b[2J\x1b[H' + text + '\n');
    else console.log(text);
  };

  async function showReleaseAssets(run: RunInfo): Promise<void> {
    try {
      const m = run.displayTitle.match(/v?(\d{2}\.\d{1,2}\.\d+)/);
      if (!m) return;
      const tag = `v${m[1]}`;
      const rel = await ghJson(['release', 'view', tag, '-R', repo, '--json', 'tagName,url,assets']);
      if (!rel || !rel.assets?.length) {
        console.log(`\n${C.dim}no release assets found for ${tag}${C.reset}`);
        return;
      }
      console.log(`\n${C.bold}Release assets for ${rel.tagName}${C.reset}`);
      console.log(`${C.dim}${rel.url}${C.reset}\n`);
      for (const a of rel.assets) {
        console.log(`${C.green}•${C.reset} ${a.name}  ${C.dim}${a.url}${C.reset}`);
      }
    } catch {
      // ignore if release not yet created
    }
  }

  const finishRun = async (run: RunInfo): Promise<never> => {
    showCursor();
    if (opts.logs && run.status === 'completed' && run.conclusion !== 'success') {
      console.log('\n--- failed logs ---');
      try {
        const logs = execFileSync('gh', ['run', 'view', String(runId), '-R', repo, '--log-failed'], {
          encoding: 'utf-8',
          maxBuffer: 64 * 1024 * 1024,
        });
        console.log(logs.split('\n').slice(-120).join('\n'));
      } catch (e: any) {
        console.error(`could not fetch logs: ${e?.message ?? e}`);
      }
    }
    if (run.conclusion === 'success') {
      await showReleaseAssets(run);
    }
    process.exit(run.status === 'completed' && run.conclusion !== 'success' ? 1 : 0);
  };

  // --once: single blocking snapshot.
  if (opts.once) {
    const run = await fetchRunAsync(repo, runId);
    const jobs = await fetchJobsAsync(repo, runId);
    paint(render(run, jobs, 0));
    await finishRun(run);
  }

  // Watch mode: poll the API in the background while a fast local timer
  // keeps the spinner and elapsed clocks animating, so the UI never looks frozen.
  let cache: { run: RunInfo; jobs: JobInfo[] } | null = null;
  let lastError = '';
  let tick = 0;
  let polling = false;

  paint(`${C.dim}connecting to run ${runId}…${C.reset}`);

  const drawTimer = setInterval(() => {
    if (!cache) {
      paint(`${C.dim}connecting to run ${runId}… ${lastError}${C.reset}`);
      return;
    }
    paint(render(cache.run, cache.jobs, tick++));
  }, 150);

  const poll = async () => {
    if (polling) return;
    polling = true;
    try {
      const [run, jobs] = await Promise.all([fetchRunAsync(repo, runId), fetchJobsAsync(repo, runId)]);
      cache = { run, jobs };
      lastError = '';
      if (run.status === 'completed') {
        paint(render(run, jobs, tick++));
        clearInterval(drawTimer);
        await finishRun(run);
      }
    } catch (e: any) {
      lastError = `${C.red}(retrying: ${String(e?.message ?? e).split('\n')[0]})${C.reset}`;
    } finally {
      polling = false;
    }
  };
  await poll();
  setInterval(poll, opts.interval * 1000);
  await new Promise(() => {});
}


// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function rechangelog(dryRun = false) {
  const tags = execSync('git tag --list "v*" --sort=creatordate', { encoding: 'utf-8' })
    .trim()
    .split('\n')
    .filter((t) => /^v\d{2}\.\d{1,2}\.\d+(-beta\.\d+)?$/.test(t));
  if (!tags.length) fail('No version tags found.');

  if (dryRun) {
    console.log(`\n${YELLOW}DRY RUN${NC} — no changes will be made\n`);
  }
  console.log(`  tags    : ${tags.length} (${tags[0]}..${tags[tags.length - 1]})`);

  // Preview the newest entry so a glance catches format regressions.
  const newest = tags[tags.length - 1];
  const newestVer = newest.replace(/^v/, '');
  const { changelogEntry: preview } = generateChangelog(
    newestVer,
    tags.length > 1 ? tags[tags.length - 2] : undefined,
    newest,
  );
  console.log(`\n${BLUE}=== Preview (${newest}) ===${NC}\n`);
  console.log(preview);

  if (!dryRun) {
    const looksGood = await ask('\nRegenerate all per-version changelogs + CHANGELOG.md? (Y/n) ');
    if (checkYesOrNo(looksGood)) {
      console.log('Aborted.');
      process.exit(0);
    }
  }

  const built: { tag: string; ver: string; date: string; body: string }[] = [];
  let prev = '';
  for (const tag of tags) {
    const ver = tag.replace(/^v/, '');
    const date =
      execSync(`git log -1 --format=%cs ${tag}`, { encoding: 'utf-8' }).trim() ||
      new Date().toISOString().slice(0, 10);
    const { releaseEntry } = generateChangelog(ver, prev || undefined, tag);
    built.push({ tag, ver, date, body: releaseEntry });
    if (!dryRun) {
      if (!existsSync('changelogs')) mkdirSync('changelogs', { recursive: true });
      writeFileSync(`changelogs/${tag}.md`, releaseEntry);
    }
    prev = tag;
  }

  if (!dryRun) {
    const current = readFileSync('CHANGELOG.md', 'utf-8');
    // Match `# [` or legacy `## [` version headers.
    const at = current.search(/^#{1,2} \[/m);
    const preamble = (at === -1 ? current : current.slice(0, at)).trimEnd();
    const entries = [...built]
      .reverse()
      .map((b) => `# [${b.ver}] - ${b.date}\n\n${b.body}`);
    writeFileSync('CHANGELOG.md', preamble ? `${preamble}\n\n${entries.join('\n\n')}\n` : `${entries.join('\n\n')}\n`);
  }
  ok(dryRun ? 'Preview only — nothing written' : `Rewrote ${built.length} changelogs + CHANGELOG.md`);
}

async function main() {
  const args = process.argv.slice(2);

  // Release status (merged from scripts/release-status.ts). Dispatched
  // before --help so `status --help` shows the status options.
  if (args[0] === 'status') {
    await statusMain(args.slice(1));
    return;
  }

  // Help
  if (args.includes('--help') || args.includes('-h') || args.includes('help')) {
    showUsage();
    rl.close();
    process.exit(0);
  }

  // Undo (`--undo` or bare `undo`)
  if (args.includes('--undo') || args[0] === 'undo') {
    const dryRun = args.includes('--dry-run');
    const force = args.includes('--force') || args.includes('-f');
    await undoRelease(dryRun, force);
    rl.close();
    process.exit(0);
  }

  // Regenerate every per-version changelog + CHANGELOG.md from tags
  if (args[0] === 'rechangelog') {
    await rechangelog(args.includes('--dry-run'));
    rl.close();
    process.exit(0);
  }

  // Dry run
  const dryRun = args.includes('--dry-run');

  // Extract flags
  const skipChangelog = args.includes('--no-changelog');
  const noPush = args.includes('--no-push');
  const flags = new Set(['--undo', '--no-changelog', '--no-push', '--dry-run', '--help', '-h']);
  const positional = args.filter((a) => !flags.has(a));

  const isChangelogMode = positional[0] === 'changelog';
  const releaseArgs = isChangelogMode ? positional.slice(1) : positional;

  const betaModifier = releaseArgs.includes('beta');
  const nonBeta = releaseArgs.filter((a) => a !== 'beta');
  const hasNoBump = nonBeta.length === 0;
  const bumpArg = nonBeta[0] ?? '';

  let bump: string;
  if (hasNoBump && !betaModifier) bump = '';
  else if (hasNoBump && betaModifier) bump = 'beta';
  else bump = bumpArg;

  // Validate tools
  for (const cmd of ['git']) {
    try {
      execSync(`where ${cmd}`, { stdio: 'ignore' });
    } catch {
      try {
        execSync(`command -v ${cmd}`, { stdio: 'ignore' });
      } catch {
        fail(`Required tool not found: ${cmd}`);
      }
    }
  }

  // Read current version
  const pkg = JSON.parse(readFileSync('package.json', 'utf-8'));
  const current = pkg.version as string;
  const parsed = parseVersion(current);

  // Resolve next version
  let next: string;
  if (bump === '') {
    if (parsed.prerelease === null) fail('Already a stable release. Use "patch" to bump, or specify an explicit version.');
    next = `${String(parsed.year).padStart(2, '0')}.${parsed.month}.${parsed.patch}`;
  } else if (['patch', 'beta'].includes(bump)) {
    next = resolveNextVersion(current, bump, betaModifier);
  } else if (/^\d{2}\.\d{1,2}\.\d+(-beta\.\d+)?$/.test(bump)) {
    if (betaModifier) fail('Cannot combine "beta" modifier with an explicit version. Specify the full version instead.');
    next = bump;
  } else {
    fail(`Invalid version or bump type: "${bump}"`);
  }

  console.log(`  bump type : ${bump === '' ? 'stable (strip beta)' : bump}`);
  console.log(`  current   : ${DIM}${current}${NC}`);
  console.log(`  next      : ${GREEN}${next}${NC}\n`);

  // Changelog-only mode
  if (isChangelogMode) {
    step('Generating changelog preview');

    execSync('git fetch --tags --force', { stdio: 'ignore' });

    const isStableFromBeta = parsed.prerelease !== null && !next.includes('-');
    const baseTag = isStableFromBeta ? getLastNonBetaTag() : undefined;
    const { changelogEntry, releaseEntry } = generateChangelog(next, baseTag);

    console.log(`\n${BLUE}=== CHANGELOG.md entry ===${NC}\n`);
    console.log(changelogEntry);
    console.log(`\n${BLUE}=== Release body (changelogs/v${next}.md) ===${NC}\n`);
    console.log(releaseEntry);
    rl.close();
    process.exit(0);
  }

  // Pre-flight checks
  step('Running pre-flight checks');

  let didAutoStash = false;
  if (!dryRun) {
    const status = execSync('git status --porcelain', { encoding: 'utf-8' }).trim();
    if (status) {
      // Non-interactive (CI) — fail fast when stdin is explicitly non-TTY
      if (process.stdin.isTTY === false) {
        fail('Uncommitted changes detected. Commit or stash them first.');
      }
      console.log(`  ${YELLOW}warning${NC} Uncommitted changes detected:`);
      try {
        const short = execSync('git status --short', { encoding: 'utf-8' }).trim();
        if (short) console.log(`${DIM}${short}${NC}`);
      } catch {}
      const ans = await ask('  [s] Stash & continue  [c] Continue anyway  [a] Abort [s/c/a] (a): ');
      const v = ans.trim().toLowerCase();
      if (v === 's') {
        try {
          execSync('git stash push -m "release: autostash before v' + next + '" --include-untracked', {
            stdio: 'inherit',
          });
          didAutoStash = true;
          ok('Stashed working tree');
        } catch (e) {
          fail('Failed to stash changes: ' + e);
        }
      } else if (v === 'c') {
        ok('Continuing with dirty tree');
      } else {
        console.log('Aborted.');
        process.exit(0);
      }
    } else {
      ok('Working tree clean');
    }
  } else {
    ok('Working tree clean (dry run — ignoring dirty check)');
  }

  const branch = execSync('git branch --show-current', { encoding: 'utf-8' }).trim();
  if (branch !== 'main') {
    console.log(`  ${YELLOW}warning${NC} You are on branch '${branch}', not 'main'.`);
    const reply = await ask('  Continue anyway? (y/N) ');
    if (reply.toLowerCase() !== 'y') {
      if (didAutoStash) {
        try {
          execSync('git stash pop', { stdio: 'inherit' });
        } catch {}
      }
      console.log('Aborted.');
      process.exit(0);
    }
  }

  // Confirm
  if (!dryRun) {
    const proceed = await ask(`Proceed with release v${next}? (Y/n) `);
    if (checkYesOrNo(proceed)) {
      if (didAutoStash) {
        try {
          execSync('git stash pop', { stdio: 'inherit' });
        } catch {}
      }
      console.log('Aborted.');
      process.exit(0);
    }
    // Past this point the tree gets mutated: arm the revert first so any
    // later abort ('n', Ctrl+C, fail()) restores the tree. Cleared on commit.
    pendingRevert = { snapshot: {}, next, didAutoStash };
  } else {
    console.log(`\n${YELLOW}DRY RUN${NC} — no changes will be made\n`);
  }

  const isBetaRelease = next.includes('-');
  const isStableFromBeta = parsed.prerelease !== null && !next.includes('-');

  // Snapshot files before any changes (for undo log + mid-run reverts)
  const fileSnapshot =
    dryRun ?
      {}
    : captureFileSnapshot([
        'package.json',
        'src-tauri/Cargo.toml',
        'src-tauri/Cargo.lock',
        'src-tauri/tauri.conf.json',
        ...(skipChangelog || isBetaRelease ? [] : ['CHANGELOG.md']),
      ]);
  if (pendingRevert !== null) pendingRevert.snapshot = fileSnapshot;

  // Bump versions
  step('Updating version numbers');

  if (!dryRun) {
    pkg.version = next;
    writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
    try {
      execSync('bun run sync-version', { stdio: 'inherit' });
    } catch (error) {
      fail('Failed to sync version: ' + error);
    }
  }
  ok('package.json');
  ok('Synced version to Cargo.toml, Cargo.lock, and tauri.conf.json');

  // Changelog
  if (skipChangelog) {
    ok('SKIP — changelog generation disabled');
  } else {
    step('Generating changelog');

    // For stable releases from beta, aggregate all commits since last non-beta tag
    const baseTag = isStableFromBeta ? getLastNonBetaTag() : undefined;
    const { changelogEntry, releaseEntry } = generateChangelog(next, baseTag);

    if (isBetaRelease) {
      ok('SKIP — CHANGELOG.md not updated for beta releases');
    } else if (!dryRun) {
      // Insert into CHANGELOG.md
      const changelogPath = 'CHANGELOG.md';
      if (existsSync(changelogPath)) {
        const changelog = readFileSync(changelogPath, 'utf-8');
        if (changelog.startsWith('# [')) {
          writeFileSync(changelogPath, changelogEntry + '\n\n' + changelog);
        } else {
          // Match `# [` or legacy `## [` version headers.
          const at = changelog.search(/^#{1,2} \[/m);
          if (at !== -1) {
            const before = changelog.slice(0, at);
            const after = changelog.slice(at);
            writeFileSync(changelogPath, before.trimEnd() + '\n\n' + changelogEntry + '\n\n' + after);
          } else {
            writeFileSync(changelogPath, changelog.trimEnd() + '\n\n' + changelogEntry + '\n');
          }
        }
      } else {
        writeFileSync(changelogPath, changelogEntry + '\n');
      }
      ok('CHANGELOG.md');
    } else {
      ok('CHANGELOG.md');
    }

    // Write tag-specific changelog
    const changelogsDir = 'changelogs';
    if (!dryRun) {
      if (!existsSync(changelogsDir)) {
        mkdirSync(changelogsDir, { recursive: true });
      }
      writeFileSync(`${changelogsDir}/v${next}.md`, releaseEntry);
    }
    ok(`${changelogsDir}/v${next}.md`);

    // Preview
    console.log(`\n${BLUE}=== CHANGELOG.md entry ===${NC}\n`);
    console.log(changelogEntry);
    console.log(`\n${BLUE}=== Release body (changelogs/v${next}.md) ===${NC}\n`);
    console.log(releaseEntry);

    if (!dryRun) {
      const looksGood = await ask('Does the changelog look good? (Y/n) ');
      if (checkYesOrNo(looksGood)) {
        revertPendingRelease();
        console.log(`
If you want to edit manually, run:
  git diff
and re-run: ./scripts/release.ts ${bump}${betaModifier ? ' beta' : ''} ${noPush ? '--no-push' : ''} ${skipChangelog ? '--no-changelog' : ''}
`);
        process.exit(0);
      }
    }
  }

  // Git commit and tag
  step('Creating release commit');

  const filesToAdd = [
    'package.json',
    'src-tauri/Cargo.toml',
    'src-tauri/Cargo.lock',
    'src-tauri/tauri.conf.json',
    ...(skipChangelog ? [] : [`changelogs/v${next}.md`]),
    ...(skipChangelog || isBetaRelease ? [] : ['CHANGELOG.md']),
  ];
  if (!dryRun) {
    execSync(`git add ${filesToAdd.join(' ')}`, { encoding: 'utf-8' });
    execSync(`git commit -m "chore: release v${next}"`, { encoding: 'utf-8' });
    // Committed: the tree is the release now, nothing left to revert.
    pendingRevert = null;
  }
  ok(`Committed release v${next}`);

  step('Saving undo log');
  if (!dryRun) {
    const commitHash = execSync('git rev-parse HEAD', { encoding: 'utf-8' }).trim();
    const createdFiles: string[] = skipChangelog ? [] : [`changelogs/v${next}.md`];
    saveUndoLog({
      version: { from: current, to: next },
      timestamp: new Date().toISOString(),
      branch,
      commit: commitHash,
      tag: `v${next}`,
      files: fileSnapshot,
      created: createdFiles,
    });
  }
  ok(`.release-undo.json`);

  step(`Creating tag v${next}`);
  if (!dryRun) {
    execSync(`git tag -a "v${next}" -m "Release v${next}"`, { encoding: 'utf-8' });
  }
  ok('Tagged');

  // Push to remote. A failed push leaves local state (commit + tag) intact;
  // never let a remote-side failure dump a stack trace. Diagnose remote
  // vs local failures and print the exact recovery steps instead.
  const pushRecovery = (out: string) => {
    const remoteDown = /internal server error|remote rejected|http 5\d\d|rpc failed|service unavailable|bad gateway|gateway timeout/i.test(out);
    console.error(`\n${RED}Push to origin/${branch} failed.${NC}`);
    if (remoteDown) {
      console.error('  The remote rejected the push (GitHub-side error, not your repo).');
      console.error('  Local commit and tag are intact — nothing was lost.');
    } else {
      console.error('  Push output:');
      for (const line of out.split('\n').slice(0, 8)) {
        if (line.trim()) console.error(`    ${line.trim()}`);
      }
    }
    console.error('\n  To retry once GitHub recovers:');
    console.error(`    git push origin ${branch} --tags`);
    console.error('  Or skip pushing and do it later:');
    console.error('    ./scripts/release.ts --help  # see --no-push');
    console.error('  To throw this release away entirely:');
    console.error('    ./scripts/release.ts --undo');
  };
  if (noPush) {
    console.log(`\n${YELLOW}SKIP — push disabled by --no-push${NC}`);
    console.log(`${DIM}To push manually:${NC}`);
    console.log(`  git push origin ${branch} --tags`);
  } else {
    step(`Pushing to origin/${branch}`);
    if (!dryRun) {
      try {
        execSync(`git push origin ${branch} --tags`, { encoding: 'utf-8' });
      } catch (e) {
        const out = (e as { stderr?: unknown; stdout?: unknown; message?: unknown });
        const text = [out.stderr, out.stdout, out.message].map((x) => String(x ?? '')).join('\n');
        pushRecovery(text);
        process.exit(1);
      }
    }
    ok('Pushed');
  }

  if (didAutoStash && !dryRun) {
    step('Restoring stashed changes');
    try {
      execSync('git stash pop', { stdio: 'inherit' });
      ok('Restored stashed changes');
    } catch {
      console.log(`  ${YELLOW}warning${NC} Could not pop stash — check 'git stash list'`);
    }
  }

  // Done
  if (!dryRun) {
    console.log(`\n${GREEN}========================================${NC}`);
    console.log(`${GREEN}  Released v${next}${NC}`);
    console.log(`${GREEN}========================================${NC}\n`);
    console.log('Next step:');
    console.log('  CI will build and create a GitHub release.\n');
    console.log('To undo this release:');
    console.log('  ./scripts/release.ts --undo');
  }
}

main()
  .catch((err) => {
    // Crash abort (Ctrl+C / EOF AbortError from a prompt, unexpected throw)
    // after writes began: best-effort restore the tree before exiting.
    if (pendingRevert !== null) revertPendingRelease();
    if ((err as { name?: string })?.name === 'AbortError') {
      console.log('Aborted.');
      process.exit(130);
    }
    console.error('Release script failed:', err);
    process.exit(1);
  })
  .finally(() => rl.close());
