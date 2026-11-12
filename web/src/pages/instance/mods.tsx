import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useRef, useState } from "react";
import { toast } from "sonner";
import { Link } from "wouter";

import { ActionButton, Empty, ErrorNote, Help, Loading, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, message } from "@/lib/api";
import { bytes, compact, LOADER_NAMES, SIDE_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useSettings } from "@/lib/queries";
import type {
  Dependency,
  Installed,
  Instance,
  ModVersion,
  MrpackReport,
  Provider,
  SearchHit,
} from "@/lib/types";
import { PackCatalogue } from "@/pages/instance/packs";

const PROVIDER_NAMES: Record<Provider, string> = {
  modrinth: "Modrinth",
  curseforge: "CurseForge",
};

function VersionPicker({
  instance,
  hit,
  onClose,
  onPick,
}: {
  instance: Instance;
  hit: SearchHit;
  onClose: () => void;
  onPick: (version: ModVersion) => void;
}) {
  const { t } = useI18n();
  const versions = useQuery({
    queryKey: ["mod-versions", instance.id, hit.provider, hit.id],
    queryFn: () =>
      api<ModVersion[]>("GET", `/admin/instances/${instance.id}/mods/versions`, {
        query: { provider: hit.provider, project: hit.id },
      }),
  });
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{hit.title}</DialogTitle>
          <DialogDescription>
            {t("Builds for {loader} {version}, newest first.", {
              loader: LOADER_NAMES[instance.loader.type],
              version: instance.minecraft_version,
            })}
          </DialogDescription>
        </DialogHeader>
        {versions.isPending ? (
          <Loading />
        ) : versions.error ? (
          <ErrorNote error={versions.error} />
        ) : versions.data.length === 0 ? (
          <p className="py-6 text-center text-sm text-muted-foreground">
            {t("No build of this mod fits the instance.")}
          </p>
        ) : (
          <ul className="-mx-2 max-h-80 overflow-y-auto">
            {versions.data.map((version) => (
              <li key={version.id}>
                <button
                  type="button"
                  onClick={() => onPick(version)}
                  className="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-2 text-left outline-none hover:bg-accent focus-visible:bg-accent"
                >
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium">{version.name}</span>
                    <span className="block truncate font-mono text-xs text-muted-foreground">
                      {version.filename}
                    </span>
                  </span>
                  <span className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
                    {version.channel !== "release" && (
                      <Badge variant="outline">{version.channel}</Badge>
                    )}
                    <span className="font-mono tabular">{bytes(version.size)}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

function ReportDialog({ report, onClose }: { report: MrpackReport; onClose: () => void }) {
  const { t, plural } = useI18n();
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {report.name} {report.version}
          </DialogTitle>
          <DialogDescription>
            {report.failed.length > 0
              ? plural(
                  report.added,
                  "{count} file added to the modpack, {failed} failed.",
                  "{count} files added to the modpack, {failed} failed.",
                  { failed: report.failed.length },
                )
              : plural(
                  report.added,
                  "{count} file added to the modpack.",
                  "{count} files added to the modpack.",
                )}
          </DialogDescription>
        </DialogHeader>
        {report.mismatch && (
          <div className="flex items-start gap-2 rounded-xl border border-warn/40 bg-warn/10 px-3 py-2 text-sm">
            <Icon name="alert" className="mt-0.5 text-warn" appear />
            <span>
              {t(
                "The pack targets {target}, which differs from this instance. Change the platform in Settings or expect mods to fail to load.",
                {
                  target:
                    (report.loader ? `${LOADER_NAMES[report.loader.type]} ` : "") +
                    (report.minecraft_version ?? t("another version")),
                },
              )}
            </span>
          </div>
        )}
        {report.failed.length > 0 && (
          <ul className="max-h-60 space-y-2 overflow-y-auto text-sm">
            {report.failed.map((item) => (
              <li key={item.path}>
                <p className="truncate font-mono text-xs">{item.path}</p>
                <p className="text-destructive">{item.error}</p>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function ModsTab({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const settings = useSettings();
  const base = `/admin/instances/${instance.id}`;

  const [kind, setKind] = useState<"mods" | "packs">("mods");
  const [provider, setProvider] = useState<Provider>("modrinth");
  const [draft, setDraft] = useState("");
  const [term, setTerm] = useState("");
  const [picking, setPicking] = useState<SearchHit | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [added, setAdded] = useState<Set<string>>(new Set());
  const [report, setReport] = useState<MrpackReport | null>(null);
  const packPicker = useRef<HTMLInputElement>(null);

  const keyMissing = provider === "curseforge" && settings.data?.curseforge_key_set === false;
  const results = useQuery({
    queryKey: ["mod-search", instance.id, provider, term],
    queryFn: ({ signal }) =>
      api<SearchHit[]>("GET", `${base}/mods/search`, { query: { provider, q: term }, signal }),
    enabled: !keyMissing,
    staleTime: 60_000,
  });

  const refresh = () => {
    void client.invalidateQueries({ queryKey: keys.files(instance.id) });
    void client.invalidateQueries({ queryKey: keys.instances });
  };

  const install = async (
    source: Provider,
    project: string,
    title: string,
    version?: string,
  ): Promise<void> => {
    const tag = `${source}:${project}`;
    setInstalling(tag);
    try {
      const done = await api<Installed>("POST", `${base}/mods/install`, {
        json: { provider: source, project, version },
      });
      setAdded((previous) => new Set(previous).add(tag));
      refresh();
      const needs = done.requires;
      if (needs.length === 0) {
        toast.success(t("{title} added", { title }), { description: done.file.path });
      } else {
        toast.success(t("{title} added", { title }), {
          description: t("Needs {mods}", { mods: needs.map((item) => item.title).join(", ") }),
          duration: 12000,
          action: {
            label:
              needs.length === 1 ? t("Add it") : t("Add all {count}", { count: needs.length }),
            onClick: () => void installMany(source, needs),
          },
        });
      }
    } catch (error) {
      toast.error(`${title}: ${message(error)}`);
    } finally {
      setInstalling(null);
    }
  };

  const installMany = async (source: Provider, list: Dependency[]) => {
    for (const item of list) await install(source, item.id, item.title);
  };

  const importPack = useMutation({
    mutationFn: (file: File) => api<MrpackReport>("POST", `${base}/mrpack`, { raw: file }),
    onSuccess: (result) => {
      setReport(result);
      refresh();
    },
    onError: (error) => toast.error(message(error)),
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTerm(draft.trim());
  };

  const vanilla = instance.loader.type === "vanilla";

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 bg-background px-5 py-3">
        <Tabs value={kind} onValueChange={(next) => setKind(next as "mods" | "packs")}>
          <TabsList>
            <TabsTrigger value="mods">{t("Mods")}</TabsTrigger>
            <TabsTrigger value="packs">{t("Modpacks")}</TabsTrigger>
          </TabsList>
        </Tabs>
        <p className="text-sm text-muted-foreground">
          {kind === "mods"
            ? t("Single mods, added to the modpack one at a time.")
            : t("Whole modpacks, ready to play.")}
        </p>
      </div>

      {kind === "packs" && (
      <Tile className="gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="font-medium">{t("Import a modpack")}</h2>
          <p className="text-sm text-muted-foreground">
            {t(
              "A Modrinth .mrpack brings its mods, configs and the side of every file in one step.",
            )}
          </p>
        </div>
        <input
          ref={packPicker}
          type="file"
          accept=".mrpack"
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) importPack.mutate(file);
            event.target.value = "";
          }}
        />
        <ActionButton
          icon="box"
          variant="outline"
          busy={importPack.isPending}
          onClick={() => packPicker.current?.click()}
        >
          {importPack.isPending ? t("Importing") : t("Choose .mrpack")}
        </ActionButton>
      </Tile>
      )}

      {kind === "packs" && <PackCatalogue instance={instance} onReport={setReport} />}

      {kind === "mods" && (
      <Tile className="gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <Tabs value={provider} onValueChange={(next) => setProvider(next as Provider)}>
            <TabsList>
              <TabsTrigger value="modrinth">Modrinth</TabsTrigger>
              <TabsTrigger value="curseforge">CurseForge</TabsTrigger>
            </TabsList>
          </Tabs>
          <form onSubmit={submit} className="flex min-w-56 flex-1 items-center gap-2">
            <div className="relative flex-1">
              <Icon
                name="search"
                className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder={t("Search {provider} for {loader} {version}", {
                  provider: PROVIDER_NAMES[provider],
                  loader: LOADER_NAMES[instance.loader.type],
                  version: instance.minecraft_version,
                })}
                aria-label={t("Search mods")}
                className="pl-8"
              />
            </div>
            <Button type="submit" variant="secondary">
              {t("Search")}
            </Button>
          </form>
        </div>

        {vanilla && (
          <p className="text-sm text-muted-foreground">
            {t(
              "This instance runs vanilla Minecraft, which loads no mods. Pick a loader in Settings first.",
            )}
          </p>
        )}

        {keyMissing ? (
          <Empty icon="key" title={t("CurseForge needs an API key")}>
            <p>{t("CurseForge only answers requests signed with your own key.")}</p>
            <Button asChild variant="outline" className="mt-4">
              <Link href="/settings">
                <Icon name="settings" />
                {t("Add the key in Settings")}
              </Link>
            </Button>
          </Empty>
        ) : results.isPending ? (
          <Loading label={t("Searching")} />
        ) : results.error ? (
          <ErrorNote error={results.error} />
        ) : results.data.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            {term
              ? t("Nothing found for “{term}” on this platform.", { term })
              : t("Nothing found on this platform.")}
          </p>
        ) : (
          <ul className="grid gap-(--frame) lg:grid-cols-2">
            {results.data.map((hit) => {
              const tag = `${hit.provider}:${hit.id}`;
              const busy = installing === tag;
              return (
                <li key={tag} className="flex gap-3 rounded-xl border p-3">
                  {hit.icon_url ? (
                    <img
                      src={hit.icon_url}
                      alt=""
                      loading="lazy"
                      referrerPolicy="no-referrer"
                      className="size-12 shrink-0 rounded-lg bg-muted object-cover"
                    />
                  ) : (
                    <span className="grid size-12 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
                      <Icon name="mods" size={20} />
                    </span>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <a
                        href={hit.page_url}
                        target="_blank"
                        rel="noreferrer"
                        className="truncate font-medium outline-none hover:underline focus-visible:underline"
                      >
                        {hit.title}
                      </a>
                      <span className="shrink-0 truncate text-xs text-muted-foreground">
                        {hit.author}
                      </span>
                    </div>
                    <p className="mt-0.5 line-clamp-2 text-sm text-muted-foreground">
                      {hit.description}
                    </p>
                    <div className="mt-2 flex flex-wrap items-center gap-2">
                      <span className="flex items-center gap-1 text-xs text-muted-foreground">
                        <Icon name="download" size={12} />
                        <span className="font-mono tabular">{compact(hit.downloads)}</span>
                      </span>
                      {hit.side !== "unknown" && (
                        <Badge variant="secondary">{t(SIDE_NAMES[hit.side])}</Badge>
                      )}
                      <span className="flex-1" />
                      {!hit.distributable ? (
                        <Button asChild variant="outline" size="sm">
                          <a
                            href={hit.page_url}
                            target="_blank"
                            rel="noreferrer"
                            title={t(
                              "The author does not allow third-party downloads. Get the file from the page and upload it on the Files tab.",
                            )}
                          >
                            <Icon name="external" />
                            {t("Manual download")}
                          </a>
                        </Button>
                      ) : (
                        <>
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={busy}
                            onClick={() => setPicking(hit)}
                          >
                            {t("Versions")}
                          </Button>
                          <ActionButton
                            icon={added.has(tag) ? "check" : "plus"}
                            size="sm"
                            variant={added.has(tag) ? "secondary" : "default"}
                            busy={busy}
                            disabled={installing !== null}
                            onClick={() => void install(hit.provider, hit.id, hit.title)}
                          >
                            {added.has(tag) ? t("Added") : t("Add")}
                          </ActionButton>
                        </>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </Tile>
      )}

      {picking && (
        <VersionPicker
          instance={instance}
          hit={picking}
          onClose={() => setPicking(null)}
          onPick={(version) => {
            const hit = picking;
            setPicking(null);
            void install(hit.provider, hit.id, hit.title, version.id);
          }}
        />
      )}
      {report && <ReportDialog report={report} onClose={() => setReport(null)} />}

      <Help>
        <p>
          {t(
            "Everything added here goes into the modpack: the server gets it at the next deploy and players' launchers download it before the next game.",
          )}
        </p>
        <p>
          {t(
            "A mod that needs a library says so when you add it, with a button that adds the library too. Check the result on the Modpack page.",
          )}
        </p>
      </Help>
    </>
  );
}
