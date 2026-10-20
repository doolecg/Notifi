export interface Row {
  id: string;
  kind: 'app' | 'window' | 'file' | 'clipboard' | 'calc' | 'unit' | 'emoji' | 'script';
  title: string;
  subtitle: string;
  icon: string;
}

export interface Outcome {
  copy: string | null;
  complete: string | null;
  closed: boolean;
}

export interface OpenPayload {
  compact: boolean;
  noSuggestions: boolean;
  query: string;
}

const PATHS: Record<string, string> = {
  window: 'M4 5h16v14H4zM4 9h16',
  file: 'M7 3h7l4 4v14H7zM14 3v4h4',
  folder: 'M3 6h6l2 2h10v11H3z',
  clipboard: 'M9 4h6v3H9zM7 5H6v16h12V5h-1',
  calc: 'M5 4h14v16H5zM8 8h8M8 12h2M12 12h2M16 12h0M8 16h2M12 16h2M16 16h0',
  ruler: 'M3 9l12-6 6 12-12 6zM8 8l2 4M11 6l1 2M5 13l2 4',
  script: 'M4 5h16v14H4zM8 10l3 2-3 2M13 15h4',
};

export const glyphPath = (name: string) => PATHS[name];
export const isGlyph = (name: string) => name in PATHS;
