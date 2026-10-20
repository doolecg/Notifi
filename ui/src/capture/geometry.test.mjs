import assert from 'node:assert/strict';
import { test } from 'node:test';
import { bounds, eraseHits, fitImage, hasPad, hit, innerBox, moved, noPad, onHandle, padded, resizedImage, toImage, topHit } from './geometry.ts';

const rect = { id: 1, t: 'rect', size: 4, a: { x: 10, y: 10 }, b: { x: 110, y: 60 } };
const line = { id: 2, t: 'line', size: 4, a: { x: 0, y: 0 }, b: { x: 100, y: 0 } };
const image = { id: 3, t: 'image', size: 0, a: { x: 20, y: 20 }, w: 80, h: 40 };

test('a rectangle is hit on its outline, not its middle', () => {
  assert.ok(hit(rect, { x: 60, y: 10 }, 3));
  assert.ok(hit(rect, { x: 110, y: 35 }, 3));
  assert.ok(!hit(rect, { x: 60, y: 35 }, 3));
  assert.ok(!hit(rect, { x: 200, y: 200 }, 3));
});

test('a line is hit near the segment only', () => {
  assert.ok(hit(line, { x: 50, y: 4 }, 3));
  assert.ok(!hit(line, { x: 50, y: 20 }, 3));
  assert.ok(!hit(line, { x: 150, y: 0 }, 3));
});

test('an ellipse is hit on its edge', () => {
  const e = { id: 4, t: 'ellipse', size: 4, a: { x: 0, y: 0 }, b: { x: 100, y: 60 } };
  assert.ok(hit(e, { x: 100, y: 30 }, 3));
  assert.ok(hit(e, { x: 50, y: 0 }, 3));
  assert.ok(!hit(e, { x: 50, y: 30 }, 3));
});

test('images, text and badges are hit inside their box', () => {
  assert.ok(hit(image, { x: 60, y: 40 }, 0));
  assert.ok(!hit(image, { x: 130, y: 40 }, 0));
  assert.ok(hit({ id: 5, t: 'step', size: 12, at: { x: 50, y: 50 } }, { x: 58, y: 50 }, 0));
  const text = { id: 6, t: 'text', size: 20, text: 'abcd', at: { x: 10, y: 10 } };
  assert.deepEqual(bounds(text), { x: 10, y: 10, w: 4 * 20 * 0.58, h: 24 });
});

test('a crop is never selected or erased', () => {
  const crop = { id: 7, t: 'crop', size: 1, a: { x: 0, y: 0 }, b: { x: 50, y: 50 } };
  assert.ok(!hit(crop, { x: 25, y: 25 }, 5));
  assert.equal(topHit([crop], { x: 25, y: 25 }, 5), -1);
});

test('the topmost item under the point wins', () => {
  const list = [rect, { ...rect, id: 8 }];
  assert.equal(topHit(list, { x: 60, y: 10 }, 3), 1);
  assert.equal(topHit(list, { x: 300, y: 300 }, 3), -1);
});

test('the eraser takes every item under it', () => {
  const list = [rect, line, image];
  assert.deepEqual(eraseHits(list, { x: 50, y: 2 }, 3), [2]);
  assert.deepEqual(eraseHits(list, { x: 60, y: 10 }, 3), [1]);
  assert.deepEqual(eraseHits(list, { x: 500, y: 500 }, 3), []);
});

test('moving shifts every point and leaves the rest alone', () => {
  const pen = { id: 9, t: 'pen', size: 3, pts: [{ x: 1, y: 1 }, { x: 5, y: 5 }] };
  assert.deepEqual(moved(pen, 10, -1).pts, [{ x: 11, y: 0 }, { x: 15, y: 4 }]);
  const m = moved(rect, 5, 5);
  assert.deepEqual(m.a, { x: 15, y: 15 });
  assert.deepEqual(m.b, { x: 115, y: 65 });
  assert.equal(m.at, undefined);
  assert.deepEqual(rect.a, { x: 10, y: 10 });
});

test('an image resizes from its corner and keeps its shape', () => {
  assert.ok(onHandle(image, { x: 100, y: 60 }, 5));
  assert.ok(!onHandle(image, { x: 20, y: 20 }, 5));
  const r = resizedImage(image, { x: 180, y: 999 });
  assert.equal(r.w, 160);
  assert.equal(r.h, 80);
  assert.equal(resizedImage(image, { x: 21, y: 0 }).w, 8);
});

test('dropped images shrink to half the capture at most', () => {
  assert.deepEqual(fitImage(100, 50, { w: 800, h: 600 }), { w: 100, h: 50 });
  assert.deepEqual(fitImage(2000, 1000, { w: 800, h: 600 }), { w: 400, h: 200 });
});

test('padding grows the canvas and moves the capture', () => {
  const size = { w: 200, h: 100 };
  const pad = { t: 10, r: 20, b: 30, l: 40 };
  assert.ok(hasPad(pad));
  assert.ok(!hasPad(noPad()));
  assert.deepEqual(padded(size, pad), { w: 260, h: 140 });
  assert.deepEqual(innerBox(size, pad), { x: 40, y: 10, w: 200, h: 100 });
  assert.deepEqual(toImage({ x: 40, y: 10 }, pad), { x: 0, y: 0 });
  assert.deepEqual(toImage({ x: 0, y: 0 }, pad), { x: -40, y: -10 });
  assert.deepEqual(padded(size, noPad()), size);
});
