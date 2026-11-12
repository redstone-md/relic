import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useMemo, useState } from "react";
import { toast } from "sonner";

import {
  ActionButton,
  Confirm,
  Empty,
  ErrorNote,
  Field,
  Loading,
  PageHeader,
  Tile,
} from "@/components/common";
import { Icon } from "@/components/icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, message } from "@/lib/api";
import { dateTime } from "@/lib/format";
import { t, useI18n } from "@/lib/i18n";
import { keys, useInstances, useInvites, useMe, useUsers } from "@/lib/queries";
import type { Invite, NewInvite, ResetCode, User } from "@/lib/types";

const STATUS: Record<User["status"], { label: string; className: string }> = {
  active: { label: "Active", className: "text-ok" },
  pending: { label: "Awaiting approval", className: "text-warn" },
  banned: { label: "Banned", className: "text-destructive" },
};

/** How long a new invite stays valid, in hours; `0` is forever. */
const LIFETIMES = [
  { value: "24", label: "1 day" },
  { value: "72", label: "3 days" },
  { value: "168", label: "7 days" },
  { value: "720", label: "30 days" },
  { value: "0", label: "Never expires" },
];

/** A secret the server shows exactly once, with a button to copy it. */
function Secret({ value, label }: { value: string; label: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error(t("Could not copy. Select the code and copy it by hand."));
    }
  };

  return (
    <div className="flex items-center gap-2 rounded-xl border bg-muted/50 p-2 pl-3">
      <code className="min-w-0 flex-1 font-mono text-sm break-all select-all">{value}</code>
      <Button type="button" variant="outline" size="sm" aria-label={label} onClick={copy}>
        <Icon name={copied ? "check" : "copy"} appear={copied} />
        {copied ? t("Copied") : t("Copy")}
      </Button>
    </div>
  );
}

function NewAccount({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<User["role"]>("player");

  const create = useMutation({
    mutationFn: () =>
      api<User>("POST", "/admin/users", { json: { username: username.trim(), password, role } }),
    onSuccess: (user) => {
      toast.success(t("Account {name} created", { name: user.username }));
      void client.invalidateQueries({ queryKey: keys.users });
      onClose();
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate();
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{t("New account")}</DialogTitle>
            <DialogDescription>
              {t("Created active right away, whatever the registration policy says.")}
            </DialogDescription>
          </DialogHeader>
          <Field
            label={t("Username")}
            htmlFor="account-username"
            hint={t("3-16 characters: letters, digits and underscore. This is the in-game name.")}
          >
            <Input
              id="account-username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              minLength={3}
              maxLength={16}
              pattern="[A-Za-z0-9_]+"
              autoComplete="off"
              required
              autoFocus
            />
          </Field>
          <Field
            label={t("Password")}
            htmlFor="account-password"
            hint={t(
              "At least 8 characters. Pass it to the player; they can change it in the launcher.",
            )}
          >
            <Input
              id="account-password"
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              minLength={8}
              maxLength={128}
              autoComplete="new-password"
              required
            />
          </Field>
          <Field label={t("Role")}>
            <Select value={role} onValueChange={(next) => setRole(next as User["role"])}>
              <SelectTrigger className="w-full" aria-label={t("Role")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="player">{t("Player")}</SelectItem>
                <SelectItem value="admin">{t("Admin: full access to this panel")}</SelectItem>
              </SelectContent>
            </Select>
          </Field>
          {create.error && <ErrorNote error={create.error} />}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t("Cancel")}
            </Button>
            <ActionButton type="submit" icon="plus" busy={create.isPending}>
              {t("Create")}
            </ActionButton>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function NewInviteDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [note, setNote] = useState("");
  const [lifetime, setLifetime] = useState("168");

  const create = useMutation({
    mutationFn: () =>
      api<NewInvite>("POST", "/admin/invites", {
        json: { note: note.trim(), expires_in_hours: Number(lifetime) || null },
      }),
    onSuccess: () => void client.invalidateQueries({ queryKey: keys.invites }),
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate();
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {create.data ? (
          <div className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t("Invite ready")}</DialogTitle>
              <DialogDescription>
                {t("Send this code to the player. It works once and is not shown again.")}
              </DialogDescription>
            </DialogHeader>
            <Secret value={create.data.code} label={t("Copy the invite code")} />
            <DialogFooter>
              <Button type="button" onClick={onClose}>
                {t("Done")}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t("New invite")}</DialogTitle>
              <DialogDescription>
                {t("A one-time code that lets one person register an account.")}
              </DialogDescription>
            </DialogHeader>
            <Field
              label={t("Note")}
              htmlFor="invite-note"
              hint={t("Optional. Who the invite is for; only admins see it.")}
            >
              <Input
                id="invite-note"
                value={note}
                onChange={(event) => setNote(event.target.value)}
                maxLength={200}
                autoFocus
              />
            </Field>
            <Field label={t("Valid for")}>
              <Select value={lifetime} onValueChange={setLifetime}>
                <SelectTrigger className="w-full" aria-label={t("Valid for")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {LIFETIMES.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {t(item.label)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            {create.error && <ErrorNote error={create.error} />}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={onClose}>
                {t("Cancel")}
              </Button>
              <ActionButton type="submit" icon="plus" busy={create.isPending}>
                {t("Create invite")}
              </ActionButton>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

function ResetDialog({
  user,
  reset,
  onClose,
}: {
  user: User;
  reset: ResetCode;
  onClose: () => void;
}) {
  const { t } = useI18n();
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("Reset code for {name}", { name: user.username })}</DialogTitle>
          <DialogDescription>
            {t(
              "The player enters it in the launcher under “Forgot password” and picks a new password. Valid until {date}; not shown again.",
              { date: dateTime(reset.expires_at) },
            )}
          </DialogDescription>
        </DialogHeader>
        <Secret value={reset.code} label={t("Copy the reset code")} />
        <DialogFooter>
          <Button type="button" onClick={onClose}>
            {t("Done")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Accounts() {
  const { t } = useI18n();
  const client = useQueryClient();
  const users = useUsers();
  const instances = useInstances();
  const me = useMe();
  const [search, setSearch] = useState("");
  const [deleting, setDeleting] = useState<User | null>(null);
  const [banning, setBanning] = useState<User | null>(null);
  const [issued, setIssued] = useState<{ user: User; reset: ResetCode } | null>(null);

  const refresh = () => void client.invalidateQueries({ queryKey: keys.users });

  const update = useMutation({
    mutationFn: ({ user, ...change }: { user: User; status?: User["status"]; role?: User["role"] }) =>
      api<User>("PATCH", `/admin/users/${user.id}`, { json: change }),
    onSuccess: (user, { status, role }) => {
      const name = user.username;
      if (role) {
        toast.success(
          role === "admin"
            ? t("{name} is now an admin", { name })
            : t("{name} is now a player", { name }),
        );
      } else {
        toast.success(
          status === "banned" ? t("{name} banned", { name }) : t("{name} can play now", { name }),
        );
      }
      refresh();
    },
    onError: (error) => toast.error(message(error)),
  });

  const remove = useMutation({
    mutationFn: (user: User) => api("DELETE", `/admin/users/${user.id}`),
    onSuccess: (_, user) => {
      toast.success(t("Account {name} deleted", { name: user.username }));
      refresh();
    },
    onError: (error) => toast.error(message(error)),
  });

  const resetCode = useMutation({
    mutationFn: (user: User) => api<ResetCode>("POST", `/admin/users/${user.id}/reset-code`),
    onSuccess: (reset, user) => setIssued({ user, reset }),
    onError: (error) => toast.error(message(error)),
  });

  /** Player name, lowercased, to the instance they are on right now. */
  const playing = useMemo(() => {
    const map = new Map<string, string>();
    for (const instance of instances.data ?? []) {
      if (instance.runtime.status !== "running") continue;
      for (const player of instance.runtime.players) map.set(player.toLowerCase(), instance.name);
    }
    return map;
  }, [instances.data]);

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const order = { pending: 0, active: 1, banned: 2 };
    return (users.data ?? [])
      .filter((user) => user.username.toLowerCase().includes(needle))
      .sort((a, b) => order[a.status] - order[b.status]);
  }, [users.data, search]);

  if (users.isPending) return <Loading />;
  if (users.error) return <ErrorNote error={users.error} />;

  return (
    <Tile className="gap-4">
      <div className="relative max-w-sm">
        <Icon
          name="search"
          className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t("Find a player")}
          aria-label={t("Find a player")}
          className="pl-8"
        />
      </div>
      {shown.length === 0 ? (
        <Empty icon="users" title={t("No player matches")} />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("Username")}</TableHead>
              <TableHead>{t("Status")}</TableHead>
              <TableHead>{t("Now")}</TableHead>
              <TableHead className="text-right">{t("Registered")}</TableHead>
              <TableHead className="w-0">
                <span className="sr-only">{t("Actions")}</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((user) => {
              const where = playing.get(user.username.toLowerCase());
              const self = user.id === me.data?.id;
              return (
                <TableRow key={user.id}>
                  <TableCell>
                    <span className="font-medium">{user.username}</span>
                    {user.role === "admin" && (
                      <Badge variant="secondary" className="ml-2">
                        {t("Admin")}
                      </Badge>
                    )}
                    {self && <span className="ml-2 text-xs text-muted-foreground">{t("you")}</span>}
                  </TableCell>
                  <TableCell className={STATUS[user.status].className}>
                    {t(STATUS[user.status].label)}
                  </TableCell>
                  <TableCell>
                    {where ? (
                      <span className="inline-flex items-center gap-2">
                        <span
                          aria-hidden
                          className="status-live size-2 rounded-full bg-current text-ok"
                        />
                        {where}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">{t("Offline")}</span>
                    )}
                  </TableCell>
                  <TableCell className="text-right text-muted-foreground tabular">
                    {dateTime(user.created_at)}
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center justify-end gap-1">
                      {user.status === "pending" && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => update.mutate({ user, status: "active" })}
                        >
                          <Icon name="user-check" />
                          {t("Approve")}
                        </Button>
                      )}
                      {!self && (
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button
                              variant="ghost"
                              size="icon-sm"
                              aria-label={t("Actions for {name}", { name: user.username })}
                            >
                              <Icon name="more" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end" className="min-w-48">
                            {user.status === "banned" ? (
                              <DropdownMenuItem
                                onSelect={() => update.mutate({ user, status: "active" })}
                              >
                                <Icon name="user-check" />
                                {t("Lift the ban")}
                              </DropdownMenuItem>
                            ) : (
                              <DropdownMenuItem onSelect={() => setBanning(user)}>
                                <Icon name="ban" />
                                {t("Ban")}
                              </DropdownMenuItem>
                            )}
                            <DropdownMenuItem
                              onSelect={() =>
                                update.mutate({
                                  user,
                                  role: user.role === "admin" ? "player" : "admin",
                                })
                              }
                            >
                              <Icon name={user.role === "admin" ? "user" : "shield"} />
                              {user.role === "admin" ? t("Make a player") : t("Make an admin")}
                            </DropdownMenuItem>
                            <DropdownMenuItem onSelect={() => resetCode.mutate(user)}>
                              <Icon name="key" />
                              {t("Reset password")}
                            </DropdownMenuItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuItem
                              variant="destructive"
                              onSelect={() => setDeleting(user)}
                            >
                              <Icon name="delete" />
                              {t("Delete account")}
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}

      {issued && <ResetDialog {...issued} onClose={() => setIssued(null)} />}
      <Confirm
        open={banning !== null}
        onOpenChange={(open) => !open && setBanning(null)}
        title={t("Ban {name}?", { name: banning?.username ?? "" })}
        description={t(
          "They are signed out everywhere, kicked from running servers and cannot sign in until the ban is lifted.",
        )}
        action={t("Ban")}
        destructive
        onConfirm={() => banning && update.mutate({ user: banning, status: "banned" })}
      />
      <Confirm
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("Delete {name}?", { name: deleting?.username ?? "" })}
        description={t(
          "The account, its skin and its sessions are removed for good. The name becomes free to register again. Worlds keep the player's items.",
        )}
        action={t("Delete")}
        destructive
        onConfirm={() => deleting && remove.mutate(deleting)}
      />
    </Tile>
  );
}

function inviteState(invite: Invite): { label: string; className: string } {
  if (invite.used_at !== null) {
    return {
      label:
        invite.used_by != null
          ? t("Used by {name}", { name: invite.used_by })
          : t("Used by a deleted account"),
      className: "text-muted-foreground",
    };
  }
  if (invite.expires_at !== null && invite.expires_at * 1000 <= Date.now()) {
    return { label: t("Expired"), className: "text-destructive" };
  }
  return { label: t("Open ## invite state"), className: "text-ok" };
}

function Invites() {
  const { t } = useI18n();
  const client = useQueryClient();
  const invites = useInvites();
  const [deleting, setDeleting] = useState<Invite | null>(null);

  const remove = useMutation({
    mutationFn: (invite: Invite) => api("DELETE", `/admin/invites/${invite.id}`),
    onSuccess: () => {
      toast.success(t("Invite deleted"));
      void client.invalidateQueries({ queryKey: keys.invites });
    },
    onError: (error) => toast.error(message(error)),
  });

  if (invites.isPending) return <Loading />;
  if (invites.error) return <ErrorNote error={invites.error} />;
  if (invites.data.length === 0) {
    return (
      <Empty icon="key" title={t("No invites yet")}>
        {t(
          "An invite is a one-time code. Under the “invite only” registration policy nobody can sign up without one.",
        )}
      </Empty>
    );
  }

  return (
    <Tile>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("Note")}</TableHead>
            <TableHead>{t("State")}</TableHead>
            <TableHead>{t("Created")}</TableHead>
            <TableHead>{t("Expires")}</TableHead>
            <TableHead className="w-0">
              <span className="sr-only">{t("Actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {invites.data.map((invite) => {
            const state = inviteState(invite);
            return (
              <TableRow key={invite.id}>
                <TableCell className="max-w-64 truncate">
                  {invite.note || <span className="text-muted-foreground">{t("No note")}</span>}
                </TableCell>
                <TableCell className={state.className}>{state.label}</TableCell>
                <TableCell className="text-muted-foreground tabular">
                  {invite.created_by
                    ? t("{date} by {name}", {
                        date: dateTime(invite.created_at),
                        name: invite.created_by,
                      })
                    : dateTime(invite.created_at)}
                </TableCell>
                <TableCell className="text-muted-foreground tabular">
                  {invite.expires_at === null ? t("Never") : dateTime(invite.expires_at)}
                </TableCell>
                <TableCell className="text-right">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("Delete the invite")}
                    onClick={() => setDeleting(invite)}
                  >
                    <Icon name="delete" />
                  </Button>
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
      <Confirm
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("Delete this invite?")}
        description={
          deleting?.used_at != null
            ? t("Only the record goes away; the account made with it stays.")
            : t("The code stops working.")
        }
        action={t("Delete")}
        destructive
        onConfirm={() => deleting && remove.mutate(deleting)}
      />
    </Tile>
  );
}

export function PlayersPage() {
  const { t } = useI18n();
  const users = useUsers();
  const [tab, setTab] = useState("accounts");
  const [creating, setCreating] = useState<"account" | "invite" | null>(null);
  const waiting = (users.data ?? []).filter((user) => user.status === "pending").length;

  return (
    <>
      <PageHeader
        title={t("Players")}
        description={t("Accounts registered on this server. The username is the in-game name.")}
        actions={
          tab === "accounts" ? (
            <Button onClick={() => setCreating("account")}>
              <Icon name="plus" />
              {t("New account")}
            </Button>
          ) : (
            <Button onClick={() => setCreating("invite")}>
              <Icon name="plus" />
              {t("New invite")}
            </Button>
          )
        }
      />
      <Tabs value={tab} onValueChange={setTab} className="gap-4">
        <TabsList>
          <TabsTrigger value="accounts">
            <Icon name="users" />
            {t("Accounts")}
            {waiting > 0 && <Badge className="ml-1">{waiting}</Badge>}
          </TabsTrigger>
          <TabsTrigger value="invites">
            <Icon name="key" />
            {t("Invites")}
          </TabsTrigger>
        </TabsList>
        <TabsContent value="accounts">
          <Accounts />
        </TabsContent>
        <TabsContent value="invites">
          <Invites />
        </TabsContent>
      </Tabs>

      {creating === "account" && <NewAccount onClose={() => setCreating(null)} />}
      {creating === "invite" && <NewInviteDialog onClose={() => setCreating(null)} />}
    </>
  );
}
