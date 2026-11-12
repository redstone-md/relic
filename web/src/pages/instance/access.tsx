import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useId, useState } from "react";
import { toast } from "sonner";

import { ErrorNote, Help, Loading, Tile, TileLabel } from "@/components/common";
import { Icon, type IconName } from "@/components/icon";
import { StartPrompt } from "@/components/power";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { api, message } from "@/lib/api";
import { parseList, parseProperties, patchProperties, readText, useText, writeText } from "@/lib/fs";
import { useI18n } from "@/lib/i18n";
import { keys, useUsers } from "@/lib/queries";
import type { Instance } from "@/lib/types";

const WHITELIST = "whitelist.json";
const OPS = "ops.json";
const BANS = "banned-players.json";
const IP_BANS = "banned-ips.json";
const PROPERTIES = "server.properties";

interface Listed {
  uuid: string;
  name: string;
}
interface Op extends Listed {
  level: number;
  bypassesPlayerLimit: boolean;
}
interface Ban {
  created: string;
  source: string;
  expires: string;
  reason: string;
}
type PlayerBan = Listed & Ban;
type IpBan = { ip: string } & Ban;

const NAME = /^[A-Za-z0-9_]{1,16}$/;
const IP = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

function validIp(value: string): boolean {
  const parts = IP.exec(value);
  return parts !== null && parts.slice(1).every((part) => Number(part) <= 255);
}

/** The date format the server writes into its ban lists. */
function stamp(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return (
    `${now.getUTCFullYear()}-${pad(now.getUTCMonth() + 1)}-${pad(now.getUTCDate())} ` +
    `${pad(now.getUTCHours())}:${pad(now.getUTCMinutes())}:${pad(now.getUTCSeconds())} +0000`
  );
}

function ban(reason: string): Ban {
  return {
    created: stamp(),
    source: "Relic",
    expires: "forever",
    reason: reason || "Banned by an operator.",
  };
}

/**
 * How a change reaches the server: as a console command while it runs (it
 * rewrites its own lists then), and straight into the list while it does not.
 */
function useAccess(instance: Instance) {
  const { t } = useI18n();
  const client = useQueryClient();
  const users = useUsers();
  const live = instance.runtime.status === "running";

  const refresh = () => {
    for (const path of [WHITELIST, OPS, BANS, IP_BANS, PROPERTIES]) {
      void client.invalidateQueries({ queryKey: keys.fsText(instance.id, path) });
    }
  };

  /** The Relic account behind a name; only those can be listed while the server is down. */
  const account = (name: string): Listed => {
    const user = users.data?.find((entry) => entry.username.toLowerCase() === name.toLowerCase());
    if (!user) {
      throw new Error(
        t("“{name}” has no account here. Start the server to list players by name alone.", { name }),
      );
    }
    return { uuid: user.id, name: user.username };
  };

  const change = useMutation({
    mutationFn: async (step: { command: string; offline: () => Promise<void> }) => {
      if (live) {
        await api("POST", `/admin/instances/${instance.id}/command`, {
          json: { line: step.command },
        });
        // The server writes its lists a moment after it takes the command.
        await new Promise((resolve) => setTimeout(resolve, 700));
      } else {
        await step.offline();
      }
    },
    onSuccess: refresh,
    onError: (error) => toast.error(message(error)),
  });

  /** Rewrites one of the lists while the server is not there to do it. */
  const edit = async <T,>(path: string, update: (list: T[]) => T[]) => {
    const list = parseList<T>(await readText(instance.id, path));
    await writeText(instance.id, path, `${JSON.stringify(update(list), null, 2)}\n`);
  };

  return { live, account, change, edit, names: users.data?.map((user) => user.username) ?? [] };
}

type Access = ReturnType<typeof useAccess>;

function ListTile({
  icon,
  title,
  note,
  count,
  action,
  form,
  children,
  empty,
}: {
  icon: IconName;
  title: string;
  note: string;
  count: number;
  action?: ReactNode;
  form: ReactNode;
  children: ReactNode;
  empty: string;
}) {
  return (
    <Tile className="gap-4">
      <div className="flex items-start justify-between gap-4">
        <div className="grid gap-1">
          <div className="flex items-center gap-3">
            <TileLabel icon={icon}>{title}</TileLabel>
            <span className="font-pixel text-xs tabular">{count}</span>
          </div>
          <p className="text-sm text-muted-foreground">{note}</p>
        </div>
        {action}
      </div>
      {form}
      {count === 0 ? (
        <p className="border border-dashed px-3 py-6 text-center text-sm text-muted-foreground">
          {empty}
        </p>
      ) : (
        <ul className="grid max-h-80 content-start overflow-y-auto border">{children}</ul>
      )}
    </Tile>
  );
}

function Row({
  title,
  note,
  onRemove,
  removeLabel,
  busy,
}: {
  title: string;
  note?: string;
  onRemove: () => void;
  removeLabel: string;
  busy: boolean;
}) {
  return (
    <li className="flex items-center gap-3 border-b px-3 py-2 last:border-b-0">
      <div className="min-w-0 flex-1">
        <p className="truncate font-mono text-sm">{title}</p>
        {note && <p className="truncate text-xs text-muted-foreground">{note}</p>}
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={removeLabel}
        title={removeLabel}
        disabled={busy}
        onClick={onRemove}
      >
        <Icon name="x" />
      </Button>
    </li>
  );
}

function AddForm({
  placeholder,
  label,
  names,
  valid,
  extra,
  busy,
  onAdd,
}: {
  placeholder: string;
  label: string;
  names?: string[];
  valid: (value: string) => boolean;
  /** A second field: the reason of a ban. */
  extra?: string;
  busy: boolean;
  onAdd: (value: string, extra: string) => void;
}) {
  const list = useId();
  const [value, setValue] = useState("");
  const [more, setMore] = useState("");
  const ok = valid(value.trim());

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!ok) return;
    onAdd(value.trim(), more.trim());
    setValue("");
    setMore("");
  };

  return (
    <form onSubmit={submit} className="flex flex-wrap gap-2">
      <Input
        value={value}
        onChange={(event) => setValue(event.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        aria-invalid={value.trim() !== "" && !ok}
        list={names ? list : undefined}
        autoComplete="off"
        spellCheck={false}
        className="min-w-32 flex-1 font-mono"
      />
      {names && (
        <datalist id={list}>
          {names.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
      )}
      {extra !== undefined && (
        <Input
          value={more}
          onChange={(event) => setMore(event.target.value)}
          placeholder={extra}
          aria-label={extra}
          maxLength={120}
          className="min-w-32 flex-[2]"
        />
      )}
      <Button type="submit" variant="secondary" disabled={!ok || busy}>
        <Icon name="plus" />
        {label}
      </Button>
    </form>
  );
}

function Whitelist({ instance, access }: { instance: Instance; access: Access }) {
  const { t } = useI18n();
  const file = useText(instance.id, WHITELIST);
  const properties = useText(instance.id, PROPERTIES);
  const list = parseList<Listed>(file.data);
  const on = parseProperties(properties.data).get("white-list") === "true";
  const busy = access.change.isPending;

  const toggle = (next: boolean) =>
    access.change.mutate({
      command: `whitelist ${next ? "on" : "off"}`,
      offline: async () => {
        const text = await readText(instance.id, PROPERTIES);
        await writeText(
          instance.id,
          PROPERTIES,
          patchProperties(text, new Map([["white-list", String(next)]])),
        );
      },
    });

  return (
    <ListTile
      icon="user-check"
      title={t("Whitelist")}
      count={list.length}
      note={
        on
          ? t("Only these players can join.")
          : t("Off: anyone with an account can join. The list is kept for when you switch it on.")
      }
      action={
        <Label className="flex shrink-0 items-center gap-2 font-normal">
          <span className="text-sm">{on ? t("On") : t("Off")}</span>
          <Switch checked={on} disabled={busy || properties.isPending} onCheckedChange={toggle} />
        </Label>
      }
      empty={t("Nobody is on the whitelist")}
      form={
        <AddForm
          placeholder={t("Player name")}
          label={t("Add")}
          names={access.names}
          valid={(value) => NAME.test(value)}
          busy={busy}
          onAdd={(name) =>
            access.change.mutate({
              command: `whitelist add ${name}`,
              offline: async () => {
                const player = access.account(name);
                await access.edit<Listed>(WHITELIST, (now) => [
                  ...now.filter((entry) => entry.uuid !== player.uuid),
                  player,
                ]);
              },
            })
          }
        />
      }
    >
      {list.map((entry) => (
        <Row
          key={entry.uuid}
          title={entry.name}
          busy={busy}
          removeLabel={t("Take {name} off the whitelist", { name: entry.name })}
          onRemove={() =>
            access.change.mutate({
              command: `whitelist remove ${entry.name}`,
              offline: () =>
                access.edit<Listed>(WHITELIST, (now) =>
                  now.filter((other) => other.uuid !== entry.uuid),
                ),
            })
          }
        />
      ))}
    </ListTile>
  );
}

function Operators({ instance, access }: { instance: Instance; access: Access }) {
  const { t } = useI18n();
  const file = useText(instance.id, OPS);
  const properties = useText(instance.id, PROPERTIES);
  const list = parseList<Op>(file.data);
  const level = Number(parseProperties(properties.data).get("op-permission-level") ?? 4) || 4;
  const busy = access.change.isPending;

  return (
    <ListTile
      icon="shield"
      title={t("Operators")}
      count={list.length}
      note={t("May run commands in the game: change modes, teleport, ban and stop the server.")}
      empty={t("Nobody is an operator")}
      form={
        <AddForm
          placeholder={t("Player name")}
          label={t("Make operator")}
          names={access.names}
          valid={(value) => NAME.test(value)}
          busy={busy}
          onAdd={(name) =>
            access.change.mutate({
              command: `op ${name}`,
              offline: async () => {
                const player = access.account(name);
                await access.edit<Op>(OPS, (now) => [
                  ...now.filter((entry) => entry.uuid !== player.uuid),
                  { ...player, level, bypassesPlayerLimit: false },
                ]);
              },
            })
          }
        />
      }
    >
      {list.map((entry) => (
        <Row
          key={entry.uuid}
          title={entry.name}
          note={t("Level {level}", { level: entry.level })}
          busy={busy}
          removeLabel={t("Take operator rights from {name}", { name: entry.name })}
          onRemove={() =>
            access.change.mutate({
              command: `deop ${entry.name}`,
              offline: () =>
                access.edit<Op>(OPS, (now) => now.filter((other) => other.uuid !== entry.uuid)),
            })
          }
        />
      ))}
    </ListTile>
  );
}

function Bans({ instance, access }: { instance: Instance; access: Access }) {
  const { t } = useI18n();
  const file = useText(instance.id, BANS);
  const list = parseList<PlayerBan>(file.data);
  const busy = access.change.isPending;

  return (
    <ListTile
      icon="ban"
      title={t("Banned players")}
      count={list.length}
      note={t("Cannot join this server. Their account on the panel stays as it is.")}
      empty={t("Nobody is banned")}
      form={
        <AddForm
          placeholder={t("Player name")}
          label={t("Ban")}
          names={access.names}
          extra={t("Reason, shown to the player")}
          valid={(value) => NAME.test(value)}
          busy={busy}
          onAdd={(name, reason) =>
            access.change.mutate({
              command: reason ? `ban ${name} ${reason}` : `ban ${name}`,
              offline: async () => {
                const player = access.account(name);
                await access.edit<PlayerBan>(BANS, (now) => [
                  ...now.filter((entry) => entry.uuid !== player.uuid),
                  { ...player, ...ban(reason) },
                ]);
              },
            })
          }
        />
      }
    >
      {list.map((entry) => (
        <Row
          key={entry.uuid}
          title={entry.name}
          note={entry.reason}
          busy={busy}
          removeLabel={t("Lift the ban of {name}", { name: entry.name })}
          onRemove={() =>
            access.change.mutate({
              command: `pardon ${entry.name}`,
              offline: () =>
                access.edit<PlayerBan>(BANS, (now) =>
                  now.filter((other) => other.uuid !== entry.uuid),
                ),
            })
          }
        />
      ))}
    </ListTile>
  );
}

function IpBans({ instance, access }: { instance: Instance; access: Access }) {
  const { t } = useI18n();
  const file = useText(instance.id, IP_BANS);
  const list = parseList<IpBan>(file.data);
  const busy = access.change.isPending;

  return (
    <ListTile
      icon="lock"
      title={t("Banned addresses")}
      count={list.length}
      note={t("Nobody can join from these IP addresses, whatever account they use.")}
      empty={t("No address is banned")}
      form={
        <AddForm
          placeholder={t("IP address, e.g. 203.0.113.7")}
          label={t("Ban")}
          extra={t("Reason, shown to the player")}
          valid={validIp}
          busy={busy}
          onAdd={(ip, reason) =>
            access.change.mutate({
              command: reason ? `ban-ip ${ip} ${reason}` : `ban-ip ${ip}`,
              offline: () =>
                access.edit<IpBan>(IP_BANS, (now) => [
                  ...now.filter((entry) => entry.ip !== ip),
                  { ip, ...ban(reason) },
                ]),
            })
          }
        />
      }
    >
      {list.map((entry) => (
        <Row
          key={entry.ip}
          title={entry.ip}
          note={entry.reason}
          busy={busy}
          removeLabel={t("Lift the ban of {name}", { name: entry.ip })}
          onRemove={() =>
            access.change.mutate({
              command: `pardon-ip ${entry.ip}`,
              offline: () =>
                access.edit<IpBan>(IP_BANS, (now) => now.filter((other) => other.ip !== entry.ip)),
            })
          }
        />
      ))}
    </ListTile>
  );
}

/** Who is on right now, each with the two things an admin does in a hurry. */
function Online({ instance, access }: { instance: Instance; access: Access }) {
  const { t } = useI18n();
  const { runtime } = instance;
  const send = (line: string) =>
    access.change.mutate({ command: line, offline: () => Promise.resolve() });

  return (
    <Tile className="gap-3 md:col-span-2">
      <div className="flex items-center gap-3">
        <TileLabel icon="users">{t("Online now")}</TileLabel>
        {access.live && (
          <span className="font-pixel text-xs tabular">
            {runtime.players.length} / {runtime.players_max}
          </span>
        )}
      </div>
      {!access.live ? (
        <StartPrompt instance={instance} title={t("The server is stopped")} className="py-6">
          {t(
            "Nobody is playing. The lists below can still be edited, for players who have an account here.",
          )}
        </StartPrompt>
      ) : runtime.players.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("Nobody is playing")}</p>
      ) : (
        <ul className="grid gap-(--frame) sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {runtime.players.map((name) => (
            <li key={name} className="flex items-center gap-1 border py-1 pr-1 pl-3">
              <span className="min-w-0 flex-1 truncate font-mono text-sm">{name}</span>
              <Button
                variant="ghost"
                size="xs"
                disabled={access.change.isPending}
                onClick={() => send(`kick ${name}`)}
              >
                {t("Kick")}
              </Button>
              <Button
                variant="ghost"
                size="xs"
                className="text-destructive"
                disabled={access.change.isPending}
                onClick={() => send(`ban ${name}`)}
              >
                {t("Ban")}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Tile>
  );
}

export function AccessTab({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const access = useAccess(instance);
  const probe = useText(instance.id, WHITELIST);

  if (probe.isPending) return <Loading />;
  if (probe.error) return <ErrorNote error={probe.error} />;

  return (
    <>
      <div className="grid gap-(--frame) md:grid-cols-2">
        <Online instance={instance} access={access} />
        <Whitelist instance={instance} access={access} />
        <Operators instance={instance} access={access} />
        <Bans instance={instance} access={access} />
        <IpBans instance={instance} access={access} />
      </div>
      <Help>
        <p>
          {t(
            "These are the server's own lists. While it runs, every change here is sent as a console command, so it applies at once.",
          )}
        </p>
        <p>
          {t(
            "While it is stopped, the lists are written directly and take effect at the next start. That needs the player's account id, so only players registered on this panel can be added then.",
          )}
        </p>
        <p>
          {t(
            "A ban here closes one server. To close every server and the launcher, ban the account on the panel's Players page.",
          )}
        </p>
      </Help>
    </>
  );
}
