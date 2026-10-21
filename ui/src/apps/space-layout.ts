export interface SpaceNode {
  name: string;
  path: string;
  dir: boolean;
  other: boolean;
  size: number;
  files: number;
  kind: string;
  tag: string | null;
  mtime: number;
  children: SpaceNode[];
}

export interface Box {
  node: SpaceNode;
  x: number;
  y: number;
  w: number;
  h: number;
  level: number;
  /** A folder drawn as a frame with its contents inside. */
  frame: boolean;
}

const HEAD = 16;
const PAD = 2;
const MIN = 3;

function worst(areas: number[], side: number): number {
  const s = areas.reduce((a, b) => a + b, 0);
  const max = Math.max(...areas);
  const min = Math.min(...areas);
  return Math.max((side * side * max) / (s * s), (s * s) / (side * side * min));
}

/** Squarified treemap of `items` (already sorted, largest first) inside the rectangle. */
export function squarify(items: SpaceNode[], x: number, y: number, w: number, h: number): { node: SpaceNode; x: number; y: number; w: number; h: number }[] {
  const out: { node: SpaceNode; x: number; y: number; w: number; h: number }[] = [];
  let left = items.filter((n) => n.size > 0);
  let total = left.reduce((a, n) => a + n.size, 0);
  if (!total || w <= 0 || h <= 0) return out;
  while (left.length) {
    const side = Math.min(w, h);
    const scale = (w * h) / total;
    let n = 1;
    let areas = [left[0].size * scale];
    while (n < left.length) {
      const next = [...areas, left[n].size * scale];
      if (worst(next, side) > worst(areas, side)) break;
      areas = next;
      n++;
    }
    const row = left.slice(0, n);
    const sum = areas.reduce((a, b) => a + b, 0);
    if (w >= h) {
      const rw = Math.min(w, sum / h);
      let cy = y;
      row.forEach((node, i) => {
        const ch = i === row.length - 1 ? y + h - cy : areas[i] / rw;
        out.push({ node, x, y: cy, w: rw, h: ch });
        cy += ch;
      });
      x += rw;
      w -= rw;
    } else {
      const rh = Math.min(h, sum / w);
      let cx = x;
      row.forEach((node, i) => {
        const cw = i === row.length - 1 ? x + w - cx : areas[i] / rh;
        out.push({ node, x: cx, y, w: cw, h: rh });
        cx += cw;
      });
      y += rh;
      h -= rh;
    }
    total -= row.reduce((a, r) => a + r.size, 0);
    left = left.slice(n);
  }
  return out;
}

/** Flat list of nested boxes for a view root drawn into a width by height area. */
export function layout(root: SpaceNode | null, w: number, h: number): Box[] {
  if (!root) return [];
  const boxes: Box[] = [];
  const fill = (node: SpaceNode, x: number, y: number, bw: number, bh: number, level: number) => {
    for (const c of squarify(node.children, x, y, bw, bh)) {
      if (c.w < MIN || c.h < MIN) continue;
      const frame = c.node.dir && c.node.children.length > 0 && c.w > 44 && c.h > HEAD + 14;
      boxes.push({ node: c.node, x: c.x, y: c.y, w: c.w, h: c.h, level, frame });
      if (frame) fill(c.node, c.x + PAD, c.y + HEAD, c.w - PAD * 2, c.h - HEAD - PAD, level + 1);
    }
  };
  fill(root, 0, 0, w, h, 0);
  return boxes;
}
