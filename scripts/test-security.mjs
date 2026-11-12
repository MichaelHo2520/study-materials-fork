// Local browser fixtures only. Does not contact content providers or use credentials.
import assert from 'node:assert/strict';
import { createServer } from 'vite';
import { chromium } from 'playwright-core';
import { mkdir, writeFile } from 'node:fs/promises';

const server = await createServer({ server: { host: '127.0.0.1', port: 0 } });
let browser;
const reports = [];
try {
  await server.listen();
  const origin = server.resolvedUrls.local[0];
  browser = await chromium.launch({ headless: true, executablePath: process.env.BROWSER_EXECUTABLE || undefined });
  for (const platform of ['Windows NT 10.0; Win64; x64', 'Macintosh; Intel Mac OS X 10_15_7']) {
    const context = await browser.newContext({ userAgent: `Mozilla/5.0 (${platform}) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36` });
    const page = await context.newPage();
    const external = [];
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      if (url.origin === new URL(origin).origin) return route.continue();
      // Sanitizer fixtures intentionally produce native image-proxy URLs.
      if (['http://hg.localhost', 'https://hg.localhost'].includes(url.origin) ||
          (url.protocol === 'hg:' && url.hostname === 'localhost')) {
        return route.fulfill({ status: 204, body: '' });
      }
      external.push(url.origin); return route.abort();
    });
    for (const fixture of ['html-security', 'security-settings']) {
      await page.goto(`${origin}tests/${fixture}.html`);
      await page.waitForFunction(() => {
        try { return typeof JSON.parse(document.querySelector('#results').textContent).failed === 'number'; }
        catch { return false; }
      });
      const report = JSON.parse(await page.locator('#results').textContent());
      reports.push({ platform, fixture, ...report });
      assert.equal(report.failed, 0, JSON.stringify(report));
      assert(report.passed > 0);
    }
    assert.deepEqual(errors, []);
    assert.deepEqual(external, []);
    await context.close();
  }
} finally {
  await browser?.close();
  await server.close();
  await mkdir('test-results', { recursive: true });
  await writeFile('test-results/security.json', JSON.stringify(reports, null, 2));
}
console.log(JSON.stringify(reports.map(({ platform, fixture, passed, failed }) => ({ platform, fixture, passed, failed })), null, 2));
