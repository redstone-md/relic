import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";

import {
  ActionButton,
  Confirm,
  ErrorNote,
  Field,
  Loading,
  PageHeader,
  Tile,
} from "@/components/common";
import { Icon } from "@/components/icon";
import { FONTS, MediaField } from "@/components/media-field";
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
import { useI18n } from "@/lib/i18n";
import { keys, useSettings } from "@/lib/queries";
import type {
  General,
  LinkItem,
  Palette,
  RegistrationPolicy,
  Settings,
  Theme,
} from "@/lib/types";

const REGISTRATION: { value: RegistrationPolicy; label: string; text: string }[] = [
  {
    value: "strict_invite",
    label: "Invite only",
    text: "A new player needs a one-time invite code from you.",
  },
  {
    value: "approval_required",
    label: "Approval",
    text: "Anyone can sign up, and plays once you approve the account.",
  },
  { value: "closed", label: "Closed", text: "Nobody new can register." },
];

const COLORS: { key: keyof Palette; label: string }[] = [
  { key: "background", label: "Background" },
  { key: "surface", label: "Surface" },
  { key: "border", label: "Border" },
  { key: "accent", label: "Accent" },
  { key: "text_primary", label: "Text" },
  { key: "text_secondary", label: "Muted text" },
];

const HEX = /^#[0-9a-f]{6}$/i;

function ColorField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const valid = HEX.test(value);
  return (
    <label className="grid gap-1.5 text-sm">
      <span className="font-medium">{label}</span>
      <span className="flex items-center gap-2">
        <input
          type="color"
          value={valid ? value : "#000000"}
          onChange={(event) => onChange(event.target.value)}
          aria-label={t("{label} colour picker", { label })}
          className="size-9 shrink-0 cursor-pointer rounded-lg border bg-transparent p-1"
        />
        <Input
          value={value}
          onChange={(event) => onChange(event.target.value.trim())}
          aria-label={t("{label} hex value", { label })}
          aria-invalid={!valid}
          maxLength={7}
          spellCheck={false}
          className="font-mono"
        />
      </span>
    </label>
  );
}

/** A small stand-in for the launcher's home screen, painted with the palette. */
function Preview({ theme, name }: { theme: Theme; name: string }) {
  const { t } = useI18n();
  const { palette } = theme;
  return (
    <div
      aria-hidden
      className="relative overflow-hidden rounded-xl border"
      style={{
        background: palette.background,
        borderColor: palette.border,
        color: palette.text_primary,
      }}
    >
      {theme.background_url && (
        <img
          src={theme.background_url}
          alt=""
          referrerPolicy="no-referrer"
          className="absolute inset-0 size-full object-cover opacity-40"
        />
      )}
      <div className="relative grid gap-3 p-4">
        <div className="flex items-center gap-2">
          {theme.logo_url ? (
            <img
              src={theme.logo_url}
              alt=""
              referrerPolicy="no-referrer"
              className="size-6 rounded object-contain"
            />
          ) : (
            <span className="size-6 rounded" style={{ background: palette.accent }} />
          )}
          <span className="truncate text-sm font-semibold">{name || t("Your server")}</span>
        </div>
        <div
          className="rounded-lg border p-3"
          style={{ background: palette.surface, borderColor: palette.border }}
        >
          <p className="text-sm font-medium">{t("Season two opens Friday")}</p>
          <p className="mt-1 text-xs" style={{ color: palette.text_secondary }}>
            {t("New world, updated modpack. The launcher downloads only what changed.")}
          </p>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs" style={{ color: palette.text_secondary }}>
            {t("12 players online")}
          </span>
          <span
            className="rounded-lg px-5 py-2 text-sm font-semibold"
            style={{ background: palette.accent, color: palette.background }}
          >
            {t("PLAY")}
          </span>
        </div>
      </div>
    </div>
  );
}

function CurseForgeKey({ present }: { present: boolean }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [key, setKey] = useState("");
  const [removing, setRemoving] = useState(false);
  const put = useMutation({
    mutationFn: (value: string | null) =>
      api("PUT", "/admin/settings/curseforge-key", { json: { key: value } }),
    onSuccess: (_, value) => {
      setKey("");
      toast.success(value ? t("CurseForge key saved") : t("CurseForge key removed"));
      void client.invalidateQueries({ queryKey: keys.settings });
    },
    onError: (error) => toast.error(message(error)),
  });

  return (
    <Tile className="gap-4 md:col-span-3">
      <div>
        <h2 className="font-semibold">CurseForge</h2>
        <p className="text-sm text-muted-foreground">
          {t("Modrinth works without a key. Searching CurseForge needs your own API key from")}{" "}
          <a
            href="https://console.curseforge.com/"
            target="_blank"
            rel="noreferrer"
            className="font-medium text-foreground underline underline-offset-4"
          >
            console.curseforge.com
          </a>
        </p>
      </div>
      <p className="flex items-center gap-2 text-sm">
        {present ? (
          <>
            <Icon name="circle-check" className="text-ok" appear />
            {t("A key is stored. It is never shown again.")}
          </>
        ) : (
          <>
            <Icon name="key" className="text-muted-foreground" />
            {t("No key stored.")}
          </>
        )}
      </p>
      <form
        className="flex flex-wrap items-end gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (key.trim()) put.mutate(key.trim());
        }}
      >
        <Field
          label={present ? t("Replace the key") : t("API key")}
          htmlFor="cf-key"
          className="flex-1"
        >
          <Input
            id="cf-key"
            type="password"
            value={key}
            onChange={(event) => setKey(event.target.value)}
            autoComplete="off"
            className="font-mono"
          />
        </Field>
        <ActionButton
          type="submit"
          icon="key"
          variant="outline"
          busy={put.isPending}
          disabled={key.trim() === ""}
        >
          {t("Save key")}
        </ActionButton>
        {present && (
          <Button type="button" variant="ghost" onClick={() => setRemoving(true)}>
            <Icon name="delete" />
            {t("Remove")}
          </Button>
        )}
      </form>
      <Confirm
        open={removing}
        onOpenChange={setRemoving}
        title={t("Remove the CurseForge key?")}
        description={t("CurseForge search and installs stop working until a key is added again.")}
        action={t("Remove")}
        destructive
        onConfirm={() => put.mutate(null)}
      />
    </Tile>
  );
}

function strip(settings: Settings): General {
  const { curseforge_key_set: _, ...general } = settings;
  return general;
}

function Form({ initial }: { initial: Settings }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [saved, setSaved] = useState<General>(() => strip(initial));
  const [form, setForm] = useState<General>(saved);
  const patch = (change: Partial<General>) => setForm((previous) => ({ ...previous, ...change }));
  const patchTheme = (change: Partial<Theme>) => patch({ theme: { ...form.theme, ...change } });
  const setLink = (index: number, change: Partial<LinkItem>) =>
    patch({
      links: form.links.map((link, at) => (at === index ? { ...link, ...change } : link)),
    });

  const save = useMutation({
    mutationFn: () => {
      const body: General = {
        ...form,
        server_name: form.server_name.trim(),
        public_host: form.public_host.trim(),
        theme: {
          palette: form.theme.palette,
          font_url: form.theme.font_url?.trim() || undefined,
          logo_url: form.theme.logo_url?.trim() || undefined,
          background_url: form.theme.background_url?.trim() || undefined,
        },
        links: form.links
          .map((link) => ({ ...link, label: link.label.trim(), url: link.url.trim() }))
          .filter((link) => link.label !== "" && link.url !== ""),
      };
      return api<Settings | undefined>("PUT", "/admin/settings", { json: body }).then(() => body);
    },
    onSuccess: (body) => {
      setSaved(body);
      setForm(body);
      toast.success(t("Settings saved"));
      void client.invalidateQueries({ queryKey: keys.settings });
    },
  });

  const paletteValid = COLORS.every((color) => HEX.test(form.theme.palette[color.key]));
  const dirty = JSON.stringify(form) !== JSON.stringify(saved);
  const policy = REGISTRATION.find((item) => item.value === form.registration);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (paletteValid) save.mutate();
  };

  return (
    <div className="grid gap-(--frame) md:grid-cols-6">
      <form onSubmit={submit} className="contents">
        <Tile className="gap-4 md:col-span-3">
          <h2 className="font-semibold">{t("Server")}</h2>
          <Field label={t("Name")} htmlFor="server-name" hint={t("Shown in the launcher's title.")}>
            <Input
              id="server-name"
              value={form.server_name}
              onChange={(event) => patch({ server_name: event.target.value })}
              maxLength={64}
              required
            />
          </Field>
          <Field
            label={t("Public address")}
            htmlFor="public-host"
            hint={t(
              "The domain players type into the launcher and connect to in game, without a scheme or port.",
            )}
          >
            <Input
              id="public-host"
              value={form.public_host}
              onChange={(event) => patch({ public_host: event.target.value })}
              placeholder="play.example.com"
              spellCheck={false}
              className="font-mono"
            />
          </Field>
        </Tile>

        <Tile className="gap-4 md:col-span-3">
          <h2 className="font-semibold">{t("Registration")}</h2>
          <Field label={t("Who can create an account")} hint={policy && t(policy.text)}>
            <Select
              value={form.registration}
              onValueChange={(next) => patch({ registration: next as RegistrationPolicy })}
            >
              <SelectTrigger className="w-full" aria-label={t("Registration policy")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REGISTRATION.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {t(item.label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <h2 className="mt-2 font-semibold">{t("Scheduled backups")}</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field
              label={t("Every, hours")}
              htmlFor="backup-interval"
              hint={t("0 turns the schedule off.")}
            >
              <Input
                id="backup-interval"
                type="number"
                inputMode="numeric"
                min={0}
                max={720}
                value={form.backup_interval_hours}
                onChange={(event) => patch({ backup_interval_hours: Number(event.target.value) })}
                className="font-mono"
              />
            </Field>
            <Field
              label={t("Keep the newest")}
              htmlFor="backup-keep"
              hint={t("Older copies of each instance are deleted.")}
            >
              <Input
                id="backup-keep"
                type="number"
                inputMode="numeric"
                min={1}
                max={365}
                value={form.backup_keep}
                onChange={(event) => patch({ backup_keep: Number(event.target.value) })}
                className="font-mono"
              />
            </Field>
          </div>
        </Tile>

        <Tile className="gap-4 md:col-span-6">
          <div className="flex items-center gap-2">
            <Icon name="palette" className="text-muted-foreground" />
            <h2 className="font-semibold">{t("Launcher theme")}</h2>
          </div>
          <div className="grid gap-6 lg:grid-cols-[1fr_20rem]">
            <div className="grid gap-4">
              <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                {COLORS.map((color) => (
                  <ColorField
                    key={color.key}
                    label={t(color.label)}
                    value={form.theme.palette[color.key]}
                    onChange={(value) =>
                      patchTheme({ palette: { ...form.theme.palette, [color.key]: value } })
                    }
                  />
                ))}
              </div>
              <div className="grid gap-4">
                <MediaField
                  id="logo-url"
                  label={t("Logo")}
                  hint={t("Square PNG or WebP. Shown in the launcher's corner and on its sign-in screen.")}
                  value={form.theme.logo_url ?? ""}
                  onChange={(value) => patchTheme({ logo_url: value })}
                  preview={false}
                />
                <MediaField
                  id="background-url"
                  label={t("Background image")}
                  hint={t("Wide PNG, JPEG, WebP or GIF.")}
                  value={form.theme.background_url ?? ""}
                  onChange={(value) => patchTheme({ background_url: value })}
                  preview={false}
                />
                <MediaField
                  id="font-url"
                  label={t("Font file")}
                  hint={t("TTF or OTF.")}
                  value={form.theme.font_url ?? ""}
                  onChange={(value) => patchTheme({ font_url: value })}
                  accept={FONTS}
                  preview={false}
                />
              </div>
            </div>
            <div className="grid content-start gap-2">
              <span className="text-xs text-muted-foreground">{t("Preview")}</span>
              <Preview theme={form.theme} name={form.server_name} />
            </div>
          </div>
        </Tile>

        <Tile className="gap-4 md:col-span-6">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h2 className="font-semibold">{t("Launcher links")}</h2>
              <p className="text-sm text-muted-foreground">
                {t("Buttons on the launcher's home screen: a site, Discord, a map.")}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              onClick={() => patch({ links: [...form.links, { label: "", url: "" }] })}
            >
              <Icon name="plus" />
              {t("Add link")}
            </Button>
          </div>
          {form.links.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("No links.")}</p>
          ) : (
            <ul className="grid gap-2">
              {form.links.map((link, index) => (
                <li key={index} className="flex flex-wrap items-center gap-2">
                  <Input
                    value={link.label}
                    onChange={(event) => setLink(index, { label: event.target.value })}
                    placeholder={t("Label")}
                    aria-label={t("Label of link {number}", { number: index + 1 })}
                    maxLength={32}
                    className="w-44"
                  />
                  <Input
                    type="url"
                    value={link.url}
                    onChange={(event) => setLink(index, { url: event.target.value })}
                    placeholder="https://"
                    aria-label={t("Address of link {number}", { number: index + 1 })}
                    className="min-w-56 flex-1"
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("Remove link {number}", { number: index + 1 })}
                    onClick={() => patch({ links: form.links.filter((_, at) => at !== index) })}
                  >
                    <Icon name="x" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </Tile>

        <div className="flex flex-wrap items-center gap-3 md:col-span-6">
          <ActionButton
            type="submit"
            icon="check"
            busy={save.isPending}
            disabled={!dirty || !paletteValid}
          >
            {t("Save settings")}
          </ActionButton>
          {!paletteValid ? (
            <span className="text-sm text-destructive">
              {t("Colours must be six-digit hex, like #fb7013.")}
            </span>
          ) : (
            dirty && <span className="text-sm text-muted-foreground">{t("Unsaved changes")}</span>
          )}
          {save.error && (
            <div className="min-w-0 flex-1">
              <ErrorNote error={save.error} />
            </div>
          )}
        </div>
      </form>

      <CurseForgeKey present={initial.curseforge_key_set} />
    </div>
  );
}

export function SettingsPage() {
  const { t } = useI18n();
  const settings = useSettings();
  return (
    <>
      <PageHeader
        title={t("Settings")}
        description={t("How this server presents itself to players and launchers.")}
      />
      {settings.isPending ? (
        <Loading />
      ) : settings.error ? (
        <ErrorNote error={settings.error} />
      ) : (
        <Form initial={settings.data} />
      )}
    </>
  );
}
