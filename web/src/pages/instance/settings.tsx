import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";
import { useLocation } from "wouter";

import { ActionButton, Confirm, Field, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { LoaderFields, type Platform } from "@/components/loader-fields";
import { MediaField } from "@/components/media-field";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { halted } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useAcceptEula, useUpdateInstance } from "@/lib/queries";
import type { Instance, InstanceUpdate, ModPolicy, Scene } from "@/lib/types";
import { cn } from "@/lib/utils";

const POLICIES: { value: ModPolicy; title: string; text: string }[] = [
  {
    value: "strict",
    title: "Strict",
    text: "Players run exactly this modpack. Any extra jar in their mods folder is removed.",
  },
  {
    value: "optional",
    title: "Optional",
    text: "Players may switch the files you mark optional on or off. Nothing else is allowed.",
  },
  {
    value: "permissive",
    title: "Permissive",
    text: "Players may add their own client mods. The launcher leaves unknown files alone.",
  },
];

const LOCKS: { dir: string; title: string; text: string }[] = [
  {
    dir: "resourcepacks",
    title: "Only the modpack's resource packs",
    text: "The launcher removes any other pack from resourcepacks before each game. Keeps out see-through texture packs.",
  },
  {
    dir: "shaderpacks",
    title: "Only the modpack's shaders",
    text: "The launcher removes any other shader pack from shaderpacks before each game.",
  },
];

const OWNED = ["options.txt", "optionsof.txt", "optionsshaders.txt", "servers.dat"];

const JAVA = [8, 11, 17, 21, 25];

const SCENES: { value: Scene; title: string }[] = [
  { value: "mine", title: "Mine shaft" },
  { value: "deep", title: "Deepslate" },
  { value: "forest", title: "Forest" },
  { value: "desert", title: "Desert" },
  { value: "snow", title: "Snow" },
];

export function InstanceSettingsTab({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [, navigate] = useLocation();
  const update = useUpdateInstance(instance.id);
  const eula = useAcceptEula(instance.id);

  const [name, setName] = useState(instance.name);
  const [description, setDescription] = useState(instance.description);
  const [platform, setPlatform] = useState<Platform>({
    minecraft: instance.minecraft_version,
    kind: instance.loader.type,
    loaderVersion: instance.loader.version,
  });
  const [java, setJava] = useState(String(instance.java_major));
  const [port, setPort] = useState(String(instance.port));
  const [memory, setMemory] = useState(String(instance.memory_mb));
  const [policy, setPolicy] = useState(instance.mod_policy);
  const [locked, setLocked] = useState<string[]>(instance.locked_dirs ?? []);
  const [autoStart, setAutoStart] = useState(instance.auto_start);
  const [autoRestart, setAutoRestart] = useState(instance.auto_restart);
  const [scene, setScene] = useState<Scene>(instance.scene ?? "mine");
  const [banner, setBanner] = useState(instance.banner_url ?? "");
  const [confirming, setConfirming] = useState(false);
  const [purge, setPurge] = useState(false);

  const platformChanged =
    platform.minecraft !== instance.minecraft_version ||
    platform.kind !== instance.loader.type ||
    platform.loaderVersion !== instance.loader.version;

  const change: InstanceUpdate = {};
  if (name.trim() !== instance.name) change.name = name.trim();
  if (description.trim() !== instance.description) change.description = description.trim();
  if (platform.minecraft !== instance.minecraft_version) {
    change.minecraft_version = platform.minecraft;
  }
  if (platformChanged) {
    change.loader = { type: platform.kind, version: platform.loaderVersion || undefined };
  }
  if (Number(java) !== instance.java_major) change.java_major = Number(java);
  if (Number(port) !== instance.port) change.port = Number(port);
  if (Number(memory) !== instance.memory_mb) change.memory_mb = Number(memory);
  if (policy !== instance.mod_policy) change.mod_policy = policy;
  if ([...locked].sort().join() !== [...(instance.locked_dirs ?? [])].sort().join()) {
    change.locked_dirs = locked;
  }
  if (autoStart !== instance.auto_start) change.auto_start = autoStart;
  if (autoRestart !== instance.auto_restart) change.auto_restart = autoRestart;
  if (scene !== (instance.scene ?? "mine")) change.scene = scene;
  if (banner.trim() !== (instance.banner_url ?? "")) change.banner_url = banner.trim();
  const dirty = Object.keys(change).length > 0;

  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!dirty) return;
    update.mutate(change, {
      onSuccess: (saved) => {
        // The daemon resolves "newest stable" to a concrete build.
        setPlatform({
          minecraft: saved.minecraft_version,
          kind: saved.loader.type,
          loaderVersion: saved.loader.version,
        });
        setJava(String(saved.java_major));
        toast.success(t("Settings saved"));
      },
    });
  };

  const remove = useMutation({
    mutationFn: () => api("DELETE", `/admin/instances/${instance.id}`, { query: { purge } }),
    onSuccess: async () => {
      toast.success(t("{name} deleted", { name: instance.name }));
      navigate("/");
      await client.invalidateQueries({ queryKey: keys.instances });
    },
    onError: (error) => toast.error(message(error)),
  });

  const stopped = halted(instance.runtime.status);

  return (
    <form onSubmit={save} className="grid gap-(--frame) md:grid-cols-6">
      <Tile className="gap-4 md:col-span-3">
        <h2 className="font-semibold">{t("Identity")}</h2>
        <Field label={t("Name")} htmlFor="name">
          <Input
            id="name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            maxLength={64}
            required
          />
        </Field>
        <Field label={t("Description")} htmlFor="description">
          <Textarea
            id="description"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={3}
            maxLength={500}
          />
        </Field>
        <Field label={t("Id")}>
          <p className="font-mono text-sm text-muted-foreground">{instance.id}</p>
        </Field>
      </Tile>

      <Tile className="gap-4 md:col-span-3">
        <h2 className="font-semibold">{t("Platform")}</h2>
        <LoaderFields value={platform} onChange={setPlatform} />
        {platformChanged && (
          <p className="flex items-start gap-2 text-sm text-warn">
            <Icon name="alert" className="mt-0.5" appear />
            <span className="text-foreground">
              {t(
                "The server reinstalls on its next start, and players download the new client. Back up the world first: older versions cannot open a newer world.",
              )}
            </span>
          </p>
        )}
      </Tile>

      <Tile className="gap-4 md:col-span-3">
        <h2 className="font-semibold">{t("Resources")}</h2>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label={t("Port")} htmlFor="port">
            <Input
              id="port"
              type="number"
              inputMode="numeric"
              min={1024}
              max={65535}
              value={port}
              onChange={(event) => setPort(event.target.value)}
              className="font-mono"
              required
            />
          </Field>
          <Field label={t("Memory, MB")} htmlFor="memory">
            <Input
              id="memory"
              type="number"
              inputMode="numeric"
              min={512}
              step={256}
              value={memory}
              onChange={(event) => setMemory(event.target.value)}
              className="font-mono"
              required
            />
          </Field>
          <Field label="Java">
            <Select value={java} onValueChange={setJava}>
              <SelectTrigger className="w-full" aria-label={t("Java version")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {[...new Set([...JAVA, instance.java_major])]
                  .sort((a, b) => a - b)
                  .map((major) => (
                    <SelectItem key={major} value={String(major)}>
                      Java {major}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
        <p className="text-xs text-muted-foreground">
          {t(
            "Port, memory and Java apply on the next start. The runtime is downloaded automatically.",
          )}
        </p>
        <div className="grid gap-3 border-t pt-4">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="auto-start" className="grid gap-0.5 font-normal">
              <span className="font-medium">{t("Start with the daemon")}</span>
              <span className="text-xs text-muted-foreground">
                {t("Bring this server up whenever Relic itself starts.")}
              </span>
            </Label>
            <Switch id="auto-start" checked={autoStart} onCheckedChange={setAutoStart} />
          </div>
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="auto-restart" className="grid gap-0.5 font-normal">
              <span className="font-medium">{t("Restart after a crash")}</span>
              <span className="text-xs text-muted-foreground">
                {t("Retries with a growing delay, and gives up if crashes keep coming.")}
              </span>
            </Label>
            <Switch id="auto-restart" checked={autoRestart} onCheckedChange={setAutoRestart} />
          </div>
        </div>
      </Tile>

      <Tile className="gap-4 md:col-span-3">
        <div>
          <h2 className="font-semibold">{t("Look in the launcher")}</h2>
          <p className="text-sm text-muted-foreground">
            {t("What players see behind this modpack. Launchers pick the change up within a minute.")}
          </p>
        </div>
        <Field
          label={t("Animated scene")}
          hint={t("Drawn by the launcher itself. Shown when there is no banner.")}
        >
          <Select value={scene} onValueChange={(value) => setScene(value as Scene)}>
            <SelectTrigger className="w-full" aria-label={t("Animated scene")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SCENES.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {t(item.title)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <MediaField
          id="banner"
          label={t("Banner")}
          hint={t("PNG, JPEG, WebP or an animated GIF, up to 16 MB. The launcher does not play video.")}
          value={banner}
          onChange={setBanner}
        />
      </Tile>

      <Tile className="gap-4 md:col-span-6">
        <div>
          <h2 className="font-semibold">{t("Sync")}</h2>
          <p className="text-sm text-muted-foreground">
            {t("What the launcher keeps identical to the modpack, and what stays the player's own.")}
          </p>
        </div>
        <div className="grid gap-x-8 gap-y-5 lg:grid-cols-2">
        <div className="grid content-start gap-2">
        <h3 className="text-sm font-medium">{t("Mods players may run")}</h3>
        <div role="radiogroup" aria-label={t("Mod policy")} className="grid gap-2">
          {POLICIES.map((item) => {
            const chosen = policy === item.value;
            return (
              <button
                key={item.value}
                type="button"
                role="radio"
                aria-checked={chosen}
                onClick={() => setPolicy(item.value)}
                className={cn(
                  "flex items-start gap-3 border p-3 text-left outline-none transition-colors hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring",
                  chosen && "border-primary bg-primary/5",
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    "mt-1 grid size-4 shrink-0 place-items-center border",
                    chosen && "border-primary",
                  )}
                >
                  {chosen && <span className="size-2 bg-primary" />}
                </span>
                <span>
                  <span className="block text-sm font-medium">{t(item.title)}</span>
                  <span className="block text-sm text-muted-foreground">{t(item.text)}</span>
                </span>
              </button>
            );
          })}
        </div>
        </div>
        <div className="grid content-start gap-4">
          <h3 className="text-sm font-medium">{t("Folders besides mods")}</h3>
          {LOCKS.map((item) => (
            <div key={item.dir} className="flex items-start justify-between gap-4">
              <Label htmlFor={`lock-${item.dir}`} className="grid gap-0.5 font-normal">
                <span className="font-medium">{t(item.title)}</span>
                <span className="text-xs text-muted-foreground">{t(item.text)}</span>
              </Label>
              <Switch
                id={`lock-${item.dir}`}
                checked={locked.includes(item.dir)}
                onCheckedChange={(on) =>
                  setLocked((now) =>
                    on ? [...now.filter((dir) => dir !== item.dir), item.dir] : now.filter((dir) => dir !== item.dir),
                  )
                }
              />
            </div>
          ))}
          <div className="grid gap-2 border-t pt-4">
            <h3 className="text-sm font-medium">{t("Always the player's own")}</h3>
            <p className="flex flex-wrap gap-1.5">
              {OWNED.map((file) => (
                <span key={file} className="bg-muted px-1.5 py-0.5 font-mono text-xs">
                  {file}
                </span>
              ))}
            </p>
            <p className="text-xs text-muted-foreground">
              {t(
                "Game settings, key binds and the server list arrive with the first install and are never overwritten after that. Configs marked “Copy once” on the Modpack page update only while the player has not edited them.",
              )}
            </p>
            <p className="text-xs text-muted-foreground">
              {t(
                "These checks stop casual cheating. A player with a modified launcher can get past them, so keep an anti-cheat mod on the server for anything that matters.",
              )}
            </p>
          </div>
        </div>
        </div>
      </Tile>

      <div className="flex items-center gap-3 md:col-span-6">
        <ActionButton type="submit" icon="check" busy={update.isPending} disabled={!dirty}>
          {t("Save changes")}
        </ActionButton>
        {dirty && <span className="text-sm text-muted-foreground">{t("Unsaved changes")}</span>}
      </div>

      <Tile className="gap-3 md:col-span-3">
        <h2 className="font-semibold">Minecraft EULA</h2>
        {instance.eula_accepted ? (
          <p className="flex items-center gap-2 text-sm">
            <Icon name="circle-check" className="text-ok" appear />
            {t("Accepted for this instance.")}
          </p>
        ) : (
          <>
            <p className="text-sm text-muted-foreground">
              {t("The server refuses to start until you accept the")}{" "}
              <a
                href="https://aka.ms/MinecraftEULA"
                target="_blank"
                rel="noreferrer"
                className="font-medium text-foreground underline underline-offset-4"
              >
                Minecraft EULA
              </a>.
            </p>
            <div>
              <ActionButton
                type="button"
                icon="shield"
                variant="outline"
                busy={eula.isPending}
                onClick={() => eula.mutate()}
              >
                {t("Accept the EULA")}
              </ActionButton>
            </div>
          </>
        )}
      </Tile>

      <Tile className="gap-3 border-destructive/30 md:col-span-3">
        <h2 className="font-semibold">{t("Delete instance")}</h2>
        <p className="text-sm text-muted-foreground">
          {t("Removes the instance from Relic and from every launcher.")}
          {!stopped && ` ${t("Stop the server first.")}`}
        </p>
        <div>
          <ActionButton
            type="button"
            icon="delete"
            variant="destructive"
            busy={remove.isPending}
            disabled={!stopped}
            onClick={() => setConfirming(true)}
          >
            {t("Delete {name}", { name: instance.name })}
          </ActionButton>
        </div>
      </Tile>

      <Confirm
        open={confirming}
        onOpenChange={setConfirming}
        title={t("Delete {name}?", { name: instance.name })}
        description={t("Players lose access to this modpack at once. This cannot be undone.")}
        action={t("Delete instance")}
        destructive
        onConfirm={() => remove.mutate()}
      >
        <Label className="flex items-start gap-2.5 font-normal">
          <Checkbox
            checked={purge}
            onCheckedChange={(value) => setPurge(value === true)}
            className="mt-0.5"
          />
          <span>
            {t("Also delete the world, configs and backups from disk")}
            <span className="block text-xs text-muted-foreground">
              {t("Left unchecked, the server folder stays where it is.")}
            </span>
          </span>
        </Label>
      </Confirm>
    </form>
  );
}
