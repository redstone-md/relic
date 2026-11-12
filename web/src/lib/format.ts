import { language, t } from "@/lib/i18n";
import type { InstanceStatus, LoaderKind, Side } from "@/lib/types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function bytes(value: number): string {
  let unit = 0;
  let amount = value;
  while (amount >= 1024 && unit < UNITS.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || amount >= 100 ? 0 : 1;
  return `${amount.toFixed(digits)} ${UNITS[unit]}`;
}

/** A formatter built once per language, on first use in that language. */
function perLanguage<T>(make: (code: string) => T): () => T {
  const made = new Map<string, T>();
  return () => {
    const code = language();
    let formatter = made.get(code);
    if (!formatter) {
      formatter = make(code);
      made.set(code, formatter);
    }
    return formatter;
  };
}

const date = perLanguage(
  (code) => new Intl.DateTimeFormat(code, { dateStyle: "medium", timeStyle: "short" }),
);
const time = perLanguage(
  (code) =>
    new Intl.DateTimeFormat(code, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }),
);
const count = perLanguage(
  (code) => new Intl.NumberFormat(code, { notation: "compact", maximumFractionDigits: 1 }),
);

/** Unix seconds to a local date and time. */
export function dateTime(seconds: number): string {
  return date().format(seconds * 1000);
}

export function clock(seconds: number): string {
  return time().format(seconds * 1000);
}

export function compact(value: number): string {
  return count().format(value);
}

/** "3h 12m" since a unix timestamp. */
export function since(seconds: number, now = Date.now()): string {
  const span = Math.max(0, Math.floor(now / 1000) - seconds);
  const days = Math.floor(span / 86400);
  const hours = Math.floor((span % 86400) / 3600);
  const minutes = Math.floor((span % 3600) / 60);
  if (days > 0) return t("{days}d {hours}h", { days, hours });
  if (hours > 0) return t("{hours}h {minutes}m", { hours, minutes });
  if (minutes > 0) return t("{minutes}m", { minutes });
  return t("{seconds}s", { seconds: span });
}

/** Product names, the same in every language. */
export const LOADER_NAMES: Record<LoaderKind, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

/** English names, which are dictionary keys: show them through `t`. */
export const STATUS_NAMES: Record<InstanceStatus, string> = {
  stopped: "Stopped",
  installing: "Installing",
  starting: "Starting",
  running: "Running",
  stopping: "Stopping",
  crashed: "Crashed",
};

/** English names, which are dictionary keys: show them through `t`. */
export const SIDE_NAMES: Record<Side, string> = {
  client: "Client",
  server: "Server",
  both: "Both",
  unknown: "Unknown",
};

/** Whether nothing is running and nothing is about to. */
export function halted(status: InstanceStatus): boolean {
  return status === "stopped" || status === "crashed";
}

/** "My Server!" to "my-server": a valid instance id, or empty. */
export function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32)
    .replace(/-+$/, "");
}
