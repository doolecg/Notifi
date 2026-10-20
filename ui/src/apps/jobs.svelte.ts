import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface JobEvent {
  id: number;
  name: string;
  manager: string;
  action: 'install' | 'update' | 'remove';
  state: 'queued' | 'running' | 'done' | 'failed' | 'cancelled';
  percent: number | null;
  line: string;
}

class JobStore {
  byId = $state<Record<number, JobEvent>>({});
  private started = false;

  async init() {
    if (this.started) return;
    this.started = true;
    await listen<JobEvent>('apps://job', (e) => (this.byId = { ...this.byId, [e.payload.id]: e.payload }));
  }

  /** The newest job for a package, if any. */
  latest(manager: string, name: string) {
    return Object.values(this.byId)
      .filter((j) => j.manager === manager && j.name === name)
      .sort((a, b) => b.id - a.id)[0];
  }

  active(j: JobEvent | undefined) {
    return j?.state === 'queued' || j?.state === 'running';
  }

  get running() {
    return Object.values(this.byId).filter((j) => this.active(j));
  }

  cancel(id: number) {
    return invoke('apps_job_cancel', { id });
  }

  dismiss(id: number) {
    const { [id]: _, ...rest } = this.byId;
    this.byId = rest;
  }
}

export const jobs = new JobStore();
