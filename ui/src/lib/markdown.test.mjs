// node --experimental-strip-types ui/src/lib/markdown.test.mjs
import assert from 'node:assert/strict';
import { markdown } from './markdown.ts';

assert.equal(markdown('# Notifi 0.3.0\n\n## New\n\n- One **bold** thing\n- `code`\n\n---\n\nText here\nand more'),
  '<h1>Notifi 0.3.0</h1><h2>New</h2><ul><li>One <strong>bold</strong> thing</li><li><code>code</code></li></ul><hr><p>Text here and more</p>');
assert.equal(markdown('<img src=x onerror=alert(1)>'), '<p>&lt;img src=x onerror=alert(1)&gt;</p>');
assert.equal(markdown('[site](https://example.org) [bad](javascript:alert(1))'),
  '<p><a href="https://example.org" target="_blank" rel="noreferrer">site</a> [bad](javascript:alert(1))</p>');
assert.equal(markdown('[x](https://a.test/" onmouseover="y)'), '<p>[x](https://a.test/&quot; onmouseover=&quot;y)</p>');
console.log('markdown ok');
