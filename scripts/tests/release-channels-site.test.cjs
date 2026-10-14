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
