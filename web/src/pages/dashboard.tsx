import { Link } from "wouter";

import { Empty, ErrorNote, Loading, PageHeader } from "@/components/common";
import { Bars, Meter } from "@/components/graph";
import { Icon } from "@/components/icon";
import { PowerControls } from "@/components/power";
import { StatusLabel } from "@/components/status";
import { Button } from "@/components/ui/button";
import { bytes, LOADER_NAMES, since } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { useSamples } from "@/lib/history";
import { useInstances, useSettings, useSystem, useUsers } from "@/lib/queries";
import type { Instance } from "@/lib/types";
import { cn } from "@/lib/utils";

const STRIPE: Record<Instance["runtime"]["status"], string> = {
  running: "bg-ok",
  starting: "bg-warn",
  installing: "bg-warn",
  stopping: "bg-warn",
  crashed: "bg-destructive",
  stopped: "bg-muted-foreground/40",
};

function Figure({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 truncate font-mono text-sm tabular">{children}</dd>
    </div>
  );
}

/** One server as a line: what it is, where it listens, how it is doing. */
function InstanceRow({
  instance,
  host,
  memory,
}: {
  instance: Instance;
  host: string;
  memory?: number;
}) {
  const { t } = useI18n();
  const { runtime } = instance;
  const running = runtime.status === "running";
  const href = `/instances/${instance.id}`;
  return (
    <li className="relative flex flex-wrap items-center gap-x-6 gap-y-3 bg-background py-4 pr-4 pl-6 transition-colors hover:bg-card">
      <span aria-hidden className={cn("absolute inset-y-0 left-0 w-1", STRIPE[runtime.status])} />
      <div className="min-w-0 flex-1 basis-56">
        <Link
          href={href}
          className="block truncate font-pixel text-xs leading-6 outline-none after:absolute after:inset-0 focus-visible:underline"
        >
          {instance.name}
        </Link>
        <p className="truncate text-sm text-muted-foreground">
          {LOADER_NAMES[instance.loader.type]} {instance.minecraft_version}
        </p>
        {runtime.status === "crashed" && runtime.last_error && (
          <p className="mt-1 line-clamp-1 text-sm text-destructive">{runtime.last_error}</p>
        )}
      </div>

      <dl className="grid flex-[2] basis-96 grid-cols-4 gap-4">
        <Figure label={t("Address")}>
          {host}:{instance.port}
        </Figure>
        <Figure label={t("Players")}>
          {running ? `${runtime.players.length} / ${runtime.players_max}` : "—"}
        </Figure>
        <Figure label={t("Memory")}>
          {memory ? bytes(memory) : "—"}
          <span className="text-muted-foreground"> / {bytes(instance.memory_mb * 1048576)}</span>
        </Figure>
        <Figure label={t("Uptime")}>
          {running && runtime.started_at ? since(runtime.started_at) : "—"}
        </Figure>
      </dl>

      <div className="relative z-10 flex w-44 shrink-0 items-center justify-end gap-4">
        <StatusLabel status={runtime.status} />
        <PowerControls instance={instance} compact />
      </div>
    </li>
  );
}

export function Dashboard() {
  const instances = useInstances();
  const system = useSystem();
  const users = useUsers();
  const settings = useSettings();
  const samples = useSamples();
  const { t, plural } = useI18n();

  if (instances.isPending) return <Loading />;
  if (instances.error) return <ErrorNote error={instances.error} />;

  const list = instances.data;
  const running = list.filter((instance) => instance.runtime.status === "running");
  const online = new Set(running.flatMap((instance) => instance.runtime.players)).size;
  const host = settings.data?.public_host || window.location.hostname;
  const usage = new Map(system.data?.instances.map((entry) => [entry.id, entry.memory_bytes]));
  const cpu = system.data?.cpu_percent;
  const memoryShare = system.data
    ? (system.data.memory_used / system.data.memory_total) * 100
    : 0;

  return (
    <>
      <PageHeader
        title={settings.data?.server_name || t("Instances")}
        description={
          settings.data?.public_host
            ? t("Players connect to {host}", { host: settings.data.public_host })
            : t("Set a public address in Settings so launchers can find this server.")
        }
        actions={
          <Button asChild>
            <Link href="/instances/new">
              <Icon name="plus" />
              {t("New instance")}
            </Link>
          </Button>
        }
      />

      {list.length > 0 ? (
        <ul className="grid gap-(--frame)">
          {list.map((instance) => (
            <InstanceRow
              key={instance.id}
              instance={instance}
              host={host}
              memory={usage.get(instance.id)}
            />
          ))}
        </ul>
      ) : (
        <Empty icon="server" title={t("No instances yet")}>
          <p>{t("An instance is one Minecraft server with its own modpack, world and port.")}</p>
          <Button asChild className="mt-4">
            <Link href="/instances/new">
              <Icon name="plus" />
              {t("Create the first one")}
            </Link>
          </Button>
        </Empty>
      )}

      {/* The machine under all of them, in one line. */}
      <section
        aria-label={t("Host")}
        className="mt-4 flex flex-wrap items-center gap-x-8 gap-y-4 bg-background px-6 py-4 text-sm"
      >
        <div className="flex min-w-56 flex-1 items-center gap-4">
          <span className="shrink-0 text-muted-foreground">{t("Processor")}</span>
          <span className="w-10 shrink-0 font-mono tabular">
            {cpu === undefined ? "—" : `${cpu.toFixed(0)}%`}
          </span>
          <Bars
            values={samples.map((sample) => sample.cpu)}
            max={100}
            label={t("Processor load, last three minutes")}
            className="h-8 min-w-0 flex-1"
          />
        </div>
        <div className="flex min-w-56 flex-1 items-center gap-4">
          <span className="shrink-0 text-muted-foreground">{t("Memory")}</span>
          <span className="shrink-0 font-mono tabular">
            {system.data
              ? `${bytes(system.data.memory_used)} / ${bytes(system.data.memory_total)}`
              : "—"}
          </span>
          <div className="min-w-0 flex-1">
            <Meter value={memoryShare} label={t("Host memory in use")} />
          </div>
        </div>
        <p className="shrink-0 text-muted-foreground">
          <span className="font-mono text-foreground tabular">{online}</span> {t("online")},{" "}
          <Link href="/players" className="underline-offset-4 hover:underline">
            <span className="font-mono text-foreground tabular">{users.data?.length ?? "—"}</span>{" "}
            {plural(users.data?.length ?? 0, "account", "accounts")}
          </Link>
        </p>
      </section>
    </>
  );
}
