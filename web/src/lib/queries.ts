import {
  type QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useEffect } from "react";
import { toast } from "sonner";

import { api, ApiError, message, socketUrl } from "@/lib/api";
import type {
  BackupInfo,
  FileRecord,
  FsEntry,
  FsUsage,
  Instance,
  InstanceUpdate,
  Invite,
  LoaderKind,
  LoaderVersion,
  NewsItem,
  RuntimeInfo,
  Settings,
  SystemInfo,
  User,
} from "@/lib/types";

export const keys = {
  setup: ["setup"] as const,
  me: ["me"] as const,
  instances: ["instances"] as const,
  system: ["system"] as const,
  files: (id: string) => ["files", id] as const,
  fs: (id: string, folder: string) => ["fs", id, folder] as const,
  fsText: (id: string, path: string) => ["fs-text", id, path] as const,
  fsUsage: (id: string) => ["fs-usage", id] as const,
  backups: (id: string) => ["backups", id] as const,
  settings: ["settings"] as const,
  news: ["news"] as const,
  users: ["users"] as const,
  invites: ["invites"] as const,
  minecraftVersions: ["meta", "minecraft"] as const,
  loaderVersions: (kind: LoaderKind, mc: string) => ["meta", "loader", kind, mc] as const,
};

export function useSetupStatus() {
  return useQuery({
    queryKey: keys.setup,
    queryFn: () => api<{ needed: boolean }>("GET", "/setup"),
    staleTime: Infinity,
  });
}

/** The signed-in admin, or `null` when nobody is signed in. */
export function useMe() {
  return useQuery({
    queryKey: keys.me,
    queryFn: async () => {
      try {
        const user = await api<User>("GET", "/auth/me");
        return user.role === "admin" ? user : null;
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) return null;
        throw error;
      }
    },
    staleTime: Infinity,
    retry: false,
  });
}

export function useInstances() {
  return useQuery({
    queryKey: keys.instances,
    queryFn: () => api<Instance[]>("GET", "/admin/instances"),
  });
}

export function useInstance(id: string): Instance | undefined {
  return useInstances().data?.find((instance) => instance.id === id);
}

export function useSystem() {
  return useQuery({
    queryKey: keys.system,
    queryFn: () => api<SystemInfo>("GET", "/admin/system"),
    refetchInterval: 3000,
  });
}

export function useFiles(id: string) {
  return useQuery({
    queryKey: keys.files(id),
    queryFn: () => api<FileRecord[]>("GET", `/admin/instances/${id}/files`),
  });
}

/** What a folder of the instance holds; an empty list when it is not there. */
export function useFolder(id: string, folder: string, enabled = true) {
  return useQuery({
    queryKey: keys.fs(id, folder),
    enabled,
    queryFn: async () => {
      try {
        return await api<FsEntry[]>("GET", `/admin/instances/${id}/fs`, { query: { path: folder } });
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return [];
        throw error;
      }
    },
  });
}

export function useUsage(id: string) {
  return useQuery({
    queryKey: keys.fsUsage(id),
    queryFn: () => api<FsUsage>("GET", `/admin/instances/${id}/fs/usage`),
    staleTime: 30_000,
  });
}

export function useBackups(id: string) {
  return useQuery({
    queryKey: keys.backups(id),
    queryFn: () => api<BackupInfo[]>("GET", `/admin/instances/${id}/backups`),
  });
}

export function useSettings() {
  return useQuery({
    queryKey: keys.settings,
    queryFn: () => api<Settings>("GET", "/admin/settings"),
  });
}

export function useNews() {
  return useQuery({ queryKey: keys.news, queryFn: () => api<NewsItem[]>("GET", "/news") });
}

export function useUsers() {
  return useQuery({ queryKey: keys.users, queryFn: () => api<User[]>("GET", "/admin/users") });
}

export function useInvites() {
  return useQuery({ queryKey: keys.invites, queryFn: () => api<Invite[]>("GET", "/admin/invites") });
}

export function useMinecraftVersions() {
  return useQuery({
    queryKey: keys.minecraftVersions,
    queryFn: () => api<string[]>("GET", "/admin/meta/minecraft-versions"),
    staleTime: 10 * 60 * 1000,
  });
}

export function useLoaderVersions(kind: LoaderKind, mc: string) {
  return useQuery({
    queryKey: keys.loaderVersions(kind, mc),
    queryFn: () =>
      api<LoaderVersion[]>("GET", "/admin/meta/loader-versions", { query: { kind, mc } }),
    enabled: kind !== "vanilla" && mc !== "",
    staleTime: 10 * 60 * 1000,
    retry: false,
  });
}

function patchRuntime(client: QueryClient, id: string, runtime: RuntimeInfo) {
  client.setQueryData<Instance[]>(keys.instances, (list) =>
    list?.map((instance) => (instance.id === id ? { ...instance, runtime } : instance)),
  );
}

type LiveEvent =
  | { type: "runtime"; instance_id: string; info: RuntimeInfo }
  | { type: "resync" };

/**
 * Keeps the instance list in step with the daemon over one socket for the
 * whole panel. Reconnects on its own and refetches after every gap.
 */
export function useLiveEvents(enabled: boolean) {
  const client = useQueryClient();
  useEffect(() => {
    if (!enabled) return;
    let socket: WebSocket | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let attempt = 0;
    let closed = false;

    const connect = () => {
      socket = new WebSocket(socketUrl("/admin/events"));
      socket.onopen = () => {
        if (attempt > 0) void client.invalidateQueries({ queryKey: keys.instances });
        attempt = 0;
      };
      socket.onmessage = (raw) => {
        let event: LiveEvent;
        try {
          event = JSON.parse(String(raw.data));
        } catch {
          return;
        }
        if (event.type === "runtime") {
          const before = client
            .getQueryData<Instance[]>(keys.instances)
            ?.find((instance) => instance.id === event.instance_id);
          patchRuntime(client, event.instance_id, event.info);
          // Install and EULA state live outside the runtime record.
          if (before && before.runtime.status !== event.info.status) {
            void client.invalidateQueries({ queryKey: keys.instances });
          }
        } else if (event.type === "resync") {
          void client.invalidateQueries({ queryKey: keys.instances });
        }
      };
      socket.onclose = () => {
        if (closed) return;
        attempt += 1;
        timer = setTimeout(connect, Math.min(15000, 500 * 2 ** attempt));
      };
    };
    connect();

    return () => {
      closed = true;
      clearTimeout(timer);
      socket?.close();
    };
  }, [client, enabled]);
}

type Power = "start" | "stop" | "restart";

const POWER_DONE: Record<Power, string> = {
  start: "Starting",
  stop: "Stopping",
  restart: "Restarting",
};

export function usePower(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (action: Power) => api("POST", `/admin/instances/${id}/${action}`),
    onSuccess: (_, action) => {
      toast.success(`${POWER_DONE[action]} ${id}`);
      void client.invalidateQueries({ queryKey: keys.instances });
    },
    onError: (error) => toast.error(message(error)),
  });
}

export function useAcceptEula(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api("POST", `/admin/instances/${id}/eula`, { json: { accept: true } }),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.instances }),
    onError: (error) => toast.error(message(error)),
  });
}

export function useUpdateInstance(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (update: InstanceUpdate) =>
      api<Instance>("PATCH", `/admin/instances/${id}`, { json: update }),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.instances }),
    onError: (error) => toast.error(message(error)),
  });
}
