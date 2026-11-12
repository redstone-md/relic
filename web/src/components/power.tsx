import { type ReactNode, useState } from "react";

import { Confirm } from "@/components/common";
import { Icon, type IconName } from "@/components/icon";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { halted } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { useAcceptEula, usePower } from "@/lib/queries";
import type { Instance } from "@/lib/types";
import { cn } from "@/lib/utils";

function PowerButton({
  icon,
  label,
  compact,
  primary = false,
  busy = false,
  disabled = false,
  onClick,
}: {
  icon: IconName;
  label: string;
  compact: boolean;
  primary?: boolean;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  const glyph = busy ? <Icon name="spinner" active /> : <Icon name={icon} />;
  if (!compact) {
    return (
      <Button
        variant={primary ? "default" : "outline"}
        disabled={disabled || busy}
        onClick={onClick}
      >
        {glyph}
        {label}
      </Button>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant={primary ? "default" : "outline"}
          size="icon-sm"
          aria-label={label}
          disabled={disabled || busy}
          onClick={onClick}
        >
          {glyph}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function usePowerActions(instance: Instance) {
  const { t } = useI18n();
  const power = usePower(instance.id);
  const eula = useAcceptEula(instance.id);
  const [asking, setAsking] = useState(false);
  const status = instance.runtime.status;
  const pending = power.isPending || eula.isPending;

  const start = () => {
    if (instance.eula_accepted) power.mutate("start");
    else setAsking(true);
  };

  const gate = (
    <Confirm
      open={asking}
      onOpenChange={setAsking}
      title={t("Accept the Minecraft EULA")}
      description={
        <>
          {t("A Minecraft server only starts once you agree to the")}{" "}
          <a
            href="https://aka.ms/MinecraftEULA"
            target="_blank"
            rel="noreferrer"
            className="font-medium text-foreground underline underline-offset-4"
          >
            {t("Minecraft End User License Agreement")}
          </a>
          . {t("Accepting here writes eula=true for {name}.", { name: instance.name })}
        </>
      }
      action={t("Accept and start")}
      onConfirm={() => eula.mutate(undefined, { onSuccess: () => power.mutate("start") })}
    />
  );

  return {
    t,
    status,
    pending,
    gate,
    start,
    restart: () => power.mutate("restart"),
    stop: () => power.mutate("stop"),
  };
}

/** What a page shows in place of live data while the server is down. */
export function StartPrompt({
  instance,
  title,
  children,
  className,
}: {
  instance: Instance;
  title: string;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 px-6 py-10 text-center",
        className,
      )}
    >
      <Icon name="moon" size={28} className="opacity-60" appear />
      <p className="font-medium">{title}</p>
      {children && <p className="max-w-md text-sm opacity-70">{children}</p>}
      {halted(instance.runtime.status) && (
        <div className="mt-1">
          <PowerControls instance={instance} />
        </div>
      )}
    </div>
  );
}

/** Start, stop and restart for one instance, with the EULA gate on first start. */
export function PowerControls({
  instance,
  compact = false,
}: {
  instance: Instance;
  compact?: boolean;
}) {
  const { t, status, pending, gate, start, restart, stop } = usePowerActions(instance);

  return (
    <div className="flex items-center gap-2">
      {halted(status) ? (
        <PowerButton
          icon="play"
          label={t("Start")}
          compact={compact}
          primary
          busy={pending}
          onClick={start}
        />
      ) : (
        <>
          <PowerButton
            icon="restart"
            label={t("Restart")}
            compact={compact}
            disabled={status !== "running" || pending}
            onClick={restart}
          />
          <PowerButton
            icon="stop"
            label={t("Stop")}
            compact={compact}
            busy={status === "stopping"}
            disabled={status === "installing" || pending}
            onClick={stop}
          />
        </>
      )}
      {gate}
    </div>
  );
}

function PowerBlock({
  icon,
  label,
  tone,
  busy = false,
  disabled = false,
  onClick,
}: {
  icon: IconName;
  label: string;
  tone: string;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      data-icon-trigger
      disabled={disabled || busy}
      onClick={onClick}
      className={cn(
        "flex h-11 min-w-0 flex-1 cursor-pointer items-center justify-center gap-2 bg-background px-3 text-sm font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset disabled:cursor-not-allowed disabled:text-muted-foreground/60",
        !(disabled || busy) && tone,
      )}
    >
      {busy ? <Icon name="spinner" active /> : <Icon name={icon} />}
      {label}
    </button>
  );
}

/** The three power tiles of a server's page: all there at once, the idle ones dimmed. */
export function PowerBlocks({ instance }: { instance: Instance }) {
  const { t, status, pending, gate, start, restart, stop } = usePowerActions(instance);
  const down = halted(status);

  return (
    <div className="flex gap-(--frame)">
      <PowerBlock
        icon="play"
        label={t("Start")}
        tone="bg-primary text-primary-foreground hover:bg-primary/85"
        busy={down && pending}
        disabled={!down}
        onClick={start}
      />
      <PowerBlock
        icon="restart"
        label={t("Restart")}
        tone="hover:bg-secondary"
        disabled={status !== "running" || pending}
        onClick={restart}
      />
      <PowerBlock
        icon="stop"
        label={t("Stop")}
        tone="text-destructive hover:bg-destructive hover:text-background"
        busy={status === "stopping"}
        disabled={down || status === "installing" || pending}
        onClick={stop}
      />
      {gate}
    </div>
  );
}
