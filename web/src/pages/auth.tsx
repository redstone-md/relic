import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useState } from "react";

import { ActionButton, ErrorNote, Field } from "@/components/common";
import { Icon } from "@/components/icon";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import { keys } from "@/lib/queries";
import type { User } from "@/lib/types";

function Frame({ title, lead, children }: { title: string; lead: ReactNode; children: ReactNode }) {
  return (
    <div className="grid min-h-dvh place-items-center p-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex items-center gap-3" data-icon-trigger>
          <span className="grid size-10 place-items-center rounded-xl bg-primary text-primary-foreground">
            <Icon name="pickaxe" size={22} appear />
          </span>
          <span className="font-pixel text-lg">Relic</span>
        </div>
        <div className="bg-background p-6">
          <h1 className="text-lg font-semibold">{title}</h1>
          <p className="mt-1 mb-5 text-sm text-muted-foreground">{lead}</p>
          {children}
        </div>
      </div>
    </div>
  );
}

export function Setup() {
  const { t } = useI18n();
  const client = useQueryClient();
  const [token, setToken] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");

  const setup = useMutation({
    mutationFn: () =>
      api<User>("POST", "/setup", { json: { token: token.trim(), username, password } }),
    onSuccess: (user) => {
      client.setQueryData(keys.me, user);
      client.setQueryData(keys.setup, { needed: false });
    },
  });

  const mismatch = repeat !== "" && repeat !== password;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (password !== repeat) return;
    setup.mutate();
  };

  return (
    <Frame
      title={t("First-run setup")}
      lead={t(
        "Create the administrator account. The setup token was printed to the daemon's terminal when it started.",
      )}
    >
      <form onSubmit={submit} className="grid gap-4">
        <Field label={t("Setup token")} htmlFor="token">
          <Input
            id="token"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            placeholder="XXXX-XXXX-XXXX-XXXX"
            className="font-mono uppercase"
            autoComplete="off"
            spellCheck={false}
            required
            autoFocus
          />
        </Field>
        <Field
          label={t("Username")}
          htmlFor="username"
          hint={t("3-16 characters: letters, digits, underscore. Also your in-game name.")}
        >
          <Input
            id="username"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            autoComplete="username"
            minLength={3}
            maxLength={16}
            pattern="[A-Za-z0-9_]+"
            required
          />
        </Field>
        <Field label={t("Password")} htmlFor="password">
          <Input
            id="password"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            autoComplete="new-password"
            minLength={8}
            required
          />
        </Field>
        <Field label={t("Repeat password")} htmlFor="repeat">
          <Input
            id="repeat"
            type="password"
            value={repeat}
            onChange={(event) => setRepeat(event.target.value)}
            autoComplete="new-password"
            aria-invalid={mismatch}
            required
          />
          {mismatch && <p className="text-xs text-destructive">{t("The passwords differ.")}</p>}
        </Field>
        {setup.error && <ErrorNote error={setup.error} />}
        <ActionButton type="submit" icon="shield" busy={setup.isPending} size="lg">
          {t("Create administrator")}
        </ActionButton>
      </form>
    </Frame>
  );
}

export function Login() {
  const { t } = useI18n();
  const client = useQueryClient();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const login = useMutation({
    mutationFn: () =>
      api<{ user: User }>("POST", "/auth/login", { json: { username, password, admin: true } }),
    onSuccess: ({ user }) => client.setQueryData(keys.me, user),
  });

  return (
    <Frame title={t("Sign in")} lead={t("Administrator access to this server.")}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          login.mutate();
        }}
        className="grid gap-4"
      >
        <Field label={t("Username")} htmlFor="username">
          <Input
            id="username"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            autoComplete="username"
            required
            autoFocus
          />
        </Field>
        <Field label={t("Password")} htmlFor="password">
          <Input
            id="password"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            autoComplete="current-password"
            required
          />
        </Field>
        {login.error && <ErrorNote error={login.error} />}
        <ActionButton type="submit" icon="key" busy={login.isPending} size="lg">
          {t("Sign in")}
        </ActionButton>
      </form>
    </Frame>
  );
}
