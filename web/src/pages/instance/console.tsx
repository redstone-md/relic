import { useQuery } from "@tanstack/react-query";
import { type FormEvent, type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";

import { Help, Loading, Tile, TileLabel } from "@/components/common";
import { Bars } from "@/components/graph";
import { Icon, type IconName } from "@/components/icon";
import { StartPrompt } from "@/components/power";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { socketUrl } from "@/lib/api";
import { bytes, clock, dateTime, halted, since } from "@/lib/format";
import { contentUrl, readText } from "@/lib/fs";
import { useSamples } from "@/lib/history";
import { useI18n } from "@/lib/i18n";
import { useFolder } from "@/lib/queries";
import type { ConsoleLine, FsEntry, Instance } from "@/lib/types";
import { cn } from "@/lib/utils";

const CAP = 2000;
const HISTORY = 50;

type Incoming =
  | { type: "history"; lines: ConsoleLine[] }
  | { type: "line"; line: ConsoleLine }
  | { type: "error"; message: string };

type Link = "connecting" | "open" | "lost";

function tone(line: ConsoleLine): string | undefined {
  if (line.system) return "text-primary";
  if (/\b(ERROR|FATAL|SEVERE)\b/.test(line.text) || /^\s+at\s|^Caused by:|Exception/.test(line.text)) {
    return "text-destructive";
  }
  if (/\bWARN(ING)?\b/.test(line.text)) return "text-warn";
  return undefined;
}

/** Live console over one socket; falls back to reconnecting when it drops. */
function useConsole(id: string) {
  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [link, setLink] = useState<Link>("connecting");
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let frame = 0;
    let attempt = 0;
    let closed = false;
    let buffer: ConsoleLine[] = [];
    let replace = false;

    // Bursts of output (startup, stack traces) land once per frame, not per line.
    const flush = () => {
      frame = 0;
      const batch = buffer;
      const fresh = replace;
      buffer = [];
      replace = false;
      setLines((previous) => {
        const last = fresh ? -1 : (previous.at(-1)?.seq ?? -1);
        const next = fresh ? batch : [...previous, ...batch.filter((line) => line.seq > last)];
        return next.length > CAP ? next.slice(-CAP) : next;
      });
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(flush);
    };

    const connect = () => {
      setLink("connecting");
      const socket = new WebSocket(socketUrl(`/admin/instances/${id}/console`));
      socketRef.current = socket;
      socket.onopen = () => {
        attempt = 0;
        setLink("open");
      };
      socket.onmessage = (raw) => {
        let event: Incoming;
        try {
          event = JSON.parse(String(raw.data));
        } catch {
          return;
        }
        if (event.type === "history") {
          buffer = event.lines;
          replace = true;
        } else if (event.type === "line") {
          buffer.push(event.line);
        } else {
          buffer.push({
            seq: Number.MAX_SAFE_INTEGER,
            time: Math.floor(Date.now() / 1000),
            text: event.message,
            system: true,
          });
        }
        schedule();
      };
      socket.onclose = () => {
        if (closed) return;
        setLink("lost");
        attempt += 1;
        timer = setTimeout(connect, Math.min(10000, 500 * 2 ** attempt));
      };
    };
    connect();

    return () => {
      closed = true;
      clearTimeout(timer);
      cancelAnimationFrame(frame);
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [id]);

  const send = (line: string): boolean => {
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) return false;
    socket.send(JSON.stringify({ type: "command", line }));
    return true;
  };

  return { lines, link, send, clear: () => setLines([]) };
}

export function ConsoleTab({ instance }: { instance: Instance }) {
  const { t, plural } = useI18n();
  const { lines, link, send, clear } = useConsole(instance.id);
  const [command, setCommand] = useState("");
  const [follow, setFollow] = useState(true);
  const history = useRef<string[]>([]);
  const cursor = useRef(-1);
  const viewport = useRef<HTMLDivElement>(null);

  const status = instance.runtime.status;
  const accepting = status === "running" || status === "starting";

  useLayoutEffect(() => {
    const node = viewport.current;
    if (node && follow) node.scrollTop = node.scrollHeight;
  }, [lines, follow]);

  const onScroll = () => {
    const node = viewport.current;
    if (!node) return;
    setFollow(node.scrollHeight - node.scrollTop - node.clientHeight < 32);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const line = command.trim();
    if (!line || !send(line)) return;
    history.current = [line, ...history.current.filter((entry) => entry !== line)].slice(0, HISTORY);
    cursor.current = -1;
    setCommand("");
    setFollow(true);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    const next = cursor.current + (event.key === "ArrowUp" ? 1 : -1);
    if (next < -1 || next >= history.current.length) return;
    event.preventDefault();
    cursor.current = next;
    setCommand(next === -1 ? "" : history.current[next]!);
  };

  return (
    <>
      <div className="grid gap-(--frame) lg:grid-cols-[minmax(0,1fr)_19rem]">
        <section className="relative flex min-w-0 flex-col bg-console text-console-foreground">
          <div className="flex items-center justify-between gap-3 border-b border-white/10 px-4 py-2 text-xs">
            <span className="flex items-center gap-2 text-console-foreground/60">
              <span
                aria-hidden
                className={cn(
                  "size-2",
                  link === "open" ? "bg-ok" : link === "lost" ? "bg-destructive" : "bg-warn",
                )}
              />
              {link === "open" ? t("Live") : link === "lost" ? t("Reconnecting") : t("Connecting")}
              <span className="font-mono tabular">
                {plural(lines.length, "{count} line", "{count} lines")}
              </span>
            </span>
            <Button
              variant="ghost"
              size="xs"
              onClick={clear}
              className="text-console-foreground/70 hover:bg-white/10 hover:text-console-foreground"
            >
              <Icon name="delete" />
              {t("Clear view")}
            </Button>
          </div>

          <div
            ref={viewport}
            onScroll={onScroll}
            role="log"
            aria-label={t("{name} console", { name: instance.name })}
            aria-live="off"
            tabIndex={0}
            className="h-[min(58dvh,44rem)] min-h-80 flex-1 overflow-y-auto px-4 py-3 font-mono text-[13px] leading-5 outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          >
            {lines.length === 0 ? (
              halted(status) ? (
                <StartPrompt
                  instance={instance}
                  className="h-full"
                  title={t("The server is stopped")}
                >
                  {t("Its output appears here once it starts.")}
                </StartPrompt>
              ) : (
                <p className="text-console-foreground/50">{t("Waiting for output.")}</p>
              )
            ) : (
              lines.map((line, index) => (
                <div key={`${line.seq}-${index}`} className="flex gap-3">
                  <span className="shrink-0 text-console-foreground/35 select-none tabular">
                    {clock(line.time)}
                  </span>
                  <span className={cn("min-w-0 break-words whitespace-pre-wrap", tone(line))}>
                    {line.text}
                  </span>
                </div>
              ))
            )}
          </div>

          {!follow && (
            <Button
              size="sm"
              variant="secondary"
              className="absolute right-4 bottom-16"
              onClick={() => setFollow(true)}
            >
              <Icon name="chevron-down" />
              {t("Jump to latest")}
            </Button>
          )}

          <form
            onSubmit={submit}
            className="flex items-center gap-3 border-t border-white/10 bg-console pl-4"
          >
            <span aria-hidden className="font-mono text-primary select-none">
              &gt;
            </span>
            <input
              value={command}
              onChange={(event) => setCommand(event.target.value)}
              onKeyDown={onKeyDown}
              disabled={!accepting}
              aria-label={t("Server command")}
              placeholder={
                accepting
                  ? t("Type a command, e.g. say hello")
                  : t("Start the server to send commands")
              }
              autoComplete="off"
              spellCheck={false}
              className="h-11 min-w-0 flex-1 bg-transparent font-mono text-[13px] outline-none placeholder:text-console-foreground/40 disabled:cursor-not-allowed"
            />
            <button
              type="submit"
              data-icon-trigger
              disabled={!accepting || link !== "open" || command.trim() === ""}
              className="flex h-11 cursor-pointer items-center gap-2 bg-primary px-5 text-sm font-medium text-primary-foreground outline-none transition-colors hover:bg-primary/85 focus-visible:ring-2 focus-visible:ring-console-foreground focus-visible:ring-inset disabled:cursor-not-allowed disabled:bg-white/10 disabled:text-console-foreground/40"
            >
              <Icon name="zap" />
              {t("Send")}
            </button>
          </form>
        </section>

        <Side instance={instance} />
      </div>

      <Crashes instance={instance} />

      <Help>
        <p>
          {t(
            "The console shows what the server prints and takes the same commands an operator types in chat, without the slash.",
          )}
        </p>
        <p>
          {t(
            "The graphs cover the last three minutes and are measured by this page, so they start empty when you open it.",
          )}
        </p>
      </Help>
    </>
  );
}

function Graph({
  icon,
  label,
  now,
  note,
  values,
  max,
}: {
  icon: IconName;
  label: string;
  now: string;
  note?: string;
  values: number[];
  max: number;
}) {
  const { t } = useI18n();
  return (
    <Tile className="gap-2 p-4 lg:flex-1">
      <TileLabel icon={icon}>{label}</TileLabel>
      <div className="flex items-baseline justify-between gap-3">
        <p className="truncate font-pixel text-sm leading-6 tabular">{now}</p>
        {note && <p className="truncate text-xs text-muted-foreground">{note}</p>}
      </div>
      <Bars
        values={values}
        max={max}
        label={t("{label}, last three minutes", { label })}
        className="mt-auto h-14"
      />
    </Tile>
  );
}

/** Beside the console: the load of the last three minutes, and who is on. */
function Side({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const samples = useSamples();
  const last = samples.at(-1);
  const { runtime } = instance;
  const running = runtime.status === "running";
  const limit = instance.memory_mb * 1048576;
  const used = last?.instances[instance.id];
  const players = samples.map((sample) => sample.players[instance.id] ?? 0);

  return (
    <aside className="grid content-start gap-(--frame) sm:grid-cols-2 lg:flex lg:flex-col">
      <Graph
        icon="cpu"
        label={t("Host processor")}
        now={last ? `${last.cpu.toFixed(0)}%` : "—"}
        values={samples.map((sample) => sample.cpu)}
        max={100}
      />
      <Graph
        icon="gauge"
        label={t("Memory")}
        now={used ? bytes(used) : "—"}
        note={t("Limit {size}", { size: bytes(limit) })}
        values={samples.map((sample) => sample.instances[instance.id] ?? 0)}
        max={limit}
      />
      <Graph
        icon="users"
        label={t("Players")}
        now={running ? `${runtime.players.length} / ${runtime.players_max}` : "—"}
        note={
          !running
            ? t("Nobody can join yet")
            : runtime.players.length > 0
              ? runtime.players.join(", ")
              : t("Nobody is playing")
        }
        values={players}
        max={Math.max(runtime.players_max, 1)}
      />
      <Tile className="gap-2 p-4">
        <TileLabel icon="clock">{t("Uptime")}</TileLabel>
        <div className="flex items-baseline justify-between gap-3">
          <p className="truncate font-pixel text-sm leading-6 tabular">
            {running && runtime.started_at ? since(runtime.started_at) : "—"}
          </p>
          {running && runtime.started_at && (
            <p className="truncate text-xs text-muted-foreground">
              {t("Since {date}", { date: dateTime(runtime.started_at) })}
            </p>
          )}
        </div>
      </Tile>
    </aside>
  );
}

const CRASH_FOLDER = "crash-reports";
const CRASHES_SHOWN = 5;

/** The newest crash reports the game left behind, each a click from being read. */
function Crashes({ instance }: { instance: Instance }) {
  const { t, plural } = useI18n();
  const folder = useFolder(instance.id, CRASH_FOLDER);
  const [reading, setReading] = useState<FsEntry | null>(null);
  const report = useQuery({
    queryKey: ["crash", instance.id, reading?.name],
    queryFn: () => readText(instance.id, `${CRASH_FOLDER}/${reading!.name}`),
    enabled: reading !== null,
    staleTime: Infinity,
  });

  const reports = (folder.data ?? [])
    .filter((entry) => !entry.dir && entry.name.endsWith(".txt"))
    .sort((a, b) => b.modified - a.modified);
  const shown = reports.slice(0, CRASHES_SHOWN);

  return (
    <Tile className="gap-3">
      <div className="flex items-baseline justify-between gap-3">
        <TileLabel icon="crash">{t("Recent crashes")}</TileLabel>
        {reports.length > CRASHES_SHOWN && (
          <span className="text-xs text-muted-foreground">
            {plural(
              reports.length - CRASHES_SHOWN,
              "{count} older one in Files, crash-reports",
              "{count} older ones in Files, crash-reports",
            )}
          </span>
        )}
      </div>
      {folder.isPending ? (
        <Loading />
      ) : shown.length === 0 ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Icon name="circle-check" className="text-ok" />
          {t("No crash reports. The game has not crashed on this server.")}
        </p>
      ) : (
        <ul className="grid gap-(--frame) sm:grid-cols-2 xl:grid-cols-5">
          {shown.map((entry) => (
            <li key={entry.name} className="flex min-w-0 border">
              <button
                type="button"
                data-icon-trigger
                onClick={() => setReading(entry)}
                className="min-w-0 flex-1 cursor-pointer p-3 text-left outline-none transition-colors hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
              >
                <span className="block truncate text-sm font-medium">
                  {entry.modified ? dateTime(entry.modified) : entry.name}
                </span>
                <span className="block truncate font-mono text-xs text-muted-foreground">
                  {bytes(entry.size)}
                </span>
              </button>
              <a
                href={contentUrl(instance.id, `${CRASH_FOLDER}/${entry.name}`)}
                download={entry.name}
                data-icon-trigger
                aria-label={t("Download {name}", { name: entry.name })}
                title={t("Download")}
                className="grid w-10 shrink-0 place-items-center border-l text-muted-foreground outline-none transition-colors hover:bg-secondary hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
              >
                <Icon name="download" />
              </a>
            </li>
          ))}
        </ul>
      )}

      <Dialog open={reading !== null} onOpenChange={(open) => !open && setReading(null)}>
        <DialogContent className="flex h-[min(44rem,90dvh)] flex-col sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle className="truncate font-mono text-sm">{reading?.name}</DialogTitle>
            <DialogDescription>
              {reading?.modified ? dateTime(reading.modified) : ""}
            </DialogDescription>
          </DialogHeader>
          {report.isPending ? (
            <Loading />
          ) : (
            <pre className="min-h-0 flex-1 overflow-auto bg-console p-3 font-mono text-xs leading-5 text-console-foreground">
              {report.data ?? t("The file could not be read")}
            </pre>
          )}
        </DialogContent>
      </Dialog>
    </Tile>
  );
}
