#!/usr/bin/env bun

import { execFileSync } from 'node:child_process';

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

function usage(): void {
  console.log(`usage: bun scripts/release-status.ts [options]

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

function parseArgs(argv: string[]) {
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
      usage();
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

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    usage();
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

  let tick = 0;
  for (;;) {
    let run: RunInfo;
    try {
      run = fetchRun(repo, runId);
    } catch {
      showCursor();
      process.exit(2);
    }
    const jobs = fetchJobs(repo, runId);
    const frame = render(run, jobs, tick++);
    if (interactive) {
      process.stdout.write('\x1b[2J\x1b[H' + frame + '\n');
    } else {
      console.log(frame);
    }
    if (run.status === 'completed' || opts.once) {
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
      process.exit(run.status === 'completed' && run.conclusion !== 'success' ? 1 : 0);
    }
    await new Promise((r) => setTimeout(r, opts.interval * 1000));
  }
}

main();
