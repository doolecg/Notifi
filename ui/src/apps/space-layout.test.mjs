import assert from 'node:assert/strict';
import { test } from 'node:test';
import { layout, squarify } from './space-layout.ts';

const node = (name, size, children = []) => ({ name, path: name, dir: children.length > 0, other: false, size, files: 1, kind: 'other', tag: null, mtime: 0, children });

test('areas follow sizes and fill the rectangle', () => {
  const items = [node('a', 600), node('b', 300), node('c', 100)];
  const boxes = squarify(items, 0, 0, 200, 100);
  const area = (b) => b.w * b.h;
  assert.equal(Math.round(boxes.reduce((s, b) => s + area(b), 0)), 20000);
  assert.equal(Math.round(area(boxes[0])), 12000);
  assert.equal(Math.round(area(boxes[2])), 2000);
});

test('boxes stay inside the area', () => {
  const items = Array.from({ length: 30 }, (_, i) => node('n' + i, 100 - i));
  for (const b of squarify(items, 10, 20, 300, 200)) {
    assert.ok(b.x >= 10 - 1e-6 && b.y >= 20 - 1e-6 && b.x + b.w <= 310 + 1e-6 && b.y + b.h <= 220 + 1e-6);
  }
});

test('a big folder becomes a frame with its files inside', () => {
  const root = node('root', 100, [node('dir', 100, [node('f1', 60), node('f2', 40)])]);
  root.dir = true;
  const boxes = layout(root, 400, 300);
  const dir = boxes.find((b) => b.node.name === 'dir');
  const f1 = boxes.find((b) => b.node.name === 'f1');
  assert.ok(dir.frame);
  assert.equal(f1.level, 1);
  assert.ok(f1.y >= dir.y + 16);
});
