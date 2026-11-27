#!/usr/bin/env node
// Browser test selection, sharding and the "nothing lost" check.
// One config file, e2e/browser-groups.json, defines the groups; this script is
// the only reader of it (CI workflows and profile-browser.sh call this script).
//
//   select  (--base REF | --files FILE)   groups a change needs, one per line
//   plan    <groups|all>                  JSON {tests, shards} for a CI matrix
//   run     <groups|all> [I/N]            run shard I of N against $BASE_URL
//   verify                                nothing-lost check (see below)
//   manifest                              print the current full selection
//
// <groups> is a comma- or space-separated list of group names, or "all".
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdtempSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const e2eDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const configPath = path.join(e2eDir, 'browser-groups.json');
const manifestPath = path.join(e2eDir, 'browser-selection.manifest.txt');
const playwright = path.join(e2eDir, 'node_modules/.bin/playwright');
const config = JSON.parse(readFileSync(configPath, 'utf8'));

function fail(message) {
  console.error(`browser-groups: ${message}`);
  process.exit(1);
}

function validateConfig() {
  const names = Object.keys(config.groups);
  for (const [name, group] of Object.entries(config.groups)) {
    if (!existsSync(path.join(e2eDir, group.spec))) fail(`group ${name}: no such spec ${group.spec}`);
  }
  const known = (list, where) => {
    for (const name of list) if (!config.groups[name]) fail(`${where}: unknown group ${name}`);
  };
  known(config.smoke, 'smoke');
  config.paths.forEach((rule, i) => {
    if (rule.groups !== 'all') known(rule.groups, `paths[${i}]`);
  });
  return names;
}

function resolveGroups(spec) {
  const names = validateConfig();
  const wanted = spec.split(/[\s,]+/).filter(Boolean);
  if (wanted.length === 0) fail('no groups given');
  if (wanted.includes('all')) return names;
  for (const name of wanted) if (!config.groups[name]) fail(`unknown group ${name}`);
  return names.filter(name => wanted.includes(name));
}

function globToRegExp(glob) {
  let source = '';
  for (let i = 0; i < glob.length; i += 1) {
    const char = glob[i];
    if (char === '*' && glob[i + 1] === '*') {
      source += '.*';
      i += 1;
    } else if (char === '*') source += '[^/]*';
    else source += char.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${source}$`);
}

function selectGroups(files) {
  const names = validateConfig();
  const selected = new Set();
  let unmapped = false;
  for (const file of files) {
    let mapped = false;
    for (const rule of config.paths) {
      if (!rule.match.some(glob => globToRegExp(glob).test(file))) continue;
      mapped = true;
      for (const name of rule.groups === 'all' ? names : rule.groups) selected.add(name);
    }
    if (!mapped) unmapped = true;
  }
  if (unmapped) for (const name of config.smoke) selected.add(name);
  return names.filter(name => selected.has(name));
}

// Every test a group runs, in the order Playwright runs them, identified by
// spec file and full title. Listing needs no server.
function listGroup(name) {
  const group = config.groups[name];
  const args = ['test', group.spec, '--list', '--reporter=json'];
  if (group.grep) args.push('-g', group.grep);
  const result = spawnSync(playwright, args, { cwd: e2eDir, encoding: 'utf8', maxBuffer: 1 << 28 });
  if (result.status !== 0) fail(`listing group ${name} failed:\n${result.stderr}`);
  const report = JSON.parse(result.stdout);
  if (report.errors?.length) fail(`listing group ${name} reported errors:\n${JSON.stringify(report.errors)}`);
  const tests = [];
  // A file's top suite carries the file path; nested suites are test.describe
  // blocks, whose titles are part of a test's identity for --test-list.
  const walk = (suite, describes) => {
    for (const spec of suite.specs ?? []) {
      tests.push({ group: name, file: spec.file, title: [...describes, spec.title].join(' › '), project: spec.tests[0].projectName });
    }
    for (const child of suite.suites ?? []) walk(child, [...describes, child.title]);
  };
  for (const fileSuite of report.suites) walk(fileSuite, []);
  return tests;
}

const testId = test => `${test.file} › ${test.title}`;

// Titles are the identity of a test across lists and the manifest, so a
// duplicate title in one file would make a skipped test indistinguishable.
function listGroups(names) {
  const tests = names.flatMap(listGroup);
  const seen = new Map();
  for (const test of tests) {
    const id = testId(test);
    if (seen.has(id)) fail(`test selected twice (groups ${seen.get(id)} and ${test.group}): ${id}`);
    seen.set(id, test.group);
  }
  return tests;
}

const listLine = test => `[${test.project}] › ${testId(test)}`;

// How many tests Playwright resolves from a --test-list built from these tests.
function resolvedCount(tests, dir) {
  let total = 0;
  for (const file of new Set(tests.map(test => test.file))) {
    const listFile = path.join(dir, `count-${total}-${path.basename(file)}.txt`);
    writeFileSync(listFile, tests.filter(test => test.file === file).map(listLine).join('\n') + '\n');
    const result = spawnSync(playwright, ['test', '--test-list', listFile, '--list', '--reporter=json'], { cwd: e2eDir, encoding: 'utf8', maxBuffer: 1 << 28 });
    if (result.status !== 0) fail(`resolving test list for ${file} failed:\n${result.stderr}`);
    const report = JSON.parse(result.stdout);
    const count = suite => (suite.specs ?? []).length + (suite.suites ?? []).reduce((sum, child) => sum + count(child), 0);
    total += report.suites.reduce((sum, suite) => sum + count(suite), 0);
  }
  return total;
}

const shardOf = (tests, index, count) => tests.filter((_, i) => i % count === index - 1);

function parseShard(text) {
  const match = /^(\d+)\/(\d+)$/.exec(text ?? '1/1');
  if (!match || +match[1] < 1 || +match[1] > +match[2]) fail(`bad shard ${text}, expected I/N`);
  return [+match[1], +match[2]];
}

const [command, ...args] = process.argv.slice(2);

if (command === 'select') {
  const flag = args[0];
  let files;
  if (flag === '--base') {
    files = execFileSync('git', ['diff', '--name-only', `${args[1]}...HEAD`], { encoding: 'utf8' }).split('\n').filter(Boolean);
  } else if (flag === '--files') {
    files = readFileSync(args[1], 'utf8').split('\n').filter(Boolean);
  } else fail('select needs --base REF or --files FILE');
  console.log(selectGroups(files).join('\n'));
} else if (command === 'plan') {
  const names = resolveGroups(args[0] ?? '');
  const tests = listGroups(names).length;
  const full = names.length === Object.keys(config.groups).length;
  const count = full
    ? config.full_shards
    : Math.min(config.max_pr_shards, Math.max(1, Math.ceil(tests / config.tests_per_pr_shard)));
  console.log(JSON.stringify({ tests, shards: Array.from({ length: count }, (_, i) => `${i + 1}/${count}`) }));
} else if (command === 'run') {
  const names = resolveGroups(args[0] ?? '');
  const [index, count] = parseShard(args[1]);
  const mine = shardOf(listGroups(names), index, count);
  if (mine.length === 0) fail(`shard ${index}/${count} has no tests`);
  const dir = mkdtempSync(path.join(tmpdir(), 'browser-shard-'));
  console.log(`shard ${index}/${count}: ${mine.length} tests from ${names.join(', ')}`);
  // One Playwright process per spec file, as profile-browser.sh always ran
  // them: the first signs the test user up and the later ones reuse that
  // session. A failing file does not stop the others, so one run reports every
  // failure. Each file keeps its own output directory so a later file does not
  // delete the traces of an earlier failure.
  const files = [...new Set(mine.map(test => test.file))];
  let failed = 0;
  for (const file of files) {
    const listFile = path.join(dir, `${files.indexOf(file)}.txt`);
    writeFileSync(listFile, mine.filter(test => test.file === file).map(listLine).join('\n') + '\n');
    const run = spawnSync(playwright, ['test', '--test-list', listFile, `--output=test-results/${files.indexOf(file)}`, '--workers=1', '--reporter=line'], {
      cwd: e2eDir,
      stdio: 'inherit',
    });
    if (run.status !== 0) {
      failed += 1;
      console.error(`browser-groups: ${file} failed`);
    }
  }
  rmSync(dir, { recursive: true, force: true });
  process.exit(failed === 0 ? 0 : 1);
} else if (command === 'manifest') {
  console.log(listGroups(resolveGroups('all')).map(testId).sort().join('\n'));
} else if (command === 'verify') {
  // Nothing lost: (1) the recorded manifest of tests the full run covers is
  // still entirely selected; (2) sharding is a partition, so every selected
  // test runs in exactly one shard of the full workflow.
  const names = resolveGroups('all');
  const tests = listGroups(names);
  const ids = new Set(tests.map(testId));
  const recorded = readFileSync(manifestPath, 'utf8').split('\n').filter(Boolean);
  const lost = recorded.filter(id => !ids.has(id));
  const unrecorded = [...ids].filter(id => !recorded.includes(id));
  const count = config.full_shards;
  const ran = new Map();
  for (let index = 1; index <= count; index += 1) {
    for (const test of shardOf(tests, index, count)) ran.set(testId(test), (ran.get(testId(test)) ?? 0) + 1);
  }
  // The identities must also resolve to real tests when handed back to
  // Playwright, which is how a shard selects what to run: every shard's
  // resolved count must equal its planned count, so no test is silently skipped.
  const scratch = mkdtempSync(path.join(tmpdir(), 'browser-verify-'));
  const unresolved = [];
  for (let index = 1; index <= count; index += 1) {
    const mine = shardOf(tests, index, count);
    const resolved = resolvedCount(mine, scratch);
    if (resolved !== mine.length) unresolved.push(`shard ${index}/${count}: ${mine.length} planned, ${resolved} resolved by Playwright`);
  }
  rmSync(scratch, { recursive: true, force: true });
  const missingFromShards = [...ids].filter(id => ran.get(id) !== 1);
  if (unrecorded.length) console.log(`note: ${unrecorded.length} selected test(s) are not in the manifest yet (run "manifest" to record them):\n  ${unrecorded.join('\n  ')}`);
  if (lost.length || missingFromShards.length || unresolved.length) {
    if (unresolved.length) console.error(`TEST LIST DOES NOT RESOLVE:\n  ${unresolved.join('\n  ')}`);
    if (lost.length) console.error(`LOST: ${lost.length} recorded test(s) no longer run in the full selection:\n  ${lost.join('\n  ')}`);
    if (missingFromShards.length) console.error(`NOT IN EXACTLY ONE SHARD:\n  ${missingFromShards.join('\n  ')}`);
    process.exit(1);
  }
  console.log(`ok: ${recorded.length} recorded tests all run; ${ids.size} selected tests each run in exactly one of ${count} shards, and every shard's test list resolves in Playwright`);
} else {
  fail('usage: browser-groups.mjs select|plan|run|verify|manifest');
}
