import type { ComponentProps, ReactNode } from "react";

import { Icon, type IconName } from "@/components/icon";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { message } from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** One cell of a bento grid. The lines between cells are the board showing through. */
export function Tile({ className, ...props }: ComponentProps<"section">) {
  return (
    <section
      className={cn(
        "relative flex min-w-0 flex-col bg-background p-5 text-foreground",
        className,
      )}
      {...props}
    />
  );
}

export function TileLabel({ icon, children }: { icon: IconName; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground">
      <Icon name={icon} />
      <span>{children}</span>
    </div>
  );
}

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-2 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <h1 className="truncate font-pixel text-base leading-7">{title}</h1>
        {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

export function Field({
  label,
  hint,
  htmlFor,
  children,
  className,
}: {
  label: string;
  hint?: ReactNode;
  htmlFor?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("grid gap-1.5", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}

export function Empty({
  icon,
  title,
  children,
}: {
  icon: IconName;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div
      data-icon-trigger
      className="flex flex-col items-center justify-center gap-2 bg-background px-6 py-14 text-center"
    >
      <Icon name={icon} size={28} className="text-muted-foreground" appear />
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-md text-sm text-muted-foreground">{children}</div>}
    </div>
  );
}

export function Loading({ label }: { label?: string }) {
  const { t } = useI18n();
  return (
    <div className="flex items-center justify-center gap-2 bg-background py-16 text-sm text-muted-foreground">
      <Icon name="spinner" active />
      {label ?? t("Loading")}
    </div>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  return (
    <div className="flex items-start gap-2 border-l-2 border-destructive bg-background px-4 py-3 text-sm text-destructive">
      <Icon name="alert" className="mt-0.5" appear />
      <span>{message(error)}</span>
    </div>
  );
}

/** A button that shows a spinner in place of its icon while busy. */
export function ActionButton({
  icon,
  busy = false,
  children,
  disabled,
  ...props
}: ComponentProps<typeof Button> & { icon: IconName; busy?: boolean }) {
  return (
    <Button disabled={disabled || busy} {...props}>
      {busy ? <Icon name="spinner" active /> : <Icon name={icon} />}
      {children}
    </Button>
  );
}

export function Confirm({
  open,
  onOpenChange,
  title,
  description,
  action,
  destructive = false,
  onConfirm,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: ReactNode;
  action: string;
  destructive?: boolean;
  onConfirm: () => void;
  children?: ReactNode;
}) {
  const { t } = useI18n();
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        {children}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("Cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant={destructive ? "destructive" : "default"}
            onClick={onConfirm}
          >
            {action}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** A few plain sentences under a page: what it does and what to watch for. */
export function Help({ title, children }: { title?: string; children: ReactNode }) {
  const { t } = useI18n();
  return (
    <aside className="grid gap-1.5 bg-background p-5 text-sm text-muted-foreground">
      <h2 className="font-medium text-foreground">{title ?? t("How this works")}</h2>
      <div className="grid max-w-[72ch] gap-1.5">{children}</div>
    </aside>
  );
}

/** The small heading of a tile's figure, and the figure itself in the pixel face. */
export function Stat({
  icon,
  label,
  value,
  note,
  className,
  children,
}: {
  icon: IconName;
  label: string;
  value: ReactNode;
  note?: ReactNode;
  className?: string;
  children?: ReactNode;
}) {
  return (
    <Tile className={cn("gap-2 p-4", className)}>
      <TileLabel icon={icon}>{label}</TileLabel>
      <p className="truncate font-pixel text-sm leading-6 tabular">{value}</p>
      {note && <p className="truncate text-xs text-muted-foreground">{note}</p>}
      {children}
    </Tile>
  );
}
