/** What one installed package needs, from the package manager (only Scoop reports this). */
export interface Needs {
  manager: string;
  id: string;
  needs: string[];
}

export interface Placed<T> {
  item: T;
  /** 0 for a top-level row, 1 for something a row above needs, and so on. */
  depth: number;
  /** The package this row sits under. */
  parent: T | null;
}

const key = (x: { manager: string; id: string }) => `${x.manager}|${x.id}`;

/**
 * Puts each package beneath the first package in the list that needs it, so a dependency does not
 * stand alone. Packages nobody in the list needs stay at the top level, in their original order, and
 * each parent is followed by its dependencies. Packages that need each other in a loop stay top level.
 */
export function groupRows<T extends { manager: string; id: string }>(items: T[], needs: Needs[]): Placed<T>[] {
  const byKey = new Map(items.map((it) => [key(it), it]));
  const wants = new Map(needs.map((n) => [`${n.manager}|${n.id}`, n.needs]));
  const parentOf = new Map<string, string>();
  for (const it of items) {
    for (const dep of wants.get(key(it)) ?? []) {
      const k = `${it.manager}|${dep}`;
      if (k !== key(it) && byKey.has(k) && !parentOf.has(k)) parentOf.set(k, key(it));
    }
  }
  for (const it of items) {
    const seen = new Set<string>();
    let at: string | undefined = key(it);
    while (at !== undefined && !seen.has(at)) {
      seen.add(at);
      at = parentOf.get(at);
    }
    if (at !== undefined) parentOf.delete(at);
  }
  const kids = new Map<string, T[]>();
  for (const it of items) {
    const p = parentOf.get(key(it));
    if (p) kids.set(p, [...(kids.get(p) ?? []), it]);
  }
  const out: Placed<T>[] = [];
  const place = (it: T, depth: number, parent: T | null) => {
    out.push({ item: it, depth, parent });
    for (const kid of kids.get(key(it)) ?? []) place(kid, depth + 1, it);
  };
  for (const it of items) if (!parentOf.has(key(it))) place(it, 0, null);
  return out;
}
