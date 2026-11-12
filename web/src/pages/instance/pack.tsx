import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type DragEvent, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { Confirm, Empty, ErrorNote, Loading, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api, filePath, message, upload } from "@/lib/api";
import { bytes, SIDE_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useFiles } from "@/lib/queries";
import type { FileKind, FileRecord, Instance, Side, SyncAction } from "@/lib/types";
import { cn } from "@/lib/utils";

const KIND_NAMES: Record<FileKind, string> = {
  mod: "Mods",
  config: "Configs",
  resource_pack: "Resource packs",
  shader_pack: "Shader packs",
  other: "Other",
};

const DESTINATIONS = [
  { value: "auto", label: "By file type" },
  { value: "mods", label: "mods/" },
  { value: "config", label: "config/" },
  { value: "resourcepacks", label: "resourcepacks/" },
  { value: "shaderpacks", label: "shaderpacks/" },
  { value: "root", label: "Instance root" },
];

const CONFIG_TYPES = new Set([
  "json", "json5", "toml", "cfg", "conf", "properties", "yml", "yaml", "ini", "txt", "snbt",
]);

/** Where a loose file belongs when the admin did not say. */
function place(name: string, destination: string): string {
  if (destination === "root") return name;
  if (destination !== "auto") return `${destination}/${name}`;
  const type = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
  if (type === "jar") return `mods/${name}`;
  if (type === "zip") return `resourcepacks/${name}`;
  if (CONFIG_TYPES.has(type)) return `config/${name}`;
  return name;
}

interface Picked {
  file: File;
  /** Path inside a dropped folder, or just the name for a loose file. */
  relative: string;
}

function readAll(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  return new Promise((resolve, reject) => {
    const all: FileSystemEntry[] = [];
    // One call returns at most a page of entries; keep asking until it is empty.
    const next = () =>
      reader.readEntries((page) => {
        if (page.length === 0) resolve(all);
        else {
          all.push(...page);
          next();
        }
      }, reject);
    next();
  });
}

async function walk(entry: FileSystemEntry, prefix: string, out: Picked[]): Promise<void> {
  if (entry.isFile) {
    const file = await new Promise<File>((resolve, reject) =>
      (entry as FileSystemFileEntry).file(resolve, reject),
    );
    out.push({ file, relative: prefix + entry.name });
  } else if (entry.isDirectory) {
    const children = await readAll((entry as FileSystemDirectoryEntry).createReader());
    for (const child of children) await walk(child, `${prefix}${entry.name}/`, out);
  }
}

async function collect(transfer: DataTransfer): Promise<Picked[]> {
  // Entries must be taken before the first await: the list empties afterwards.
  const entries = [...transfer.items]
    .filter((item) => item.kind === "file")
    .map((item) => item.webkitGetAsEntry?.() ?? null);
  const loose = [...transfer.files];
  const out: Picked[] = [];
  if (entries.some(Boolean)) {
    for (const entry of entries) if (entry) await walk(entry, "", out);
  } else {
    for (const file of loose) out.push({ file, relative: file.name });
  }
  return out;
}

interface Transfer {
  key: number;
  target: string;
  size: number;
  sent: number;
  state: "waiting" | "sending" | "done" | "failed";
  error?: string;
}

let nextKey = 0;

export function PackTab({ instance }: { instance: Instance }) {
  const { t, plural } = useI18n();
  const client = useQueryClient();
  const files = useFiles(instance.id);
  const base = `/admin/instances/${instance.id}/files`;

  const [search, setSearch] = useState("");
  const [side, setSide] = useState<Side | "all">("all");
  const [kind, setKind] = useState<FileKind | "all">("all");
  const [destination, setDestination] = useState("auto");
  const [dragging, setDragging] = useState(false);
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [deleting, setDeleting] = useState<FileRecord | null>(null);
  const depth = useRef(0);
  const picker = useRef<HTMLInputElement>(null);
  const chain = useRef<Promise<void>>(Promise.resolve());

  const refresh = () => {
    void client.invalidateQueries({ queryKey: keys.files(instance.id) });
    void client.invalidateQueries({ queryKey: keys.instances });
  };
  const fail = (error: unknown) => toast.error(message(error));

  const setSideOverride = useMutation({
    mutationFn: (input: { sha256: string; side: Side | null }) =>
      api("PUT", `/admin/sides/${input.sha256}`, { json: { side: input.side } }),
    onSuccess: refresh,
    onError: fail,
  });
  const patch = useMutation({
    mutationFn: (input: { path: string; body: { sync_action?: SyncAction; optional?: boolean } }) =>
      api<FileRecord>("PATCH", `${base}/${filePath(input.path)}`, { json: input.body }),
    onSuccess: refresh,
    onError: fail,
  });
  const remove = useMutation({
    mutationFn: (path: string) => api("DELETE", `${base}/${filePath(path)}`),
    onSuccess: refresh,
    onError: fail,
  });

  const update = (key: number, change: Partial<Transfer>) =>
    setTransfers((list) => list.map((item) => (item.key === key ? { ...item, ...change } : item)));

  const enqueue = (picked: Picked[]) => {
    const accepted = picked.filter((item) => !item.file.name.toLowerCase().endsWith(".mrpack"));
    if (accepted.length < picked.length) {
      toast.info(t("Modpack archives are imported on the Add mods tab."));
    }
    if (accepted.length === 0) return;
    const jobs = accepted.map((item) => ({
      key: nextKey++,
      file: item.file,
      target: item.relative.includes("/") ? item.relative : place(item.relative, destination),
    }));
    setTransfers((list) => [
      ...list.filter((item) => item.state !== "done"),
      ...jobs.map(
        (job): Transfer => ({
          key: job.key,
          target: job.target,
          size: job.file.size,
          sent: 0,
          state: "waiting",
        }),
      ),
    ]);
    // One at a time: the daemon hashes and inspects each file as it arrives.
    chain.current = chain.current.then(async () => {
      let stored = 0;
      for (const job of jobs) {
        update(job.key, { state: "sending" });
        try {
          await upload<FileRecord>(`${base}/${filePath(job.target)}`, job.file, {}, (sent) =>
            update(job.key, { sent }),
          );
          update(job.key, { state: "done", sent: job.file.size });
          stored += 1;
        } catch (error) {
          update(job.key, { state: "failed", error: message(error) });
        }
      }
      refresh();
      if (stored > 0) {
        toast.success(plural(stored, "{count} file uploaded", "{count} files uploaded"));
      }
    });
  };

  const onDrop = (event: DragEvent) => {
    event.preventDefault();
    depth.current = 0;
    setDragging(false);
    collect(event.dataTransfer).then(enqueue, fail);
  };

  const all = useMemo(() => files.data ?? [], [files.data]);
  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return all.filter(
      (file) =>
        (side === "all" || file.side === side) &&
        (kind === "all" || file.kind === kind) &&
        (needle === "" ||
          file.path.toLowerCase().includes(needle) ||
          (file.mod_name?.toLowerCase().includes(needle) ?? false)),
    );
  }, [all, search, side, kind]);
  const unknown = all.filter((file) => file.side === "unknown").length;
  const total = all.reduce((sum, file) => sum + file.size, 0);
  const active = transfers.filter((item) => item.state !== "done");

  return (
    <Tile
      className={cn("gap-4 transition-colors", dragging && "border-primary bg-primary/5")}
      onDragEnter={(event) => {
        if (!event.dataTransfer.types.includes("Files")) return;
        depth.current += 1;
        setDragging(true);
      }}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes("Files")) event.preventDefault();
      }}
      onDragLeave={() => {
        depth.current = Math.max(0, depth.current - 1);
        if (depth.current === 0) setDragging(false);
      }}
      onDrop={onDrop}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-semibold">{t("Modpack files")}</h2>
          <p className="text-sm text-muted-foreground">
            {plural(all.length, "{count} file, {size}", "{count} files, {size}", {
              size: bytes(total),
            })}
            <span className="mx-1.5">·</span>
            {t("build")} <span className="font-mono">{instance.build_id.slice(0, 8)}</span>
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Select value={destination} onValueChange={setDestination}>
            <SelectTrigger aria-label={t("Upload destination")} className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DESTINATIONS.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {t(item.label)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <input
            ref={picker}
            type="file"
            multiple
            hidden
            onChange={(event) => {
              enqueue([...(event.target.files ?? [])].map((file) => ({ file, relative: file.name })));
              event.target.value = "";
            }}
          />
          <Button onClick={() => picker.current?.click()}>
            <Icon name="upload" />
            {t("Upload")}
          </Button>
        </div>
      </div>

      {unknown > 0 && (
        <button
          type="button"
          onClick={() => {
            setSide("unknown");
            setKind("all");
            setSearch("");
          }}
          className="flex items-center gap-2 rounded-xl border border-warn/40 bg-warn/10 px-3 py-2 text-left text-sm outline-none hover:bg-warn/15 focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Icon name="alert" className="text-warn" />
          <span>
            <span className="font-medium">
              {plural(
                unknown,
                "{count} file has no side yet.",
                "{count} files have no side yet.",
              )}
            </span>{" "}
            {t("Until you pick one, they reach neither the server nor players. Show them")}
          </span>
        </button>
      )}

      {active.length > 0 && (
        <ul className="grid gap-2 rounded-xl border p-3">
          {active.map((item) => (
            <li key={item.key} className="grid gap-1 text-sm">
              <div className="flex items-center justify-between gap-3">
                <span className="truncate font-mono text-xs">{item.target}</span>
                <span
                  className={cn(
                    "shrink-0 text-xs",
                    item.state === "failed" ? "text-destructive" : "text-muted-foreground",
                  )}
                >
                  {item.state === "failed"
                    ? item.error
                    : item.state === "waiting"
                      ? t("Waiting")
                      : t("{sent} of {size}", { sent: bytes(item.sent), size: bytes(item.size) })}
                </span>
              </div>
              {item.state !== "failed" && (
                <Progress value={item.size ? (item.sent / item.size) * 100 : 0} className="h-1" />
              )}
            </li>
          ))}
          {active.every((item) => item.state === "failed") && (
            <li>
              <Button variant="ghost" size="xs" onClick={() => setTransfers([])}>
                <Icon name="x" />
                {t("Dismiss")}
              </Button>
            </li>
          )}
        </ul>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-48 flex-1">
          <Icon
            name="search"
            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("Filter by name or path")}
            aria-label={t("Filter files")}
            className="pl-8"
          />
        </div>
        <Select value={kind} onValueChange={(next) => setKind(next as FileKind | "all")}>
          <SelectTrigger aria-label={t("Kind")} className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("All kinds")}</SelectItem>
            {(Object.keys(KIND_NAMES) as FileKind[]).map((value) => (
              <SelectItem key={value} value={value}>
                {t(KIND_NAMES[value])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={side} onValueChange={(next) => setSide(next as Side | "all")}>
          <SelectTrigger aria-label={t("Side")} className="w-36">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("All sides")}</SelectItem>
            {(Object.keys(SIDE_NAMES) as Side[]).map((value) => (
              <SelectItem key={value} value={value}>
                {t(SIDE_NAMES[value])}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {files.isPending ? (
        <Loading />
      ) : files.error ? (
        <ErrorNote error={files.error} />
      ) : all.length === 0 ? (
        <Empty icon="upload-cloud" title={t("Drop mods, configs or whole folders here")}>
          {t(
            "Jars go to mods/, archives to resourcepacks/, config files to config/. A dropped folder keeps its structure.",
          )}
        </Empty>
      ) : shown.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted-foreground">
          {t("Nothing matches the filter.")}
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("File")}</TableHead>
              <TableHead className="text-right">{t("Size")}</TableHead>
              <TableHead>{t("Side")}</TableHead>
              <TableHead>{t("Updates")}</TableHead>
              <TableHead>{t("Optional ## file column")}</TableHead>
              <TableHead className="w-0" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((file) => {
              const name = file.path.slice(file.path.lastIndexOf("/") + 1);
              const overridden = file.side_source === "override";
              return (
                <TableRow key={file.path}>
                  <TableCell className="max-w-0 w-full">
                    <div className="flex items-start gap-3">
                      {file.icon ? (
                        <img
                          src={`/api/v1/instances/${instance.id}/blobs/${file.icon}`}
                          alt=""
                          loading="lazy"
                          className="size-9 shrink-0 bg-muted object-cover [image-rendering:pixelated]"
                        />
                      ) : (
                        <span className="grid size-9 shrink-0 place-items-center bg-muted text-muted-foreground">
                          <Icon name={file.kind === "mod" ? "mods" : "files"} />
                        </span>
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate font-medium">{file.mod_name ?? name}</span>
                          {file.mod_version && (
                            <Badge variant="secondary" className="shrink-0 font-mono">
                              {file.mod_version}
                            </Badge>
                          )}
                        </div>
                        {file.description && (
                          <p className="truncate text-xs text-muted-foreground">{file.description}</p>
                        )}
                        <p className="truncate font-mono text-xs text-muted-foreground">{file.path}</p>
                        {file.requires.length > 0 && (
                          <p className="truncate text-xs text-muted-foreground">
                            {t("Needs {list}", { list: file.requires.join(", ") })}
                          </p>
                        )}
                        {file.missing.length > 0 && (
                          <p className="flex items-start gap-1.5 text-xs text-warn">
                            <Icon name="alert" size={12} className="mt-0.5 shrink-0" />
                            <span className="text-foreground">
                              {t("Not in the modpack: {list}. The game will not start without it.", {
                                list: file.missing.join(", "),
                              })}
                            </span>
                          </p>
                        )}
                      </div>
                    </div>
                  </TableCell>
                  <TableCell className="text-right font-mono tabular">{bytes(file.size)}</TableCell>
                  <TableCell>
                    <div title={file.detect_reason || undefined}>
                      <Select
                        value={file.side === "unknown" ? undefined : file.side}
                        onValueChange={(next) =>
                          setSideOverride.mutate({
                            sha256: file.sha256,
                            side: next === "auto" ? null : (next as Side),
                          })
                        }
                      >
                        <SelectTrigger
                          size="sm"
                          aria-label={t("Side of {name}", { name })}
                          className={cn(
                            "w-32",
                            file.side === "unknown" && "border-warn/60 text-warn",
                          )}
                        >
                          <SelectValue placeholder={t("Classify")} />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="both">{t("Both")}</SelectItem>
                          <SelectItem value="client">{t("Client")}</SelectItem>
                          <SelectItem value="server">{t("Server")}</SelectItem>
                          {overridden && (
                            <>
                              <SelectSeparator />
                              <SelectItem value="auto">
                                {t("Detected: {side}", {
                                  side: t(SIDE_NAMES[file.detected_side]).toLowerCase(),
                                })}
                              </SelectItem>
                            </>
                          )}
                        </SelectContent>
                      </Select>
                    </div>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {overridden
                        ? t("Set by hand")
                        : file.detect_confidence === "declared"
                          ? t("Declared by the mod")
                          : file.detect_confidence === "heuristic"
                            ? t("Best guess")
                            : file.kind === "mod"
                              ? t("Not detected")
                              : t("Default")}
                    </p>
                  </TableCell>
                  <TableCell>
                    <Select
                      value={file.sync_action}
                      onValueChange={(next) =>
                        patch.mutate({ path: file.path, body: { sync_action: next as SyncAction } })
                      }
                    >
                      <SelectTrigger
                        size="sm"
                        aria-label={t("Updates of {name}", { name })}
                        className="w-36"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="enforce">{t("Always match")}</SelectItem>
                        <SelectItem value="sync_once">{t("Copy once")}</SelectItem>
                      </SelectContent>
                    </Select>
                  </TableCell>
                  <TableCell>
                    <Switch
                      checked={file.optional}
                      aria-label={t("{name} is optional", { name })}
                      disabled={file.side === "server"}
                      onCheckedChange={(optional) =>
                        patch.mutate({ path: file.path, body: { optional } })
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t("Delete {name}", { name })}
                      onClick={() => setDeleting(file)}
                    >
                      <Icon name="delete" />
                    </Button>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}

      {all.length > 0 && (
        <p className="text-xs text-muted-foreground">
          {t(
            "“Copy once” files reach a player on first install and are never overwritten after that, which suits configs players tune themselves. Optional files can be switched off in the launcher.",
          )}
        </p>
      )}

      {dragging && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center bg-background/70">
          <div className="flex items-center gap-2 border border-primary bg-card px-4 py-3 font-medium">
            <Icon name="upload-cloud" active className="text-primary" />
            {t("Drop to upload")}
          </div>
        </div>
      )}

      <Confirm
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("Remove this file?")}
        description={
          deleting &&
          t(
            "{path} leaves the modpack. Launchers delete it from players' installs on the next sync.",
            { path: deleting.path },
          )
        }
        action={t("Remove")}
        destructive
        onConfirm={() => deleting && remove.mutate(deleting.path)}
      />
    </Tile>
  );
}
