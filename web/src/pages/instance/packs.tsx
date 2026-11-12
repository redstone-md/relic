import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";

import { ActionButton, Confirm, Empty, ErrorNote, Loading, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api, message } from "@/lib/api";
import { compact, LOADER_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys } from "@/lib/queries";
import type { Instance, MrpackReport, PackHit } from "@/lib/types";

const ANY = "any";

const CATEGORIES: [string, string][] = [
  ["adventure", "Adventure ## kind of modpack"],
  ["challenging", "Challenging"],
  ["combat", "Combat"],
  ["kitchen-sink", "Kitchen sink"],
  ["lightweight", "Lightweight"],
  ["magic", "Magic"],
  ["multiplayer", "Multiplayer"],
  ["optimization", "Optimization"],
  ["quests", "Quests"],
  ["technology", "Technology"],
];

const SORTS: [string, string][] = [
  ["relevance", "Best match"],
  ["downloads", "Most downloaded"],
  ["follows", "Most followed"],
  ["updated", "Recently updated"],
];

function Card({
  hit,
  busy,
  locked,
  onInstall,
}: {
  hit: PackHit;
  busy: boolean;
  locked: boolean;
  onInstall: () => void;
}) {
  const { t } = useI18n();
  const named = new Map(CATEGORIES);
  const tags = hit.categories.filter((id) => named.has(id)).slice(0, 3);

  return (
    <li className="flex min-w-0 flex-col border">
      <div className="relative aspect-[16/9] overflow-hidden bg-muted">
        {hit.cover_url ? (
          <img
            src={hit.cover_url}
            alt=""
            loading="lazy"
            referrerPolicy="no-referrer"
            className="size-full object-cover"
          />
        ) : hit.icon_url ? (
          <img
            src={hit.icon_url}
            alt=""
            loading="lazy"
            referrerPolicy="no-referrer"
            className="size-full scale-150 object-cover opacity-30 blur-md"
          />
        ) : (
          <span className="grid size-full place-items-center text-muted-foreground">
            <Icon name="box" size={28} />
          </span>
        )}
      </div>
      <div className="flex flex-1 flex-col gap-2 p-3">
        <div className="flex items-center gap-3">
          {hit.icon_url ? (
            <img
              src={hit.icon_url}
              alt=""
              loading="lazy"
              referrerPolicy="no-referrer"
              className="size-10 shrink-0 bg-muted object-cover"
            />
          ) : (
            <span className="grid size-10 shrink-0 place-items-center bg-muted text-muted-foreground">
              <Icon name="box" />
            </span>
          )}
          <div className="min-w-0">
            <a
              href={hit.page_url}
              target="_blank"
              rel="noreferrer"
              className="block truncate font-medium outline-none hover:underline focus-visible:underline"
            >
              {hit.title}
            </a>
            <p className="truncate text-xs text-muted-foreground">{hit.author}</p>
          </div>
        </div>
        <p className="line-clamp-3 text-sm text-muted-foreground">{hit.description}</p>
        {tags.length > 0 && (
          <p className="truncate text-xs text-muted-foreground">
            {tags.map((id) => t(named.get(id)!)).join(", ")}
          </p>
        )}
        <div className="mt-auto flex items-center justify-between gap-3 pt-1">
          <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
            <Icon name="download" size={12} />
            <span className="font-mono tabular">{compact(hit.downloads)}</span>
          </span>
          <ActionButton icon="install" size="sm" busy={busy} disabled={locked} onClick={onInstall}>
            {t("Install")}
          </ActionButton>
        </div>
      </div>
    </li>
  );
}

/** Ready-made modpacks from Modrinth that fit the instance, one click from being installed. */
export function PackCatalogue({
  instance,
  onReport,
}: {
  instance: Instance;
  onReport: (report: MrpackReport) => void;
}) {
  const { t } = useI18n();
  const client = useQueryClient();
  const base = `/admin/instances/${instance.id}`;
  const [draft, setDraft] = useState("");
  const [term, setTerm] = useState("");
  const [category, setCategory] = useState(ANY);
  const [sort, setSort] = useState("downloads");
  const [asking, setAsking] = useState<PackHit | null>(null);
  const vanilla = instance.loader.type === "vanilla";

  const results = useQuery({
    queryKey: ["pack-search", instance.id, term, category, sort],
    queryFn: ({ signal }) =>
      api<PackHit[]>("GET", `${base}/packs/search`, {
        query: { q: term, category: category === ANY ? "" : category, sort },
        signal,
      }),
    enabled: !vanilla,
    staleTime: 60_000,
  });

  const install = useMutation({
    mutationFn: (hit: PackHit) =>
      api<MrpackReport>("POST", `${base}/packs/install`, { json: { project: hit.id } }),
    onSuccess: (report) => {
      onReport(report);
      void client.invalidateQueries({ queryKey: keys.files(instance.id) });
      void client.invalidateQueries({ queryKey: keys.instances });
    },
    onError: (error) => toast.error(message(error)),
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTerm(draft.trim());
  };

  if (vanilla) {
    return (
      <Empty icon="box" title={t("Vanilla Minecraft loads no modpacks")}>
        {t("Pick a loader in Settings first; the catalogue then shows the packs made for it.")}
      </Empty>
    );
  }

  return (
    <Tile className="gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <form onSubmit={submit} className="flex min-w-56 flex-1 items-center gap-2">
          <div className="relative flex-1">
            <Icon
              name="search"
              className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              placeholder={t("Search modpacks for {loader} {version}", {
                loader: LOADER_NAMES[instance.loader.type],
                version: instance.minecraft_version,
              })}
              aria-label={t("Search modpacks")}
              className="pl-8"
            />
          </div>
          <Button type="submit" variant="secondary">
            {t("Search")}
          </Button>
        </form>
        <Select value={category} onValueChange={setCategory}>
          <SelectTrigger aria-label={t("Kind of modpack")} className="w-44">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ANY}>{t("Any kind")}</SelectItem>
            {CATEGORIES.map(([id, name]) => (
              <SelectItem key={id} value={id}>
                {t(name)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={sort} onValueChange={setSort}>
          <SelectTrigger aria-label={t("Order")} className="w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SORTS.map(([id, name]) => (
              <SelectItem key={id} value={id}>
                {t(name)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {install.isPending && (
        <p className="flex items-center gap-2 text-sm text-muted-foreground" role="status">
          <Icon name="spinner" active />
          {t("Downloading {title} and its mods. A large pack takes a few minutes.", {
            title: install.variables.title,
          })}
        </p>
      )}

      {results.isPending ? (
        <Loading label={t("Searching")} />
      ) : results.error ? (
        <ErrorNote error={results.error} />
      ) : results.data.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted-foreground">
          {t("No modpack for {loader} {version} matches. Try another kind or fewer words.", {
            loader: LOADER_NAMES[instance.loader.type],
            version: instance.minecraft_version,
          })}
        </p>
      ) : (
        <ul className="grid gap-(--frame) sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {results.data.map((hit) => (
            <Card
              key={hit.id}
              hit={hit}
              busy={install.isPending && install.variables.id === hit.id}
              locked={install.isPending}
              onInstall={() => setAsking(hit)}
            />
          ))}
        </ul>
      )}

      <Confirm
        open={asking !== null}
        onOpenChange={(open) => !open && setAsking(null)}
        title={t("Install {title}?", { title: asking?.title ?? "" })}
        description={t(
          "Its newest build for {loader} {version} is downloaded from Modrinth and its mods and configs are added to this server's modpack. Files already there with the same name are replaced; nothing else is removed.",
          { loader: LOADER_NAMES[instance.loader.type], version: instance.minecraft_version },
        )}
        action={t("Install")}
        onConfirm={() => asking && install.mutate(asking)}
      />
    </Tile>
  );
}
