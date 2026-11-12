import { STATUS_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import type { InstanceStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

const TONE: Record<InstanceStatus, string> = {
  running: "text-ok",
  starting: "text-warn",
  installing: "text-warn",
  stopping: "text-warn",
  crashed: "text-destructive",
  stopped: "text-muted-foreground",
};

export function StatusDot({ status, className }: { status: InstanceStatus; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block size-2 shrink-0 bg-current",
        TONE[status],
        status === "running" && "status-live",
        className,
      )}
    />
  );
}

export function StatusLabel({ status, className }: { status: InstanceStatus; className?: string }) {
  const { t } = useI18n();
  return (
    <span className={cn("inline-flex items-center gap-2 text-sm font-medium", className)}>
      <StatusDot status={status} />
      <span className={TONE[status]}>{t(STATUS_NAMES[status])}</span>
    </span>
  );
}
