import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { once } from 'node:events';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const exe = path.resolve(process.argv[2] || 'src-tauri/target/release/subforge.exe');
const data = mkdtempSync(path.join(os.tmpdir(), 'subforge-provider-removal-'));
const video = path.join(data, 'fixture.mp4');
const listener = net.createServer();
listener.listen(0, '127.0.0.1');
await once(listener, 'listening');
const { port } = listener.address();
await new Promise(resolve => listener.close(resolve));

async function waitFor(check) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const result = await check();
    if (result) return result;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('Timed out waiting for SubForge');
}

let child, browser;
try {
  execFileSync('src-tauri/resources/bin/ffmpeg.exe', [
    '-loglevel', 'error', '-f', 'lavfi', '-i', 'color=c=black:s=160x90:r=1',
    '-t', '3', '-c:v', 'mpeg4', video,
  ], { windowsHide: true });
  writeFileSync(video.replace('.mp4', '.en.srt'), '1\n00:00:00,000 --> 00:00:02,000\nHello.\n');
  const fixtureProvider = {
    kind: 'responses', endpoint: 'http://127.0.0.1:1', models: ['fixture'],
    enabled: true, cost: 'free', retries: 0, timeout_secs: 10,
  };
  writeFileSync(path.join(data, 'settings.json'), JSON.stringify({
    ui_language: 'en', target_language: 'el', source_language: 'en',
    shell_enabled: false, speech: [],
    translation: [
      { ...fixtureProvider, id: 'retired-zen-fixture', name: 'Retired Zen', kind: 'zen' },
      { ...fixtureProvider, id: 'provider-removal-stale-fixture', name: 'Removed fixture' },
      { ...fixtureProvider, id: 'provider-removal-kept-fixture', name: 'Kept fixture' },
    ],
  }));
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SUBFORGE_DATA_DIR: data,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    windowsHide: true,
  });
  const endpoint = `http://127.0.0.1:${port}`;
  await waitFor(() => fetch(`${endpoint}/json/version`).then(r => r.ok).catch(() => false));
  browser = await chromium.connectOverCDP(endpoint);
  const page = browser.contexts()[0].pages()[0];
  await page.waitForSelector('[data-page="settings"]');
  const invoke = (cmd, args = {}) => page.evaluate(
    ({ cmd, args }) => window.__TAURI_INTERNALS__.invoke(cmd, args), { cmd, args },
  );
  await invoke('add_files', { paths: [video] });
  const [queued] = (await invoke('snapshot')).jobs;
  assert.equal(queued.config.translation.length, 2);
  await invoke('job_action', { id: queued.id, action: 'pause' });
  await page.locator('[data-page="settings"]').click();
  await page.locator('[data-settings-tab="aiProviders"]').click();
  assert.equal(await page.locator('.settings-grid > .panel').first().locator('[data-edit="translation:0"]').count(), 1);
  assert.equal(await page.getByText('Retired Zen', { exact: true }).count(), 0);
  await page.locator('[data-edit="translation:0"]').click();
  await page.locator('#provider-remove').click();
  await page.locator('#modal').waitFor({ state: 'hidden' });
  const saved = JSON.parse(readFileSync(path.join(data, 'settings.json'), 'utf8'));
  assert.deepEqual(saved.translation.map(p => p.name), ['Kept fixture']);

  await page.locator('[data-page="queue"]').click();
  await page.locator('[data-job-action="resume"]').click();
  const job = await waitFor(async () => {
    const [current] = (await invoke('snapshot')).jobs;
    return current.status === 'failed' && current;
  });
  assert.equal(job.target, 'el');
  assert.match(job.message, /Kept fixture: key_missing/);
  assert.doesNotMatch(await invoke('job_log', { id: job.id }), /Removed fixture/);
  console.log('PASS: Provider removal saves immediately and resumed Greek jobs only try current providers.');
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
  }
  rmSync(data, { recursive: true, force: true });
}
