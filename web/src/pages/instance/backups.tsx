import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { ActionButton, Confirm, Empty, ErrorNote, Loading, Tile } from "@/components/common";
import { Icon } from "@/components/icon";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { api, downloadUrl, message } from "@/lib/api";
import { bytes, dateTime, halted } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useBackups } from "@/lib/queries";
import type { BackupInfo, Instance } from "@/lib/types";

export function BackupsTab({ instance }: { instance: Instance }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const backups = useBackups(instance.id);
  const [restoring, setRestoring] = useState<BackupInfo | null>(null);
  const [deleting, setDeleting] = useState<BackupInfo | null>(null);
  const base = `/admin/instances/${instance.id}/backups`;
  const refresh = () => client.invalidateQueries({ queryKey: keys.backups(instance.id) });

  const create = useMutation({
    mutationFn: () => api<BackupInfo>("POST", base),
    onSuccess: (backup) => {
      toast.success(t("Backup saved, {size}", { size: bytes(backup.size) }));
      void refresh();
    },
    onError: (error) => toast.error(message(error)),
  });
  const restore = useMutation({
    mutationFn: (backup: BackupInfo) => api("POST", `${base}/${backup.id}/restore`),
    onSuccess: () => toast.success(t("World restored from backup")),
    onError: (error) => toast.error(message(error)),
  });
  const remove = useMutation({
    mutationFn: (backup: BackupInfo) => api("DELETE", `${base}/${backup.id}`),
    onSuccess: () => {
      toast.success(t("Backup deleted"));
      void refresh();
    },
    onError: (error) => toast.error(message(error)),
  });

  const stopped = halted(instance.runtime.status);

  return (
    <Tile className="gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-semibold">{t("World backups")}</h2>
          <p className="text-sm text-muted-foreground">
            {t(
              "A running server pauses saving for the copy, so players are not interrupted. The schedule and retention live in Settings.",
            )}
          </p>
        </div>
        <ActionButton icon="backup" busy={create.isPending} onClick={() => create.mutate()}>
          {create.isPending ? t("Backing up") : t("Back up now")}
        </ActionButton>
      </div>

      {backups.isPending ? (
        <Loading />
      ) : backups.error ? (
        <ErrorNote error={backups.error} />
      ) : backups.data.length === 0 ? (
        <Empty icon="archive" title={t("No backups yet")}>
          {t("Take one before changing mods or updating the Minecraft version.")}
        </Empty>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("Taken")}</TableHead>
              <TableHead className="text-right">{t("Size")}</TableHead>
              <TableHead className="w-0" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {backups.data.map((backup) => (
              <TableRow key={backup.id}>
                <TableCell>
                  <span className="font-medium">{dateTime(backup.created_at)}</span>
                  <span className="ml-3 font-mono text-xs text-muted-foreground">{backup.id}</span>
                </TableCell>
                <TableCell className="text-right font-mono tabular">{bytes(backup.size)}</TableCell>
                <TableCell>
                  <div className="flex justify-end gap-1">
                    <Button asChild variant="ghost" size="sm">
                      <a href={downloadUrl(`${base}/${backup.id}/download`)} download>
                        <Icon name="download" />
                        {t("Download")}
                      </a>
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={!stopped || restore.isPending}
                      title={stopped ? undefined : t("Stop the server to restore")}
                      onClick={() => setRestoring(backup)}
                    >
                      <Icon name="history" />
                      {t("Restore")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t("Delete backup")}
                      onClick={() => setDeleting(backup)}
                    >
                      <Icon name="delete" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}

      {!stopped && backups.data && backups.data.length > 0 && (
        <p className="text-xs text-muted-foreground">{t("Stop the server to restore a backup.")}</p>
      )}

      <Confirm
        open={restoring !== null}
        onOpenChange={(open) => !open && setRestoring(null)}
        title={t("Restore this backup?")}
        description={
          restoring &&
          t(
            "The current world is replaced with the copy from {date}. Progress made since then is lost.",
            { date: dateTime(restoring.created_at) },
          )
        }
        action={t("Restore")}
        destructive
        onConfirm={() => restoring && restore.mutate(restoring)}
      />
      <Confirm
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("Delete this backup?")}
        description={
          deleting &&
          t("The copy from {date} is removed from disk.", { date: dateTime(deleting.created_at) })
        }
        action={t("Delete")}
        destructive
        onConfirm={() => deleting && remove.mutate(deleting)}
      />
    </Tile>
  );
}
