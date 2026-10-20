import { invoke } from '@tauri-apps/api/core';

export interface Pkg {
  manager: string;
  id: string;
  name: string;
  version: string;
  available: string | null;
  origin: string;
}

export interface SourceError {
  source: string;
  message: string;
}

export interface Listing {
  items: Pkg[];
  errors: SourceError[];
}

export interface ManagerInfo {
  id: string;
  label: string;
  found: boolean;
}

export interface UpdateItem {
  manager: string;
  id: string;
  name: string;
  installed: string;
  available: string;
  origin: string;
}

export interface UpdatesResult {
  items: UpdateItem[];
  notCompared: UpdateItem[];
  errors: SourceError[];
  pending: string[];
  checkedAt: number;
}

export interface Rules {
  skipped: { manager: string; id: string; name: string; version: string }[];
  ignored: { manager: string; id: string; name: string }[];
}

export interface Target {
  manager: string;
  id: string;
  name: string;
  origin?: string;
  expectedVersion?: string;
}

export interface Socket {
  proto: 'TCP' | 'UDP';
  address: string;
  port: number;
  pid: number | null;
  process: string;
  user: string;
  exposed: boolean;
  allInterfaces: boolean;
  startTime: number;
  protected: string | null;
}

export interface Proc {
  pid: number;
  name: string;
  cpu: number;
  memory: number;
  startTime: number;
  parent: number | null;
  exe: string | null;
  user: string;
  protected: string | null;
  canRestart: boolean;
}

export const MANAGER_LABEL: Record<string, string> = { winget: 'winget', scoop: 'Scoop', pkcon: 'PackageKit', flatpak: 'Flatpak' };

export const runJobs = (action: 'install' | 'update' | 'remove', targets: Target[]) => invoke<number[]>('apps_job_run', { action, targets });

export const message = (e: unknown) => (typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));

export function bytes(n: number) {
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(n < 10 * 1024 ** 2 ? 1 : 0)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}
