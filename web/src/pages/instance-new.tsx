import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { Link, useLocation } from "wouter";

import { ActionButton, ErrorNote, Field, PageHeader, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { LoaderFields, type Platform } from "@/components/loader-fields";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { api } from "@/lib/api";
import { slug } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useInstances } from "@/lib/queries";
import type { Instance, NewInstance } from "@/lib/types";

const ID_RULE = /^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$/;

export function NewInstancePage() {
  const { t } = useI18n();
  const client = useQueryClient();
  const [, navigate] = useLocation();
  const existing = useInstances();

  const [name, setName] = useState("");
  const [id, setId] = useState("");
  const [idEdited, setIdEdited] = useState(false);
  const [description, setDescription] = useState("");
  const [platform, setPlatform] = useState<Platform>({
    minecraft: "",
    kind: "fabric",
    loaderVersion: "",
  });
  const [port, setPort] = useState("");
  const [memory, setMemory] = useState("4096");

  const effectiveId = idEdited ? id : slug(name);
  const taken = existing.data?.some((instance) => instance.id === effectiveId) ?? false;
  const idProblem =
    effectiveId === ""
      ? null
      : !ID_RULE.test(effectiveId)
        ? t("Use 1-32 lowercase letters, digits and dashes, without a dash at either end.")
        : taken
          ? t("Another instance already uses this id.")
          : null;

  const create = useMutation({
    mutationFn: () => {
      const body: NewInstance = {
        id: effectiveId,
        name: name.trim(),
        description: description.trim(),
        minecraft_version: platform.minecraft,
        loader: {
          type: platform.kind,
          version: platform.loaderVersion || undefined,
        },
        port: port ? Number(port) : undefined,
        memory_mb: memory ? Number(memory) : undefined,
      };
      return api<Instance>("POST", "/admin/instances", { json: body });
    },
    onSuccess: async (instance) => {
      await client.invalidateQueries({ queryKey: keys.instances });
      navigate(`/instances/${instance.id}`);
    },
  });

  const ready = name.trim() !== "" && effectiveId !== "" && !idProblem && platform.minecraft !== "";
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (ready) create.mutate();
  };

  return (
    <>
      <PageHeader
        title={t("New instance")}
        description={t(
          "The server files install on first start. Mods and settings can be added before that.",
        )}
      />
      <form onSubmit={submit} className="grid gap-(--frame) md:grid-cols-6">
        <Tile className="gap-4 md:col-span-3">
          <h2 className="font-semibold">{t("Identity")}</h2>
          <Field label={t("Name")} htmlFor="name" hint={t("Shown to players in the launcher.")}>
            <Input
              id="name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={64}
              required
              autoFocus
            />
          </Field>
          <Field
            label={t("Id")}
            htmlFor="id"
            hint={
              idProblem ? (
                <span className="text-destructive">{idProblem}</span>
              ) : (
                t("Names the folder on disk and the address in the API. Cannot be changed later.")
              )
            }
          >
            <Input
              id="id"
              value={effectiveId}
              onChange={(event) => {
                setIdEdited(true);
                setId(event.target.value.toLowerCase());
              }}
              className="font-mono"
              maxLength={32}
              spellCheck={false}
              aria-invalid={Boolean(idProblem)}
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
        </Tile>

        <Tile className="gap-4 md:col-span-3">
          <h2 className="font-semibold">{t("Platform")}</h2>
          <LoaderFields value={platform} onChange={setPlatform} />
        </Tile>

        <Tile className="gap-4 md:col-span-6">
          <h2 className="font-semibold">{t("Resources")}</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field
              label={t("Port")}
              htmlFor="port"
              hint={t("Leave empty to take the next free port from 25565.")}
            >
              <Input
                id="port"
                type="number"
                inputMode="numeric"
                min={1024}
                max={65535}
                value={port}
                onChange={(event) => setPort(event.target.value)}
                placeholder={t("Automatic")}
                className="font-mono"
              />
            </Field>
            <Field
              label={t("Memory, MB")}
              htmlFor="memory"
              hint={t("The Java heap limit. Modded servers usually want 4096 or more.")}
            >
              <Input
                id="memory"
                type="number"
                inputMode="numeric"
                min={512}
                step={256}
                value={memory}
                onChange={(event) => setMemory(event.target.value)}
                className="font-mono"
              />
            </Field>
          </div>
        </Tile>

        <div className="flex flex-wrap items-center gap-2 md:col-span-6">
          <ActionButton type="submit" icon="plus" busy={create.isPending} disabled={!ready}>
            {t("Create instance")}
          </ActionButton>
          <Button asChild variant="ghost">
            <Link href="/">
              <Icon name="back" />
              {t("Cancel")}
            </Link>
          </Button>
          {create.error && (
            <div className="min-w-0 flex-1">
              <ErrorNote error={create.error} />
            </div>
          )}
        </div>
      </form>
    </>
  );
}
