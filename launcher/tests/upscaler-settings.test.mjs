import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

// Check the actual launcher schema, not a copied list of its rows.
const context = vm.createContext({});
const file = new URL('../ui/js/pages/settings-schema.js', import.meta.url);
const module = new vm.SourceTextModule(await readFile(file, 'utf8'), { context, identifier: file.href });
await module.link((specifier) => {
  assert.equal(specifier, '../i18n.js');
  return new vm.SyntheticModule(['t'], function () {
    this.setExport('t', (key) => key);
  }, { context });
});
await module.evaluate();
const graphics = module.namespace.SETTINGS_TABS.find((tab) => tab.id === 'graphics');
assert.ok(graphics, 'graphics tab missing');
const rows = graphics.cols.flatMap((col) => col.rows);
const selector = rows.find((row) => row.key === 'upscaler');
assert.deepEqual(Array.from(selector.options, ([value]) => value), ['default', 'taa', 'fsr1', 'fsr2']);

function shown(mode) {
  const settings = { upscaler: mode, graphics: 'vanilla_plus', msaa: 1 };
  return rows.filter((row) => !row.when || row.when(settings, 'windows')).map((row) => row.key);
}

for (const [mode, quality, sharp1, sharp2] of [
  ['default', false, false, false],
  ['taa', false, false, false],
  ['fsr1', true, true, false],
  ['fsr2', true, false, true],
  // Legacy settings may still be in memory when the launcher is opened.
  ['fsr2wgsl', true, false, true],
]) {
  test(`${mode}: quality and sharpness visibility`, () => {
    const actual = shown(mode);
    assert.equal(actual.includes('fsr_quality'), quality, `${mode}: FSR quality row`);
    assert.equal(actual.includes('fsr_sharpness'), sharp1, `${mode}: FSR 1 sharpening row`);
    assert.equal(actual.includes('fsr2_sharpness'), sharp2, `${mode}: FSR 2 sharpening row`);
  });
}
