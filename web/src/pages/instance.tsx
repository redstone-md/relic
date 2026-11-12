import { toast } from "sonner";
import { Link } from "wouter";

import { Empty, ErrorNote, Loading } from "@/components/common";
import { Icon, type IconName } from "@/components/icon";
import { PowerControls } from "@/components/power";
import { StatusLabel } from "@/components/status";
import { Button } from "@/components/ui/button";
import { LOADER_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { useInstances, useSettings } from "@/lib/queries";
import type { Instance } from "@/lib/types";
import { cn } from "@/lib/utils";
import { AccessTab } from "@/pages/instance/access";
import { BackupsTab } from "@/pages/instance/backups";
import { ConsoleTab } from "@/pages/instance/console";
import { FilesTab } from "@/pages/instance/files";
import { ModsTab } from "@/pages/instance/mods";
import { OptionsTab } from "@/pages/instance/options";
import { PackTab } from "@/pages/instance/pack";
import { InstanceSettingsTab } from "@/pages/instance/settings";
import { WorldsTab } from "@/pages/instance/worlds";

const TABS: { id: string; label: string; icon: IconName }[] = [
  { id: "console", label: "Console", icon: "console" },
  { id: "players", label: "Players", icon: "users" },
  { id: "options", label: "Game rules", icon: "sliders" },
  { id: "worlds", label: "Worlds", icon: "earth" },
  { id: "files", label: "Files", icon: "files" },
  { id: "pack", label: "Modpack", icon: "box" },
  { id: "mods", label: "Catalogue", icon: "mods" },
  { id: "backups", label: "Backups", icon: "backup" },
  { id: "settings", label: "Settings", icon: "settings" },
];

/** The address players type, as a button that copies it. */
function Address({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const settings = useSettings();
  const address = `${settings.data?.public_host || window.location.hostname}:${instance.port}`;

  const copy = () => {
    void navigator.clipboard
      .writeText(address)
      .then(() => toast.success(t("Address copied")))
      .catch(() => toast.error(t("The browser refused to copy the address")));
  };

  return (
    <button
      type="button"
      onClick={copy}
      data-icon-trigger
      title={t("Copy the address")}
      className="flex h-8 min-w-0 cursor-pointer items-center gap-2 border px-2.5 font-mono text-sm outline-none transition-colors hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="truncate">{address}</span>
      <Icon name="copy" className="shrink-0 text-muted-foreground" />
      <span className="sr-only">{t("Copy the address")}</span>
    </button>
  );
}

export function InstancePage({ id, tab }: { id: string; tab?: string }) {
  const { t } = useI18n();
  const instances = useInstances();

  if (instances.isPending) return <Loading />;
  if (instances.error) return <ErrorNote error={instances.error} />;
  const instance = instances.data.find((candidate) => candidate.id === id);
  if (!instance) {
    return (
      <Empty icon="server" title={t("No such instance")}>
        <p>{t("Nothing is registered under “{id}”.", { id })}</p>
        <Button asChild variant="outline" className="mt-4">
          <Link href="/">
            <Icon name="back" />
            {t("Back to overview")}
          </Link>
        </Button>
      </Empty>
    );
  }

  const active = TABS.some((entry) => entry.id === tab) ? (tab as string) : "console";
  const { runtime } = instance;

  return (
    <>
      {/* Stays under the panel's own bar, so the server can be stopped from any depth of any page. */}
      <div className="sticky top-14 z-20 -mt-6 bg-background shadow-[0_var(--frame)_0_var(--canvas)]">
        <header className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 px-5 pt-4 pb-3">
          <div className="min-w-0">
            <h1 className="truncate font-pixel text-base leading-7">{instance.name}</h1>
            <p className="truncate text-sm text-muted-foreground">
              {LOADER_NAMES[instance.loader.type]}
              {instance.loader.version && ` ${instance.loader.version}`}, Minecraft{" "}
              {instance.minecraft_version}, Java {instance.java_major}
            </p>
          </div>
          <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
            <StatusLabel status={runtime.status} />
            <Address instance={instance} />
            <PowerControls instance={instance} />
          </div>
        </header>

        <nav
          className="flex overflow-x-auto border-t"
          aria-label={t("{name} sections", { name: instance.name })}
        >
          {TABS.map((entry) => (
            <Link
              key={entry.id}
              href={entry.id === "console" ? `/instances/${id}` : `/instances/${id}/${entry.id}`}
              aria-current={entry.id === active ? "page" : undefined}
              data-icon-trigger
              className={cn(
                "flex h-11 shrink-0 items-center gap-2 px-4 text-sm font-medium whitespace-nowrap text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                entry.id === active && "text-foreground shadow-[inset_0_-2px_0_var(--primary)]",
              )}
            >
              <Icon name={entry.icon} />
              {t(entry.label)}
            </Link>
          ))}
        </nav>
      </div>

      {runtime.status === "crashed" && runtime.last_error && <ErrorNote error={runtime.last_error} />}

      {active === "console" && <ConsoleTab instance={instance} />}
      {active === "players" && <AccessTab instance={instance} />}
      {active === "options" && <OptionsTab instance={instance} />}
      {active === "worlds" && <WorldsTab instance={instance} />}
      {active === "files" && <FilesTab instance={instance} />}
      {active === "pack" && <PackTab instance={instance} />}
      {active === "mods" && <ModsTab instance={instance} />}
      {active === "backups" && <BackupsTab instance={instance} />}
      {active === "settings" && <InstanceSettingsTab instance={instance} />}
    </>
  );
}
