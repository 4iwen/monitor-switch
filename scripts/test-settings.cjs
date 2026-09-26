const assert = require('node:assert/strict');
const { mkdirSync } = require('node:fs');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || undefined });
  try {
    const page = await browser.newPage({ viewport: { width: 660, height: 500 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      window.messages = [];
      window.ipc = { postMessage: text => window.messages.push(JSON.parse(text)) };
    });
    await page.goto(pathToFileURL(process.argv[2]).href);
    const shortcut = page.locator('[data-shortcut]').first();
    await shortcut.focus();
    await page.keyboard.press('Control+Alt+Digit1');
    assert.equal(await shortcut.getAttribute('data-binding'), 'Control+Alt+Digit1');
    await page.locator('#save-button').click();
    const saved = await page.evaluate(() => window.messages.find(message => message.type === 'saveLabels'));
    assert.equal(saved.shortcuts[0].shortcut, 'Control+Alt+Digit1');
    assert.equal(saved.items.length, 0);
    await page.evaluate(saved => {
      window.__STATE__.shortcuts = saved.shortcuts;
      window.monitorSwitchSaved();
      window.monitorSwitchRender(window.__STATE__);
    }, saved);
    assert.equal(await page.locator('#save-button').isDisabled(), true);

    await page.locator('[data-value]').first().fill('Gaming PC');
    await page.evaluate(() => {
      window.__STATE__.monitors[0].available_inputs[0].reported_supported = true;
      window.monitorSwitchRender(window.__STATE__);
    });
    assert.equal(await page.locator('[data-value]').first().inputValue(), 'Gaming PC');
    assert.equal(await shortcut.getAttribute('data-binding'), 'Control+Alt+Digit1');
    await page.getByRole('button', { name: '取消', exact: true }).click();
    assert.equal(await page.locator('#discard-dialog').isVisible(), true);
    await page.getByRole('button', { name: '放弃修改', exact: true }).click();
    assert.equal(await page.locator('#save-button').isDisabled(), true);

    await page.locator('.clear-shortcut').first().click();
    assert.equal(await shortcut.getAttribute('data-binding'), '');
    assert.equal(await page.locator('#save-button').isEnabled(), true);
    await page.locator('#save-button').click();
    const cleared = await page.evaluate(() => window.messages.filter(message => message.type === 'saveLabels').at(-1));
    assert.deepEqual(cleared.shortcuts, []);
    await page.evaluate(() => window.monitorSwitchSaveError('快捷键已被占用'));
    assert.equal(await page.locator('#save-button').isEnabled(), true);
    assert.equal(await page.locator('#notice').innerText(), '快捷键已被占用');

    const output = process.argv[3] || '/tmp/monitor-switch-ui';
    mkdirSync(output, { recursive: true });
    for (const platform of ['macos', 'windows']) {
      await page.evaluate(platform => document.body.className = `${platform} settings-body`, platform);
      for (const theme of ['light', 'dark']) {
        await page.emulateMedia({ colorScheme: theme });
        await page.setViewportSize({ width: 660, height: 500 });
        await page.screenshot({ path: `${output}/${platform}-${theme}.png` });
        await page.setViewportSize({ width: 560, height: 400 });
        assert.equal(await page.evaluate(() => document.querySelector('.settings-scroll').scrollWidth <= document.querySelector('.settings-scroll').clientWidth), true);
      }
    }
    assert.deepEqual(errors, []);
    console.log('Settings interaction and layout checks passed');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
