import assert from 'node:assert/strict';
import { test } from 'node:test';
import { groupRows } from './group.ts';

const pkg = (id, manager = 'scoop') => ({ manager, id });
const ids = (rows) => rows.map((r) => `${'-'.repeat(r.depth)}${r.item.id}`);

test('a dependency sits under the package that needs it', () => {
  const rows = groupRows([pkg('7zip'), pkg('git'), pkg('mytool')], [{ manager: 'scoop', id: 'mytool', needs: ['7zip'] }]);
  assert.deepEqual(ids(rows), ['git', 'mytool', '-7zip']);
  assert.equal(rows[2].parent.id, 'mytool');
});

test('without dependency data the list is unchanged', () => {
  const items = [pkg('b'), pkg('a')];
  assert.deepEqual(ids(groupRows(items, [])), ['b', 'a']);
});

test('a dependency needed by two packages goes under the first one listed', () => {
  const rows = groupRows([pkg('a'), pkg('b'), pkg('lib')], [
    { manager: 'scoop', id: 'b', needs: ['lib'] },
    { manager: 'scoop', id: 'a', needs: ['lib'] },
  ]);
  assert.deepEqual(ids(rows), ['a', '-lib', 'b']);
});

test('dependencies nest more than one level', () => {
  const rows = groupRows([pkg('app'), pkg('mid'), pkg('base')], [
    { manager: 'scoop', id: 'app', needs: ['mid'] },
    { manager: 'scoop', id: 'mid', needs: ['base'] },
  ]);
  assert.deepEqual(ids(rows), ['app', '-mid', '--base']);
});

test('packages that need each other stay in the list', () => {
  const rows = groupRows([pkg('a'), pkg('b'), pkg('c')], [
    { manager: 'scoop', id: 'a', needs: ['b'] },
    { manager: 'scoop', id: 'b', needs: ['a'] },
    { manager: 'scoop', id: 'c', needs: ['b'] },
  ]);
  assert.equal(rows.length, 3);
  assert.deepEqual(rows.map((r) => r.item.id).sort(), ['a', 'b', 'c']);
});

test('needs from another manager are ignored and missing packages are skipped', () => {
  const rows = groupRows([pkg('a'), pkg('b', 'winget')], [{ manager: 'scoop', id: 'a', needs: ['b', 'gone'] }]);
  assert.deepEqual(ids(rows), ['a', 'b']);
});
