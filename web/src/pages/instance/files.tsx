import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type DragEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Confirm, Empty, ErrorNote, Help, Loading, Tile, TileLabel } from "@/components/common";
import { Icon } from "@/components/icon";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api, ApiError, downloadUrl, message, upload } from "@/lib/api";
import { bytes, dateTime } from "@/lib/format";
import { archiveUrl, toTrash, TRASH, trashOrigin } from "@/lib/fs";
import { useI18n } from "@/lib/i18n";
import { keys, useUsage } from "@/lib/queries";
import type { FsEntry, Instance } from "@/lib/types";
import { cn } from "@/lib/utils";

/** The largest file the editor opens. Larger ones are downloaded instead. */
const EDIT_LIMIT = 1024 * 1024;

/** Files that are never text, whatever their size. */
const BINARY =
  /\.(jar|zip|gz|tgz|zst|7z|rar|png|jpe?g|gif|webp|ico|ogg|mp3|wav|dat|dat_old|mca|mcr|nbt|lock|db|sqlite|class|exe|dll|so|bin|ttf|otf)$/i;

function join(folder: string, name: string): string {
  return folder ? `${folder}/${name}` : name;
}

function editable(entry: FsEntry): boolean {
  return !entry.dir && entry.size <= EDIT_LIMIT && !BINARY.test(entry.name);
}

/** A name the daemon will take: one path part, nothing that leaves the folder. */
function usable(name: string): boolean {
  const trimmed = name.trim();
  return trimmed !== "" && trimmed !== "." && trimmed !== ".." && !/[\\/:\0]/.test(trimmed);
}

interface Sending {
  name: string;
  sent: number;
  total: number;
  /** Which file of how many, for more than one. */
  index: number;
  count: number;
}

type Naming =
  | { kind: "folder" }
  | { kind: "file" }
  | { kind: "rename"; entry: FsEntry };

interface Editing {
  path: string;
  name: string;
  /** Null while the file is on its way. */
  text: string | null;
  saved: string;
}

type SortKey = "name" | "size" | "modified";

interface Sort {
  key: SortKey;
  desc: boolean;
}

function SortHead({
  id,
  sort,
  onSort,
  className,
  children,
}: {
  id: SortKey;
  sort: Sort;
  onSort: (sort: Sort) => void;
  className?: string;
  children: ReactNode;
}) {
  const active = sort.key === id;
  return (
    <TableHead
      className={className}
      aria-sort={active ? (sort.desc ? "descending" : "ascending") : undefined}
    >
      <button
        type="button"
        onClick={() => onSort({ key: id, desc: active ? !sort.desc : id !== "name" })}
        className={cn(
          "inline-flex cursor-pointer items-center gap-1 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
          active && "text-foreground",
        )}
      >
        {children}
        <Icon
          name="chevron-down"
          size={12}
          className={cn(!active && "opacity-0", active && !sort.desc && "rotate-180")}
        />
      </button>
    </TableHead>
  );
}

const SHARES = [
  "bg-primary",
  "bg-foreground/60",
  "bg-foreground/45",
  "bg-foreground/30",
  "bg-foreground/20",
  "bg-foreground/12",
];

/** What takes the disk: the heaviest folders as one bar, each a click from being opened. */
function Storage({ instance, onOpen }: { instance: Instance; onOpen: (folder: string) => void }) {
  const { t } = useI18n();
  const usage = useUsage(instance.id);
  if (!usage.data || usage.data.total === 0) return null;

  const { total, folders } = usage.data;
  const top = folders.slice(0, SHARES.length - 1);
  const rest = folders.slice(SHARES.length - 1).reduce((sum, share) => sum + share.size, 0);
  const shares = [
    ...top.map((share) => ({
      folder: share.name as string | null,
      name:
        share.name === "" ? t("Files in the top folder") : share.name === TRASH ? t("Trash") : share.name,
      size: share.size,
    })),
    ...(rest > 0 ? [{ folder: null, name: t("Everything else"), size: rest }] : []),
  ];

  return (
    <Tile className="gap-3">
      <div className="flex items-baseline justify-between gap-3">
        <TileLabel icon="gauge">{t("Storage")}</TileLabel>
        <p className="font-pixel text-sm leading-6 tabular">{bytes(total)}</p>
      </div>
      <div className="flex h-3 gap-px" aria-hidden>
        {shares.map((share, index) => (
          <div
            key={share.name}
            className={cn("min-w-0.5", SHARES[index])}
            style={{ width: `${(share.size / total) * 100}%` }}
          />
        ))}
      </div>
      <ul className="flex flex-wrap gap-x-5 gap-y-1 text-sm">
        {shares.map((share, index) => {
          const body = (
            <>
              <span className={cn("size-2.5 shrink-0", SHARES[index])} />
              <span className="truncate">{share.name}</span>
              <span className="font-mono text-muted-foreground tabular">{bytes(share.size)}</span>
            </>
          );
          return (
            <li key={share.name} className="min-w-0">
              {share.folder ? (
                <button
                  type="button"
                  onClick={() => onOpen(share.folder!)}
                  className="flex max-w-full cursor-pointer items-center gap-2 outline-none hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {body}
                </button>
              ) : (
                <span className="flex items-center gap-2">{body}</span>
              )}
            </li>
          );
        })}
      </ul>
    </Tile>
  );
}

export function FilesTab({ instance }: { instance: Instance }) {
  const { t, plural } = useI18n();
  const client = useQueryClient();
  const base = `/admin/instances/${instance.id}/fs`;

  const [folder, setFolder] = useState("");
  const [naming, setNaming] = useState<Naming | null>(null);
  const [name, setName] = useState("");
  /** Names from the trash about to be deleted for good. */
  const [purging, setPurging] = useState<string[] | null>(null);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>({ key: "name", desc: false });
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<Editing | null>(null);
  const [sending, setSending] = useState<Sending | null>(null);
  const [over, setOver] = useState(false);
  const picker = useRef<HTMLInputElement>(null);

  const inTrash = folder === TRASH;
  const list = useQuery({
    queryKey: keys.fs(instance.id, folder),
    queryFn: async () => {
      try {
        return await api<FsEntry[]>("GET", base, { query: { path: folder } });
      } catch (error) {
        // The trash only exists once something has been thrown away.
        if (folder === TRASH && error instanceof ApiError && error.status === 404) return [];
        throw error;
      }
    },
  });
  const refresh = () => {
    void client.invalidateQueries({ queryKey: keys.fsUsage(instance.id) });
    return client.invalidateQueries({ queryKey: ["fs", instance.id] });
  };

  /** Opens a folder; what was ticked or searched for belongs to the one left behind. */
  const go = (path: string) => {
    setFolder(path);
    setPicked(new Set());
    setSearch("");
  };

  // A folder deleted elsewhere: step back out of it rather than show an error.
  const gone = list.error instanceof ApiError && list.error.status === 404 && folder !== "";
  useEffect(() => {
    if (gone) go("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gone]);

  const crumbs = folder ? folder.split("/") : [];

  async function send(files: File[]) {
    if (files.length === 0 || sending) return;
    let done = 0;
    try {
      for (const [index, file] of files.entries()) {
        setSending({ name: file.name, sent: 0, total: file.size, index, count: files.length });
        await upload(`${base}/content`, file, { path: join(folder, file.name) }, (sent) =>
          setSending((now) => now && { ...now, sent }),
        );
        done += 1;
      }
    } catch (error) {
      toast.error(message(error));
    }
    setSending(null);
    if (done > 0) {
      toast.success(plural(done, "{count} file uploaded", "{count} files uploaded"));
    }
    void refresh();
  }

  function drop(event: DragEvent) {
    event.preventDefault();
    setOver(false);
    // A dragged folder arrives as an entry without content; skip those.
    const files = [...event.dataTransfer.files].filter((file) => file.size > 0 || file.type !== "");
    if (files.length < event.dataTransfer.files.length) {
      toast.info(t("Folders cannot be dropped. Upload their files instead."));
    }
    void send(files);
  }

  const create = useMutation({
    mutationFn: async (what: Naming) => {
      const target = join(folder, name.trim());
      if (what.kind === "folder") {
        await api("POST", `${base}/mkdir`, { json: { path: target } });
      } else if (what.kind === "file") {
        if (list.data?.some((entry) => entry.name === name.trim())) {
          throw new Error(t("This name is taken"));
        }
        await api("PUT", `${base}/content`, { raw: new Blob([""]), query: { path: target } });
      } else {
        await api("POST", `${base}/rename`, {
          json: { from: join(folder, what.entry.name), to: target },
        });
      }
      return what;
    },
    onSuccess: (what) => {
      setNaming(null);
      void refresh();
      if (what.kind === "file") {
        // After the naming dialog has let go of the focus: closing it would
        // otherwise count as a click outside the editor and shut that too.
        const created = { name: name.trim(), size: 0 };
        setTimeout(() => void open(created), 250);
      }
    },
    onError: (error) => toast.error(message(error)),
  });

  const settle = () => {
    setPicked(new Set());
    void refresh();
  };

  const discard = useMutation({
    mutationFn: async (names: string[]) => {
      for (const entry of names) await toTrash(instance.id, join(folder, entry));
      return names.length;
    },
    onSuccess: (count) =>
      toast.success(plural(count, "{count} item moved to the trash", "{count} items moved to the trash")),
    onError: (error) => toast.error(message(error)),
    onSettled: settle,
  });

  const restore = useMutation({
    mutationFn: async (names: string[]) => {
      for (const entry of names) {
        const origin = trashOrigin(entry);
        if (!origin) throw new Error(t("{name} does not say where it came from. Rename it in place.", { name: entry }));
        await api("POST", `${base}/rename`, {
          json: { from: `${TRASH}/${entry}`, to: origin.path },
        });
      }
      return names.length;
    },
    onSuccess: (count) => toast.success(plural(count, "{count} item put back", "{count} items put back")),
    onError: (error) => toast.error(message(error)),
    onSettled: settle,
  });

  const purge = useMutation({
    mutationFn: async (names: string[]) => {
      for (const entry of names) {
        await api("DELETE", base, { query: { path: `${TRASH}/${entry}` } });
      }
      return names.length;
    },
    onSuccess: (count) => toast.success(plural(count, "{count} item deleted", "{count} items deleted")),
    onError: (error) => toast.error(message(error)),
    onSettled: settle,
  });

  async function open(entry: Pick<FsEntry, "name" | "size">) {
    const path = join(folder, entry.name);
    setEditing({ path, name: entry.name, text: null, saved: "" });
    try {
      const response = await fetch(
        `${downloadUrl(`${base}/content`)}?path=${encodeURIComponent(path)}`,
        { credentials: "same-origin" },
      );
      if (!response.ok) throw new Error(t("The file could not be read"));
      const buffer = await response.arrayBuffer();
      if (new Uint8Array(buffer).includes(0)) {
        throw new Error(t("This is not a text file. Download it instead."));
      }
      const text = new TextDecoder().decode(buffer);
      setEditing((now) => (now?.path === path ? { ...now, text, saved: text } : now));
    } catch (error) {
      setEditing((now) => (now?.path === path ? null : now));
      toast.error(message(error));
    }
  }

  const save = useMutation({
    mutationFn: (file: Editing) =>
      api("PUT", `${base}/content`, {
        raw: new Blob([file.text ?? ""]),
        query: { path: file.path },
      }),
    onSuccess: (_, file) => {
      toast.success(t("{name} saved", { name: file.name }));
      setEditing((now) => (now?.path === file.path ? { ...now, saved: file.text ?? "" } : now));
      void refresh();
    },
    onError: (error) => toast.error(message(error)),
  });

  function ask(what: Naming) {
    setName(what.kind === "rename" ? what.entry.name : "");
    setNaming(what);
  }

  const changed = editing !== null && editing.text !== null && editing.text !== editing.saved;

  /** What a row is called: in the trash, the path the file was thrown away from. */
  const title = (entry: FsEntry) => (inTrash ? (trashOrigin(entry.name)?.path ?? entry.name) : entry.name);
  const stamp = (entry: FsEntry) =>
    inTrash ? (trashOrigin(entry.name)?.at ?? entry.modified) : entry.modified;

  const needle = search.trim().toLowerCase();
  const hasTrash = folder === "" && (list.data?.some((entry) => entry.name === TRASH) ?? false);
  const rows = (list.data ?? [])
    .filter((entry) => !(folder === "" && entry.name === TRASH))
    .filter((entry) => needle === "" || title(entry).toLowerCase().includes(needle))
    .sort((a, b) => {
      if (a.dir !== b.dir) return a.dir ? -1 : 1;
      const order =
        sort.key === "size"
          ? a.size - b.size
          : sort.key === "modified"
            ? stamp(a) - stamp(b)
            : 0;
      const tie = title(a).localeCompare(title(b), undefined, { numeric: true, sensitivity: "base" });
      return order === 0 ? (sort.key === "name" && sort.desc ? -tie : tie) : sort.desc ? -order : order;
    });
  const chosen = rows.filter((entry) => picked.has(entry.name)).map((entry) => entry.name);
  const working = discard.isPending || restore.isPending || purge.isPending;

  const tick = (entry: string, on: boolean) =>
    setPicked((before) => {
      const next = new Set(before);
      if (on) next.add(entry);
      else next.delete(entry);
      return next;
    });
  const running = instance.runtime.status !== "stopped" && instance.runtime.status !== "crashed";

  return (
    <>
    <Tile
      className={cn("gap-4", over && "outline-2 -outline-offset-2 outline-primary")}
      onDragOver={(event) => {
        if (inTrash || !event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        setOver(true);
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(false);
      }}
      onDrop={(event) => {
        if (!inTrash) drop(event);
      }}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <nav
          aria-label={t("Folder")}
          className="flex min-w-0 flex-wrap items-center gap-1 font-mono text-sm"
        >
          <button
            type="button"
            onClick={() => go("")}
            className={cn(
              "cursor-pointer px-1 py-0.5 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
              crumbs.length === 0 ? "text-foreground" : "text-muted-foreground",
            )}
          >
            {instance.id}
          </button>
          {inTrash ? (
            <span className="flex items-center gap-1">
              <span className="text-muted-foreground">/</span>
              <span className="px-1 py-0.5 font-sans">{t("Trash")}</span>
            </span>
          ) : (
            crumbs.map((part, index) => (
              <span key={index} className="flex items-center gap-1">
                <span className="text-muted-foreground">/</span>
                <button
                  type="button"
                  onClick={() => go(crumbs.slice(0, index + 1).join("/"))}
                  className={cn(
                    "cursor-pointer px-1 py-0.5 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                    index === crumbs.length - 1 ? "text-foreground" : "text-muted-foreground",
                  )}
                >
                  {part}
                </button>
              </span>
            ))
          )}
        </nav>
        <div className="flex flex-wrap gap-2">
          <input
            ref={picker}
            type="file"
            multiple
            hidden
            onChange={(event) => {
              void send([...(event.target.files ?? [])]);
              event.target.value = "";
            }}
          />
          {inTrash ? (
            <Button
              variant="outline"
              size="sm"
              disabled={(list.data?.length ?? 0) === 0 || working}
              onClick={() => setPurging((list.data ?? []).map((entry) => entry.name))}
            >
              <Icon name="delete" />
              {t("Empty the trash")}
            </Button>
          ) : (
            <>
              <Button variant="ghost" size="sm" onClick={() => go(TRASH)}>
                <Icon name="delete" />
                {t("Trash")}
              </Button>
              <Button variant="outline" size="sm" onClick={() => ask({ kind: "folder" })}>
                <Icon name="plus" />
                {t("New folder")}
              </Button>
              <Button variant="outline" size="sm" onClick={() => ask({ kind: "file" })}>
                <Icon name="edit" />
                {t("New file")}
              </Button>
              <Button size="sm" disabled={sending !== null} onClick={() => picker.current?.click()}>
                <Icon name="upload" />
                {t("Upload")}
              </Button>
            </>
          )}
        </div>
      </div>

      <div className="flex min-h-8 flex-wrap items-center justify-between gap-3">
        <div className="relative w-full max-w-xs">
          <Icon
            name="search"
            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            type="search"
            value={search}
            placeholder={t("Search this folder")}
            aria-label={t("Search this folder")}
            onChange={(event) => setSearch(event.target.value)}
            className="h-8 pl-8"
          />
        </div>
        {chosen.length > 0 && (
          <div className="flex flex-wrap items-center gap-2" role="status">
            <span className="text-sm text-muted-foreground">
              {plural(chosen.length, "{count} selected", "{count} selected")}
            </span>
            {inTrash ? (
              <>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={working}
                  onClick={() => restore.mutate(chosen)}
                >
                  <Icon name="history" />
                  {t("Put back")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="text-destructive"
                  disabled={working}
                  onClick={() => setPurging(chosen)}
                >
                  <Icon name="delete" />
                  {t("Delete for good")}
                </Button>
              </>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                disabled={working}
                onClick={() => discard.mutate(chosen)}
              >
                <Icon name="delete" />
                {t("Move to the trash")}
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={() => setPicked(new Set())}>
              {t("Clear")}
            </Button>
          </div>
        )}
      </div>

      {sending && (
        <div className="grid gap-1.5" role="status">
          <div className="flex justify-between gap-4 text-xs text-muted-foreground">
            <span className="truncate">
              {sending.count > 1
                ? t("Uploading {name}, {index} of {count}", {
                    name: sending.name,
                    index: sending.index + 1,
                    count: sending.count,
                  })
                : t("Uploading {name}", { name: sending.name })}
            </span>
            <span className="font-mono tabular">
              {bytes(sending.sent)} / {bytes(sending.total)}
            </span>
          </div>
          <Progress value={sending.total ? (sending.sent / sending.total) * 100 : 100} />
        </div>
      )}

      {list.isPending || gone ? (
        <Loading />
      ) : list.error ? (
        <ErrorNote error={list.error} />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-0 pr-0">
                <Checkbox
                  aria-label={t("Select everything shown")}
                  disabled={rows.length === 0}
                  checked={
                    chosen.length === 0
                      ? false
                      : chosen.length === rows.length
                        ? true
                        : "indeterminate"
                  }
                  onCheckedChange={(on) =>
                    setPicked(on === true ? new Set(rows.map((entry) => entry.name)) : new Set())
                  }
                />
              </TableHead>
              <SortHead id="name" sort={sort} onSort={setSort}>
                {inTrash ? t("Was at") : t("Name")}
              </SortHead>
              <SortHead id="size" sort={sort} onSort={setSort} className="text-right">
                {t("Size")}
              </SortHead>
              <SortHead
                id="modified"
                sort={sort}
                onSort={setSort}
                className="hidden text-right sm:table-cell"
              >
                {inTrash ? t("Thrown away") : t("Changed")}
              </SortHead>
              <TableHead className="w-0" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {folder !== "" && (
              <TableRow>
                <TableCell colSpan={5} className="p-0">
                  <button
                    type="button"
                    data-icon-trigger
                    onClick={() => go(inTrash ? "" : crumbs.slice(0, -1).join("/"))}
                    className="flex w-full cursor-pointer items-center gap-2.5 p-2 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
                  >
                    <Icon name="back" />
                    {inTrash ? t("Back to the files") : t("Up one folder")}
                  </button>
                </TableCell>
              </TableRow>
            )}
            {rows.map((entry) => {
              const path = join(folder, entry.name);
              const href = entry.dir
                ? archiveUrl(instance.id, path)
                : `${downloadUrl(`${base}/content`)}?path=${encodeURIComponent(path)}`;
              const canEdit = !inTrash && editable(entry);
              const canOpen = inTrash ? false : entry.dir || canEdit;
              const shown = title(entry);
              const when = stamp(entry);
              return (
                <TableRow key={entry.name} data-state={picked.has(entry.name) ? "selected" : undefined}>
                  <TableCell className="w-0 pr-0">
                    <Checkbox
                      aria-label={t("Select {name}", { name: shown })}
                      checked={picked.has(entry.name)}
                      onCheckedChange={(on) => tick(entry.name, on === true)}
                    />
                  </TableCell>
                  <TableCell className="max-w-0 w-full p-0">
                    <button
                      type="button"
                      data-icon-trigger
                      disabled={!canOpen}
                      title={
                        canOpen || inTrash ? undefined : t("Too large or not text: download it")
                      }
                      onClick={() => (entry.dir ? go(path) : void open(entry))}
                      className="flex w-full min-w-0 items-center gap-2.5 p-2 text-left outline-none enabled:cursor-pointer enabled:hover:text-primary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
                    >
                      <Icon
                        name={entry.dir ? "files" : "news"}
                        className={entry.dir ? "text-primary" : "text-muted-foreground"}
                      />
                      <span className={cn("truncate", entry.dir && "font-medium")}>{shown}</span>
                      {entry.pack && !inTrash && (
                        <span
                          className="shrink-0 border px-1.5 text-xs text-muted-foreground"
                          title={t(
                            "Placed here from the modpack. The next deploy puts the modpack's version back.",
                          )}
                        >
                          {t("modpack")}
                        </span>
                      )}
                    </button>
                  </TableCell>
                  <TableCell className="text-right font-mono whitespace-nowrap tabular">
                    {entry.dir ? "" : bytes(entry.size)}
                  </TableCell>
                  <TableCell className="hidden text-right whitespace-nowrap text-muted-foreground sm:table-cell">
                    {when ? dateTime(when) : ""}
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-1">
                      {inTrash ? (
                        <>
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={working}
                            onClick={() => restore.mutate([entry.name])}
                          >
                            <Icon name="history" />
                            {t("Put back")}
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={t("Delete {name} for good", { name: shown })}
                            title={t("Delete for good")}
                            disabled={working}
                            onClick={() => setPurging([entry.name])}
                          >
                            <Icon name="delete" />
                          </Button>
                        </>
                      ) : (
                        <>
                          <Button asChild variant="ghost" size="icon-sm">
                            <a
                              href={href}
                              download={entry.dir ? `${entry.name}.zip` : entry.name}
                              aria-label={
                                entry.dir
                                  ? t("Download {name} as a zip", { name: entry.name })
                                  : t("Download {name}", { name: entry.name })
                              }
                              title={entry.dir ? t("Download as a zip") : t("Download")}
                            >
                              <Icon name="download" />
                            </a>
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={t("Rename {name}", { name: entry.name })}
                            title={t("Rename")}
                            onClick={() => ask({ kind: "rename", entry })}
                          >
                            <Icon name="edit" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={t("Move {name} to the trash", { name: entry.name })}
                            title={t("Move to the trash")}
                            disabled={working}
                            onClick={() => discard.mutate([entry.name])}
                          >
                            <Icon name="delete" />
                          </Button>
                        </>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}

      {list.data && rows.length === 0 &&
        (needle !== "" ? (
          <Empty icon="search" title={t("Nothing here matches “{query}”", { query: search.trim() })}>
            {t("The search looks at the open folder only.")}
          </Empty>
        ) : inTrash ? (
          <Empty icon="circle-check" title={t("The trash is empty")}>
            {t("Files you delete wait here until you empty it.")}
          </Empty>
        ) : (
          <Empty icon="upload-cloud" title={t("This folder is empty")}>
            {t("Drop files here or press Upload.")}
          </Empty>
        ))}

      <p className="text-xs text-muted-foreground">
        {inTrash
          ? t("Everything here still takes disk space.")
          : running
            ? t(
                "The server is running. It reads most files only at start and may overwrite its own while it runs.",
              )
            : hasTrash
              ? t("Drop files anywhere on this tile to upload them. Deleted files wait in the trash.")
              : t("Drop files anywhere on this tile to upload them into the open folder.")}
      </p>

      <Dialog open={naming !== null} onOpenChange={(open) => !open && setNaming(null)}>
        <DialogContent>
          <form
            className="grid gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              if (naming && usable(name)) create.mutate(naming);
            }}
          >
            <DialogHeader>
              <DialogTitle>
                {naming?.kind === "folder"
                  ? t("New folder")
                  : naming?.kind === "file"
                    ? t("New file")
                    : t("Rename {name}", { name: naming?.entry.name ?? "" })}
              </DialogTitle>
              <DialogDescription>
                {t("In {folder}", { folder: folder ? `${instance.id}/${folder}` : instance.id })}
              </DialogDescription>
            </DialogHeader>
            <Input
              autoFocus
              value={name}
              aria-label={t("Name")}
              aria-invalid={name !== "" && !usable(name)}
              onChange={(event) => setName(event.target.value)}
            />
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setNaming(null)}>
                {t("Cancel")}
              </Button>
              <Button type="submit" disabled={!usable(name) || create.isPending}>
                {naming?.kind === "rename" ? t("Rename") : t("Create")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={editing !== null}
        onOpenChange={(open) => {
          if (!open && !save.isPending) setEditing(null);
        }}
      >
        <DialogContent className="flex h-[min(44rem,90dvh)] flex-col sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle className="truncate font-mono text-sm">{editing?.path}</DialogTitle>
            <DialogDescription>
              {changed ? t("Changes are not saved yet") : t("Saved")}
            </DialogDescription>
          </DialogHeader>
          {editing?.text === null ? (
            <div className="flex-1">
              <Loading />
            </div>
          ) : (
            <textarea
              value={editing?.text ?? ""}
              spellCheck={false}
              aria-label={editing?.name}
              onChange={(event) =>
                setEditing((now) => now && { ...now, text: event.target.value })
              }
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === "s") {
                  event.preventDefault();
                  if (editing && changed && !save.isPending) save.mutate(editing);
                }
              }}
              className="min-h-0 flex-1 resize-none border bg-background p-3 font-mono text-[13px] leading-5 whitespace-pre outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setEditing(null)}>
              {t("Close")}
            </Button>
            <Button
              disabled={!changed || save.isPending}
              onClick={() => editing && save.mutate(editing)}
            >
              <Icon name={save.isPending ? "spinner" : "check"} active={save.isPending} />
              {t("Save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Confirm
        open={purging !== null}
        onOpenChange={(open) => !open && setPurging(null)}
        title={plural(
          purging?.length ?? 0,
          "Delete {count} item for good?",
          "Delete {count} items for good?",
        )}
        description={t("They leave the trash and the disk. This cannot be undone.")}
        action={t("Delete for good")}
        destructive
        onConfirm={() => purging && purge.mutate(purging)}
      />
    </Tile>

    {!inTrash && <Storage instance={instance} onOpen={go} />}

    <Help>
      <p>
        {t(
          "This is the server's own folder. Click a text file to edit it; anything else downloads, and a folder downloads as a zip.",
        )}
      </p>
      <p>
        {t(
          "Deleting moves a file to the trash, from where it can be put back to where it was. Files marked “modpack” come back with the next deploy: remove those on the Modpack page.",
        )}
      </p>
    </Help>
    </>
  );
}
