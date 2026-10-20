/** A question or permission request from an AI agent, held by the backend until it is answered. */
export interface AskOption {
  label: string;
  description?: string;
}

export interface AskQuestion {
  header?: string;
  question: string;
  multiSelect?: boolean;
  options: AskOption[];
}

export interface Ask {
  id: number;
  kind: 'question' | 'permission';
  agent: string;
  project: string | null;
  session: string;
  tool: string;
  questions: AskQuestion[];
  summary: string;
  input: unknown;
  at: number;
}

export interface HooksStatus {
  installed: boolean;
  port: number;
  settingsPath: string;
  otherHooksKept: boolean;
}

/** The answer text for one question: the picked labels joined, or the typed reply when there is one. */
export function answerFor(q: AskQuestion, picked: string[], typed: string): string {
  const text = typed.trim();
  if (text) return picked.length && q.multiSelect ? [...picked, text].join(', ') : text;
  return picked.join(', ');
}
