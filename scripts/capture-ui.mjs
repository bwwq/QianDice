// One-off visual verification in the CNB container, always a fresh headless browser.
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
const require = createRequire('/opt/qianbian-visual/package.json');
const { chromium } = require('playwright');
const root = await mkdtemp(path.join(tmpdir(), 'qianbian-visual-'));
const process = spawn(path.resolve('target/release/qianbian'), ['--root', root, '--listen', '127.0.0.1:19610'], {stdio: ['ignore', 'ignore', 'pipe']});
let browser;
const errors = [];
const consoleErrors = [];
let verified = false;
try {
  let token;
  for (let i = 0; i < 100; i++) {
    try { token = (await readFile(path.join(root, 'data/config/admin-token.txt'), 'utf8')).trim(); await fetch('http://127.0.0.1:19610/api/v1/status'); break; }
    catch { await new Promise(resolve => setTimeout(resolve, 100)); }
  }
  if (!token) throw new Error('Visual verification backend did not start');
  browser = await chromium.launch({headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']});
  const context = await browser.newContext({viewport: {width: 1440, height: 960}, colorScheme: 'dark', locale: 'zh-CN'});
  await context.request.post('http://127.0.0.1:19610/api/v1/login', {data: {token}});
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') consoleErrors.push(message.text()); });
  page.on('requestfailed', request => consoleErrors.push(`${request.url()} ${request.failure()?.errorText}`));
  await page.goto('http://127.0.0.1:19610/', {waitUntil: 'domcontentloaded'});
  try { await page.locator('flutter-view, flt-glass-pane').first().waitFor({state: 'attached', timeout: 30000}); }
  catch (error) { errors.push(`Flutter root not attached: ${error.message}`); }
  await page.waitForTimeout(3000);
  await mkdir('dist', {recursive: true});
  await page.screenshot({path: 'dist/ui-desktop.png'});
  await page.setViewportSize({width: 700, height: 900});
  await page.waitForTimeout(500);
  await page.screenshot({path: 'dist/ui-compact.png'});
  await writeFile('dist/ui-dom.html', await page.content());
  verified = errors.length === 0;
  console.log(`Headless UI screenshots captured; verified=${verified}`);
} catch (error) {
  errors.push(error.message);
} finally {
  await mkdir('dist', {recursive: true});
  await writeFile('dist/ui-runtime-errors.json', JSON.stringify({verified, errors, consoleErrors}, null, 2));
  await browser?.close();
  process.kill('SIGTERM');
}
