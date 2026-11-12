import { useEffect, useRef, useSyncExternalStore } from "react";

import { useInstances, useSystem } from "@/lib/queries";

/** How many samples a graph shows: three minutes at one every three seconds. */
export const SAMPLES = 60;

export interface Sample {
  cpu: number;
  /** Share of the host's memory in use, 0 to 100. */
  memory: number;
  /** Bytes each running instance holds. */
  instances: Record<string, number>;
  /** Players on each instance. */
  players: Record<string, number>;
}

let samples: Sample[] = [];
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** What the panel has measured since it was opened, oldest first. */
export function useSamples(): Sample[] {
  return useSyncExternalStore(subscribe, () => samples);
}

/**
 * Records one sample per answer from the daemon. Mounted once for the whole
 * panel, so a graph already has its past when a page opens.
 */
export function useSampler() {
  const system = useSystem();
  const instances = useInstances();
  const list = useRef(instances.data);
  list.current = instances.data;

  const data = system.data;
  const stamp = system.dataUpdatedAt;
  useEffect(() => {
    if (!data) return;
    const sample: Sample = {
      cpu: data.cpu_percent,
      memory: data.memory_total > 0 ? (data.memory_used / data.memory_total) * 100 : 0,
      instances: Object.fromEntries(data.instances.map((entry) => [entry.id, entry.memory_bytes])),
      players: Object.fromEntries(
        (list.current ?? []).map((instance) => [instance.id, instance.runtime.players.length]),
      ),
    };
    samples = [...samples, sample].slice(-SAMPLES);
    for (const listener of listeners) listener();
  }, [data, stamp]);
}
