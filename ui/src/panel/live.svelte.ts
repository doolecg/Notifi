import { listen } from '@tauri-apps/api/event';

/** Samples kept for history lines (one a second). */
export const HISTORY = 60;

export interface Battery {
  percent: number;
  charging: boolean;
  on_ac: boolean;
  power_watts: number | null;
  seconds_remaining: number | null;
  health_percent: number | null;
  state?: 'charging' | 'discharging' | 'full' | 'not-charging';
  cycle_count?: number | null;
  temperature_celsius?: number | null;
  adapter_watts?: number | null;
  adapter_max_watts?: number | null;
  system_watts?: number | null;
}

export interface Proc {
  name: string;
  cpu: number;
}

export interface SystemSnap {
  cpu: number;
  cores: number[];
  memUsed: number;
  memTotal: number;
  uptime: number;
  top: Proc[];
  temps: { cpu: number | null; gpu: number | null; battery: number | null };
  battery: Battery | null;
}

export interface NetSnap {
  down: number;
  up: number;
  downTotal: number;
  upTotal: number;
  ip: string | null;
}

export interface DiskSnap {
  name: string;
  mount: string;
  used: number;
  total: number;
  removable: boolean;
}

export interface PowerProfile {
  id: string;
  name: string;
  active: boolean;
}

export interface PowerSnap {
  battery: Battery | null;
  top: Proc[];
  profiles: PowerProfile[];
}

interface Snapshot {
  tab: string;
  system: SystemSnap | null;
  network: NetSnap | null;
  storage: DiskSnap[] | null;
  power: PowerSnap | null;
}

const push = (list: number[], v: number) => [...list.slice(-(HISTORY - 1)), v];

class Live {
  system = $state<SystemSnap | null>(null);
  network = $state<NetSnap | null>(null);
  storage = $state<DiskSnap[] | null>(null);
  power = $state<PowerSnap | null>(null);
  hasBattery = $state(false);
  history = $state({ cpu: [] as number[], mem: [] as number[], battery: [] as number[], down: [] as number[], up: [] as number[], watts: [] as number[] });

  async init() {
    await listen<Snapshot>('panel://stats', (e) => this.apply(e.payload));
  }

  private apply(s: Snapshot) {
    const h = this.history;
    if (s.system) {
      this.system = s.system;
      h.cpu = push(h.cpu, s.system.cpu);
      h.mem = push(h.mem, (s.system.memUsed / Math.max(1, s.system.memTotal)) * 100);
    }
    const battery = s.system?.battery ?? s.power?.battery;
    if (battery) h.battery = push(h.battery, battery.percent);
    if (s.power) {
      this.power = s.power;
      const w = s.power.battery?.system_watts ?? s.power.battery?.power_watts;
      if (w != null) h.watts = push(h.watts, w);
    }
    if (s.network) {
      this.network = s.network;
      h.down = push(h.down, s.network.down);
      h.up = push(h.up, s.network.up);
    }
    if (s.storage) this.storage = s.storage;
  }
}

export const live = new Live();
