const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const source = fs.readFileSync(path.join(__dirname, '../../site/app.js'), 'utf8');
const betaSource = source.slice(source.indexOf('async function latestBetaRelease()'), source.indexOf('async function download('));

test('Beta downloads select the highest compatible version, excluding drafts and PR builds', async () => {
  const context = vm.createContext({ gh: async () => [
    { tag_name: 'v0.2.9', prerelease: true },
    { tag_name: 'v0.2.12', prerelease: true },
    { tag_name: 'v0.2.20', prerelease: true, draft: true },
    { tag_name: 'v0.2.10', prerelease: false },
    { tag_name: 'v0.2.50-pr123', prerelease: true },
  ] });
  const release = await vm.runInContext(betaSource + '\nlatestBetaRelease()', context);
  assert.equal(release.tag_name, 'v0.2.12');
});

test('Beta subscribers receive the promoted Stable when only Stable is available', async () => {
  const context = vm.createContext({ gh: async () => [{ tag_name: 'v0.2.48', prerelease: false }] });
  const release = await vm.runInContext(betaSource + '\nlatestBetaRelease()', context);
  assert.equal(release.tag_name, 'v0.2.48');
});

test('A saved channel remains saved when update-status refresh fails, without enabling checks', async () => {
  const data = fs.readFileSync(path.join(__dirname, '../../launcher/ui/js/data.js'), 'utf8');
  const saveSource = data.slice(data.indexOf('export async function saveSettings('), data.indexOf('/** The trip of the tour'))
    .replace('export async function', 'async function');
  const S = { settings: { update_channel: 'stable', update_check: false } };
  const calls = [];
  const toasts = [];
  const context = vm.createContext({
    S, set: values => Object.assign(S, values), toast: (...args) => toasts.push(args), t: text => text,
    call: async command => {
      calls.push(command);
      if (command === 'settings_patch') return { update_channel: 'beta', update_check: false };
      throw new Error('Update status is unavailable');
    },
  });
  const saved = await vm.runInContext('let settingsSaved = Promise.resolve(); let settingsSeq = 0;\n' + saveSource + '\nsaveSettings({ update_channel: "beta" })', context);
  assert.equal(saved, true);
  assert.equal(S.settings.update_channel, 'beta');
  assert.equal(S.settings.update_check, false);
  assert.deepEqual(calls, ['settings_patch', 'update_status']);
  assert.equal(toasts.length, 0);
});
