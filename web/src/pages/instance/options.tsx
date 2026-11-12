import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { ActionButton, ErrorNote, Help, Loading, Tile, TileLabel } from "@/components/common";
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
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { api, message } from "@/lib/api";
import { contentUrl, parseProperties, patchProperties, readText, useText, writeText } from "@/lib/fs";
import { useI18n } from "@/lib/i18n";
import {
  GROUP_NAMES,
  MOTD_KEY,
  PROPERTIES,
  type PropertyGroup,
  type PropertySpec,
} from "@/lib/properties";
import { keys, useFolder, usePower } from "@/lib/queries";
import type { Instance } from "@/lib/types";
import { cn } from "@/lib/utils";

const FILE = "server.properties";
const ICON = "server-icon.png";
const ICON_SIDE = 64;

// --- Message of the day ----------------------------------------------------

const COLOURS: [string, string, string][] = [
  ["0", "#000000", "Black"],
  ["1", "#0000AA", "Dark blue"],
  ["2", "#00AA00", "Dark green"],
  ["3", "#00AAAA", "Dark aqua"],
  ["4", "#AA0000", "Dark red"],
  ["5", "#AA00AA", "Purple"],
  ["6", "#FFAA00", "Gold"],
  ["7", "#AAAAAA", "Grey"],
  ["8", "#555555", "Dark grey"],
  ["9", "#5555FF", "Blue"],
  ["a", "#55FF55", "Green"],
  ["b", "#55FFFF", "Aqua"],
  ["c", "#FF5555", "Red"],
  ["d", "#FF55FF", "Pink"],
  ["e", "#FFFF55", "Yellow"],
  ["f", "#FFFFFF", "White"],
];

const STYLES: [string, string, string][] = [
  ["l", "Bold", "font-bold"],
  ["o", "Italic", "italic"],
  ["n", "Underline", "underline"],
  ["m", "Strikethrough", "line-through"],
];

interface Run {
  text: string;
  colour: string;
  styles: string[];
}

/** The message as the game draws it: `§` codes turned into colours and styles. */
function runs(line: string): Run[] {
  const out: Run[] = [];
  let colour = "#AAAAAA";
  let styles: string[] = [];
  let text = "";
  const flush = () => {
    if (text) out.push({ text, colour, styles });
    text = "";
  };
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index]!;
    const code = line[index + 1]?.toLowerCase();
    if (char !== "§" || code === undefined) {
      text += char;
      continue;
    }
    index += 1;
    const hue = COLOURS.find(([key]) => key === code);
    const style = STYLES.find(([key]) => key === code);
    flush();
    if (hue) {
      colour = hue[1];
      styles = [];
    } else if (style) {
      styles = [...styles, style[2]];
    } else if (code === "r") {
      colour = "#AAAAAA";
      styles = [];
    }
  }
  flush();
  return out;
}

function Motd({
  instance,
  value,
  icon,
  onChange,
}: {
  instance: Instance;
  value: string;
  icon: string | null;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const area = useRef<HTMLTextAreaElement>(null);

  /** Puts a code where the cursor is and leaves the cursor after it. */
  const insert = (code: string) => {
    const field = area.current;
    const at = field?.selectionStart ?? value.length;
    const end = field?.selectionEnd ?? value.length;
    onChange(`${value.slice(0, at)}§${code}${value.slice(end)}`);
    requestAnimationFrame(() => {
      field?.focus();
      field?.setSelectionRange(at + 2, at + 2);
    });
  };

  const lines = value.split("\n").slice(0, 2);

  return (
    <Tile className="gap-4 lg:col-span-2">
      <div className="grid gap-1">
        <TileLabel icon="news">{t("Message of the day")}</TileLabel>
        <p className="text-sm text-muted-foreground">
          {t("The two lines players see under the server's name in their server list.")}
        </p>
      </div>

      <div className="flex gap-3 bg-console p-3 text-console-foreground">
        <div className="grid size-16 shrink-0 place-items-center bg-black/40">
          {icon ? (
            <img src={icon} alt="" className="size-16 [image-rendering:pixelated]" />
          ) : (
            <Icon name="server" size={24} className="opacity-50" />
          )}
        </div>
        <div className="grid min-w-0 flex-1 content-start gap-1.5 font-pixel text-[10px] leading-4">
          <p className="truncate text-white">{instance.name}</p>
          {lines.map((line, index) => (
            <p key={index} className="min-h-4 truncate whitespace-pre">
              {runs(line).map((run, part) => (
                <span key={part} style={{ color: run.colour }} className={run.styles.join(" ")}>
                  {run.text}
                </span>
              ))}
            </p>
          ))}
        </div>
      </div>

      <Textarea
        ref={area}
        value={value}
        rows={2}
        spellCheck={false}
        aria-label={t("Message of the day")}
        onChange={(event) => onChange(event.target.value.split("\n").slice(0, 2).join("\n"))}
        className="min-h-0 resize-none font-mono"
      />

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div className="flex flex-wrap gap-1" role="group" aria-label={t("Colour")}>
          {COLOURS.map(([code, hex, name]) => (
            <button
              key={code}
              type="button"
              title={t(name)}
              aria-label={t(name)}
              onClick={() => insert(code)}
              style={{ backgroundColor: hex }}
              className="size-6 cursor-pointer border outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
          ))}
        </div>
        <div className="flex flex-wrap gap-1">
          {STYLES.map(([code, name, style]) => (
            <Button key={code} variant="outline" size="xs" onClick={() => insert(code)}>
              <span className={style}>{t(name)}</span>
            </Button>
          ))}
          <Button variant="outline" size="xs" onClick={() => insert("r")}>
            {t("Reset style")}
          </Button>
        </div>
      </div>
    </Tile>
  );
}

// --- Server icon -----------------------------------------------------------

/** Any picture as the 64 by 64 PNG the game accepts, cropped to its middle. */
async function toIcon(file: File): Promise<Blob> {
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement("canvas");
  canvas.width = ICON_SIDE;
  canvas.height = ICON_SIDE;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("The browser cannot draw the picture");
  const side = Math.min(bitmap.width, bitmap.height);
  // Small pixel art stays sharp; photos are smoothed down.
  context.imageSmoothingEnabled = side > ICON_SIDE;
  context.imageSmoothingQuality = "high";
  context.drawImage(
    bitmap,
    (bitmap.width - side) / 2,
    (bitmap.height - side) / 2,
    side,
    side,
    0,
    0,
    ICON_SIDE,
    ICON_SIDE,
  );
  return new Promise((resolve, reject) =>
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(new Error("The picture could not be converted"))),
      "image/png",
    ),
  );
}

function ServerIcon({ instance, icon }: { instance: Instance; icon: string | null }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const picker = useRef<HTMLInputElement>(null);
  const done = () => client.invalidateQueries({ queryKey: keys.fs(instance.id, "") });

  const put = useMutation({
    mutationFn: async (file: File) =>
      api("PUT", `/admin/instances/${instance.id}/fs/content`, {
        raw: await toIcon(file),
        query: { path: ICON },
      }),
    onSuccess: () => {
      toast.success(t("Icon saved. Players see it after the next start."));
      return done();
    },
    onError: (error) => toast.error(message(error)),
  });

  const remove = useMutation({
    mutationFn: () => api("DELETE", `/admin/instances/${instance.id}/fs`, { query: { path: ICON } }),
    onSuccess: done,
    onError: (error) => toast.error(message(error)),
  });

  return (
    <Tile className="gap-4">
      <div className="grid gap-1">
        <TileLabel icon="palette">{t("Server icon")}</TileLabel>
        <p className="text-sm text-muted-foreground">
          {t("Shown beside the server in the list. Any picture works: it is cropped to a square and scaled to 64 by 64.")}
        </p>
      </div>
      <div className="flex items-center gap-4">
        <div className="grid size-24 shrink-0 place-items-center border bg-console text-console-foreground">
          {icon ? (
            <img src={icon} alt={t("Current icon")} className="size-24 [image-rendering:pixelated]" />
          ) : (
            <Icon name="server" size={28} className="opacity-50" />
          )}
        </div>
        <div className="grid gap-2">
          <input
            ref={picker}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file) put.mutate(file);
            }}
          />
          <ActionButton
            icon="upload"
            variant="secondary"
            busy={put.isPending}
            onClick={() => picker.current?.click()}
          >
            {icon ? t("Replace") : t("Choose a picture")}
          </ActionButton>
          {icon && (
            <ActionButton
              icon="delete"
              variant="ghost"
              busy={remove.isPending}
              onClick={() => remove.mutate()}
            >
              {t("Remove")}
            </ActionButton>
          )}
        </div>
      </div>
    </Tile>
  );
}

// --- Settings --------------------------------------------------------------

type Group = PropertyGroup | "other";
type Spec = Omit<PropertySpec, "group"> & { group: Group };

function valid(spec: Spec, value: string): boolean {
  if (spec.kind.type !== "number") return true;
  if (!/^-?\d+$/.test(value.trim())) return false;
  const number = Number(value);
  return number >= spec.kind.min && number <= spec.kind.max;
}

function Control({
  spec,
  value,
  onChange,
}: {
  spec: Spec;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const label = t(spec.title);
  const locked = spec.managed !== undefined;

  switch (spec.kind.type) {
    case "bool":
      return (
        <Switch
          checked={value === "true"}
          disabled={locked}
          aria-label={label}
          onCheckedChange={(next) => onChange(String(next))}
        />
      );
    case "choice": {
      const known = spec.kind.options.some((option) => option.value === value);
      return (
        <Select value={value} disabled={locked} onValueChange={onChange}>
          <SelectTrigger aria-label={label} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {!known && <SelectItem value={value}>{value}</SelectItem>}
            {spec.kind.options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {t(option.label)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    }
    case "number":
      return (
        <Input
          type="number"
          inputMode="numeric"
          min={spec.kind.min}
          max={spec.kind.max}
          value={value}
          disabled={locked}
          aria-label={label}
          aria-invalid={!valid(spec, value)}
          onChange={(event) => onChange(event.target.value)}
          className="font-mono tabular"
        />
      );
    default:
      return (
        <Input
          value={value}
          disabled={locked}
          aria-label={label}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
          className="font-mono"
        />
      );
  }
}

function PropertyTile({
  spec,
  value,
  changed,
  onChange,
}: {
  spec: Spec;
  value: string;
  changed: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const control = <Control spec={spec} value={value} onChange={onChange} />;
  const inline = spec.kind.type === "bool";
  const shown =
    spec.kind.type === "choice"
      ? (spec.kind.options.find((option) => option.value === spec.fallback)?.label ?? spec.fallback)
      : spec.fallback;

  return (
    <Tile className={cn("gap-2 p-4", changed && "shadow-[inset_2px_0_0_var(--primary)]")}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="font-medium">{t(spec.title)}</h3>
          <p className="truncate font-mono text-xs text-muted-foreground">{spec.key}</p>
        </div>
        {inline && control}
      </div>
      {spec.text && <p className="text-sm text-muted-foreground">{t(spec.text)}</p>}
      <div className="mt-auto grid gap-2 pt-1">
        {!inline && control}
        {spec.kind.type === "number" && !valid(spec, value) && (
          <p className="text-xs text-destructive">
            {t("A whole number from {min} to {max}", { min: spec.kind.min, max: spec.kind.max })}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          {spec.managed
            ? t(spec.managed)
            : spec.kind.type === "bool"
              ? t("Default: {value}", { value: spec.fallback === "true" ? t("on") : t("off") })
              : spec.fallback === ""
                ? t("Default: empty")
                : t("Default: {value}", {
                    value: spec.kind.type === "choice" ? t(shown) : shown,
                  })}
        </p>
      </div>
    </Tile>
  );
}

function FilterButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "h-8 cursor-pointer px-3 text-sm whitespace-nowrap text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
        active && "bg-secondary text-foreground",
      )}
    >
      {children}
    </button>
  );
}

export function OptionsTab({ instance }: { instance: Instance }) {
  const { t, plural } = useI18n();
  const client = useQueryClient();
  const file = useText(instance.id, FILE);
  const root = useFolder(instance.id, "");
  const power = usePower(instance.id);
  const [draft, setDraft] = useState<Map<string, string>>(new Map());
  const [search, setSearch] = useState("");
  const [group, setGroup] = useState<Group | "all">("all");

  const stored = useMemo(() => parseProperties(file.data), [file.data]);

  /** Everything the page can show: the known settings, then whatever else the file has. */
  const specs = useMemo(() => {
    const known = new Set(PROPERTIES.map((spec) => spec.key));
    const extra: Spec[] = [...stored.keys()]
      .filter((key) => !known.has(key) && key !== MOTD_KEY)
      .sort()
      .map((key) => ({
        key,
        group: "other" as const,
        title: key,
        text: "",
        fallback: "",
        kind: { type: "text" as const },
      }));
    return [...(PROPERTIES as Spec[]), ...extra];
  }, [stored]);

  const save = useSave(instance.id, () => {
    setDraft(new Map());
    return client.invalidateQueries({ queryKey: keys.fsText(instance.id, FILE) });
  });

  if (file.isPending) return <Loading />;
  if (file.error) return <ErrorNote error={file.error} />;

  const current = (key: string, fallback: string) => draft.get(key) ?? stored.get(key) ?? fallback;
  const change = (key: string, fallback: string) => (value: string) =>
    setDraft((before) => {
      const next = new Map(before);
      if (value === (stored.get(key) ?? fallback)) next.delete(key);
      else next.set(key, value);
      return next;
    });

  const entry = root.data?.find((item) => item.name === ICON && !item.dir);
  const icon = entry ? `${contentUrl(instance.id, ICON)}&v=${entry.modified}` : null;

  const needle = search.trim().toLowerCase();
  const shown = specs.filter(
    (spec) =>
      (group === "all" || spec.group === group) &&
      (needle === "" ||
        spec.key.includes(needle) ||
        t(spec.title).toLowerCase().includes(needle) ||
        (spec.text !== "" && t(spec.text).toLowerCase().includes(needle))),
  );
  const groups: Group[] = [
    ...(Object.keys(GROUP_NAMES) as PropertyGroup[]),
    ...(specs.some((spec) => spec.group === "other") ? (["other"] as const) : []),
  ];
  const groupName = (id: Group) => (id === "other" ? t("Other") : t(GROUP_NAMES[id]));

  const broken = specs.some((spec) => draft.has(spec.key) && !valid(spec, draft.get(spec.key)!));
  const running = instance.runtime.status === "running";

  const submit = (restart: boolean) =>
    save.mutate(draft, {
      onSuccess: () => {
        if (restart) power.mutate("restart");
        else {
          toast.success(
            running ? t("Saved. The server reads it at the next start.") : t("Saved"),
          );
        }
      },
    });

  return (
    <>
      <div className="grid gap-(--frame) lg:grid-cols-3">
        <Motd
          instance={instance}
          icon={icon}
          value={current(MOTD_KEY, "A Minecraft Server")}
          onChange={change(MOTD_KEY, "A Minecraft Server")}
        />
        <ServerIcon instance={instance} icon={icon} />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 bg-background px-5 py-3">
        <div className="relative w-full max-w-xs">
          <Icon
            name="search"
            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            type="search"
            value={search}
            placeholder={t("Search settings")}
            aria-label={t("Search settings")}
            onChange={(event) => setSearch(event.target.value)}
            className="pl-8"
          />
        </div>
        <div className="flex flex-wrap gap-1">
          <FilterButton active={group === "all"} onClick={() => setGroup("all")}>
            {t("All")}
          </FilterButton>
          {groups.map((id) => (
            <FilterButton key={id} active={group === id} onClick={() => setGroup(id)}>
              {groupName(id)}
            </FilterButton>
          ))}
        </div>
      </div>

      {file.data === null && (
        <p className="bg-background px-5 py-3 text-sm text-muted-foreground">
          {t(
            "The server has not started yet, so it has no settings file. What you save here is what it starts with.",
          )}
        </p>
      )}

      {shown.length === 0 ? (
        <p className="bg-background px-5 py-10 text-center text-sm text-muted-foreground">
          {t("No setting matches “{query}”", { query: search.trim() })}
        </p>
      ) : (
        groups
          .filter((id) => shown.some((spec) => spec.group === id))
          .map((id) => (
            <section key={id} className="grid gap-(--frame)">
              <h2 className="bg-background px-5 py-2.5 font-pixel text-xs leading-5">
                {groupName(id)}
              </h2>
              <div className="grid gap-(--frame) sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
                {shown
                  .filter((spec) => spec.group === id)
                  .map((spec) => (
                    <PropertyTile
                      key={spec.key}
                      spec={spec}
                      value={current(spec.key, spec.fallback)}
                      changed={draft.has(spec.key)}
                      onChange={change(spec.key, spec.fallback)}
                    />
                  ))}
              </div>
            </section>
          ))
      )}

      <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 border-t bg-background px-5 py-3 shadow-[0_calc(var(--frame)*-1)_0_var(--canvas)]">
        <p className="text-sm text-muted-foreground">
          {draft.size === 0
            ? t("No unsaved changes")
            : plural(draft.size, "{count} unsaved change", "{count} unsaved changes")}
          {broken && <span className="ml-2 text-destructive">{t("Fix the marked values first")}</span>}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button variant="ghost" disabled={draft.size === 0} onClick={() => setDraft(new Map())}>
            {t("Discard")}
          </Button>
          <ActionButton
            icon="check"
            variant={running ? "secondary" : "default"}
            busy={save.isPending}
            disabled={draft.size === 0 || broken}
            onClick={() => submit(false)}
          >
            {t("Save")}
          </ActionButton>
          {running && (
            <ActionButton
              icon="restart"
              busy={save.isPending || power.isPending}
              disabled={draft.size === 0 || broken}
              onClick={() => submit(true)}
            >
              {t("Save and restart")}
            </ActionButton>
          )}
        </div>
      </div>

      <Help>
        <p>
          {t(
            "These are the settings of server.properties. The server reads the file only when it starts, so a change made while it runs waits for a restart.",
          )}
        </p>
        <p>
          {t(
            "Only the settings you change are rewritten; everything else in the file stays as it was. Settings that mods add appear under Other.",
          )}
        </p>
      </Help>
    </>
  );
}

function useSave(id: string, done: () => Promise<void>) {
  return useMutation({
    mutationFn: async (changes: Map<string, string>) => {
      // Read again: the server may have rewritten the file since the page loaded.
      const text = await readText(id, FILE);
      await writeText(id, FILE, patchProperties(text, changes));
    },
    onSuccess: done,
    onError: (error) => toast.error(message(error)),
  });
}
