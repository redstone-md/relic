import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { toast } from "sonner";
import { Link } from "wouter";

import { ActionButton, Confirm, ErrorNote, Field, Help, Loading, Tile, TileLabel } from "@/components/common";
import { Icon } from "@/components/icon";
import { StartPrompt } from "@/components/power";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { message, upload } from "@/lib/api";
import { bytes, dateTime, halted } from "@/lib/format";
import { archiveUrl, parseProperties, patchProperties, readText, toTrash, useText, writeText } from "@/lib/fs";
import { useI18n } from "@/lib/i18n";
import { keys, useFolder, useUsage } from "@/lib/queries";
import type { Instance } from "@/lib/types";

const FILE = "server.properties";

/** The folders one world may take: servers with plugins split the dimensions out. */
function parts(level: string): { folder: string; label: string }[] {
  return [
    { folder: level, label: "Overworld" },
    { folder: `${level}_nether`, label: "The Nether" },
    { folder: `${level}_the_end` , label: "The End" },
  ];
}

export function WorldsTab({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const properties = useText(instance.id, FILE);
  const root = useFolder(instance.id, "");
  const usage = useUsage(instance.id);
  const picker = useRef<HTMLInputElement>(null);
  const [incoming, setIncoming] = useState<File | null>(null);
  const [sent, setSent] = useState(0);
  const [renewing, setRenewing] = useState(false);
  const [seed, setSeed] = useState("");

  const settings = parseProperties(properties.data);
  const level = settings.get("level-name") || "world";
  const down = halted(instance.runtime.status);

  const refresh = () =>
    Promise.all([
      client.invalidateQueries({ queryKey: keys.fs(instance.id, "") }),
      client.invalidateQueries({ queryKey: keys.fsUsage(instance.id) }),
      client.invalidateQueries({ queryKey: keys.fsText(instance.id, FILE) }),
    ]);

  const present = parts(level).filter((part) =>
    root.data?.some((entry) => entry.dir && entry.name === part.folder),
  );

  /** Moves the world out of the way; it waits in the trash until someone empties it. */
  const shelve = async () => {
    for (const part of present) await toTrash(instance.id, part.folder);
  };

  const replace = useMutation({
    mutationFn: async (file: File) => {
      setSent(0);
      await shelve();
      return upload<{ files: number }>(
        `/admin/instances/${instance.id}/fs/unpack`,
        file,
        { path: level },
        setSent,
        "POST",
      );
    },
    onSuccess: () => toast.success(t("World uploaded. The server opens it at the next start.")),
    onError: (error) => toast.error(message(error)),
    onSettled: () => {
      setIncoming(null);
      return refresh();
    },
  });

  const renew = useMutation({
    mutationFn: async () => {
      await shelve();
      const text = await readText(instance.id, FILE);
      await writeText(
        instance.id,
        FILE,
        patchProperties(text, new Map([["level-seed", seed.trim()]])),
      );
    },
    onSuccess: () => toast.success(t("Done. A new world is generated at the next start.")),
    onError: (error) => toast.error(message(error)),
    onSettled: refresh,
  });

  if (properties.isPending || root.isPending) return <Loading />;
  if (properties.error) return <ErrorNote error={properties.error} />;
  if (root.error) return <ErrorNote error={root.error} />;

  const size = (folder: string) => usage.data?.folders.find((entry) => entry.name === folder)?.size;
  const total = present.reduce((sum, part) => sum + (size(part.folder) ?? 0), 0);
  const busy = replace.isPending || renew.isPending;
  const exists = present.length > 0;

  const choose = (file: File) => {
    if (exists) setIncoming(file);
    else replace.mutate(file);
  };

  return (
    <>
      <div className="grid gap-(--frame) lg:grid-cols-3">
        <Tile className="gap-4 lg:col-span-2">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="grid gap-1">
              <TileLabel icon="earth">{t("Current world")}</TileLabel>
              <p className="font-pixel text-sm leading-6">{level}</p>
            </div>
            {exists && (
              <p className="font-pixel text-sm leading-6 tabular">
                {usage.data ? bytes(total) : "—"}
              </p>
            )}
          </div>

          {!exists ? (
            <StartPrompt instance={instance} title={t("No world yet")} className="py-8">
              {t("The server generates one at its first start. To play an existing world, upload it as a zip.")}
            </StartPrompt>
          ) : (
            <ul className="grid border">
              {present.map((part) => {
                const entry = root.data!.find((item) => item.name === part.folder)!;
                const weight = size(part.folder);
                return (
                  <li key={part.folder} className="flex items-center gap-3 border-b px-3 py-2 last:border-b-0">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium">{t(part.label)}</p>
                      <p className="truncate font-mono text-xs text-muted-foreground">{part.folder}</p>
                    </div>
                    <p className="hidden text-xs text-muted-foreground sm:block">
                      {t("Changed {date}", { date: dateTime(entry.modified) })}
                    </p>
                    <p className="w-20 text-right font-mono text-sm tabular">
                      {weight === undefined ? "—" : bytes(weight)}
                    </p>
                    <Button asChild variant="ghost" size="sm">
                      <a href={archiveUrl(instance.id, part.folder)} download={`${part.folder}.zip`}>
                        <Icon name="download" />
                        {t("Download")}
                      </a>
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}

          {replace.isPending && (
            <div className="grid gap-1.5">
              <p className="text-sm text-muted-foreground">
                {t("Uploading {name}: {sent} of {size}", {
                  name: replace.variables.name,
                  sent: bytes(sent),
                  size: bytes(replace.variables.size),
                })}
              </p>
              <Progress value={(sent / Math.max(replace.variables.size, 1)) * 100} />
            </div>
          )}

          <p className="text-sm text-muted-foreground">
            {t("Dimensions that mods add, and the Nether and the End on servers without plugins, live inside the main folder.")}
          </p>
        </Tile>

        <Tile className="gap-4">
          <TileLabel icon="refresh">{t("Change the world")}</TileLabel>
          {!down && (
            <p className="flex items-start gap-2 text-sm text-warn">
              <Icon name="alert" className="mt-0.5 shrink-0" />
              {t("Stop the server first: it keeps the world open while it runs.")}
            </p>
          )}
          <div className="grid gap-1.5">
            <input
              ref={picker}
              type="file"
              accept=".zip,application/zip"
              className="hidden"
              onChange={(event) => {
                const file = event.target.files?.[0];
                event.target.value = "";
                if (file) choose(file);
              }}
            />
            <ActionButton
              icon="upload"
              variant="secondary"
              busy={replace.isPending}
              disabled={!down || busy}
              onClick={() => picker.current?.click()}
            >
              {t("Upload a world")}
            </ActionButton>
            <p className="text-xs text-muted-foreground">
              {t("A zip of a world folder, the one that holds level.dat.")}
            </p>
          </div>
          <div className="grid gap-1.5">
            <ActionButton
              icon="sparkles"
              variant="secondary"
              busy={renew.isPending}
              disabled={!down || busy || !exists}
              onClick={() => {
                setSeed(settings.get("level-seed") ?? "");
                setRenewing(true);
              }}
            >
              {t("Generate a new world")}
            </ActionButton>
            <p className="text-xs text-muted-foreground">
              {t("Puts the current world aside and lets the server build a fresh one.")}
            </p>
          </div>
        </Tile>
      </div>

      <Confirm
        open={incoming !== null}
        onOpenChange={(open) => !open && setIncoming(null)}
        title={t("Replace the world “{name}”?", { name: level })}
        description={t(
          "The current world moves to the trash in Files, from where it can be put back. “{file}” takes its place.",
          { file: incoming?.name ?? "" },
        )}
        action={t("Replace the world")}
        destructive
        onConfirm={() => incoming && replace.mutate(incoming)}
      />

      <Confirm
        open={renewing}
        onOpenChange={setRenewing}
        title={t("Generate a new world?")}
        description={t(
          "The current world moves to the trash in Files, from where it can be put back. Players start over in a new one at the next start.",
        )}
        action={t("Generate a new world")}
        destructive
        onConfirm={() => renew.mutate()}
      >
        <Field
          label={t("Seed")}
          htmlFor="world-seed"
          hint={t("Leave empty for a random world.")}
        >
          <Input
            id="world-seed"
            value={seed}
            spellCheck={false}
            onChange={(event) => setSeed(event.target.value)}
            className="font-mono"
          />
        </Field>
      </Confirm>

      <Help>
        <p>
          {t("The server plays the world in the folder named by “World folder” on the Game rules page.")}
        </p>
        <p>
          {t("Nothing here deletes a world. Replaced and regenerated worlds wait in the")}{" "}
          <Link
            href={`/instances/${instance.id}/files`}
            className="font-medium text-foreground underline underline-offset-4"
          >
            {t("trash in Files")}
          </Link>
          {t(", and take disk space until you empty it.")}
        </p>
        <p>{t("For a copy you can go back to on a schedule, use Backups.")}</p>
      </Help>
    </>
  );
}
