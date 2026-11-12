import { useReducedMotion } from "motion/react";
import {
  type ForwardRefExoticComponent,
  type HTMLAttributes,
  type RefAttributes,
  useEffect,
  useRef,
} from "react";

import { ActivityIcon } from "@/components/icons/activity";
import { ArchiveIcon } from "@/components/icons/archive";
import { ArrowLeftIcon } from "@/components/icons/arrow-left";
import { BadgeAlertIcon } from "@/components/icons/badge-alert";
import { BanIcon } from "@/components/icons/ban";
import { BellIcon } from "@/components/icons/bell";
import { BlocksIcon } from "@/components/icons/blocks";
import { BoxIcon } from "@/components/icons/box";
import { CheckIcon } from "@/components/icons/check";
import { ChevronDownIcon } from "@/components/icons/chevron-down";
import { ChevronRightIcon } from "@/components/icons/chevron-right";
import { CircleCheckIcon } from "@/components/icons/circle-check";
import { ClockIcon } from "@/components/icons/clock";
import { CloudUploadIcon } from "@/components/icons/cloud-upload";
import { CopyIcon } from "@/components/icons/copy";
import { CpuIcon } from "@/components/icons/cpu";
import { DatabaseBackupIcon } from "@/components/icons/database-backup";
import { DeleteIcon } from "@/components/icons/delete";
import { DownloadIcon } from "@/components/icons/download";
import { EarthIcon } from "@/components/icons/earth";
import { EllipsisIcon } from "@/components/icons/ellipsis";
import { ExternalLinkIcon } from "@/components/icons/external-link";
import { EyeIcon } from "@/components/icons/eye";
import { EyeOffIcon } from "@/components/icons/eye-off";
import { FileTextIcon } from "@/components/icons/file-text";
import { FolderOpenIcon } from "@/components/icons/folder-open";
import { GaugeIcon } from "@/components/icons/gauge";
import { HardDriveDownloadIcon } from "@/components/icons/hard-drive-download";
import { HistoryIcon } from "@/components/icons/history";
import { KeyIcon } from "@/components/icons/key";
import { LayersIcon } from "@/components/icons/layers";
import { LayoutGridIcon } from "@/components/icons/layout-grid";
import { LinkIcon } from "@/components/icons/link";
import { LoaderCircleIcon } from "@/components/icons/loader-circle";
import { LockIcon } from "@/components/icons/lock";
import { LogoutIcon } from "@/components/icons/logout";
import { MoonIcon } from "@/components/icons/moon";
import { PaletteIcon } from "@/components/icons/palette";
import { PauseIcon } from "@/components/icons/pause";
import { PickaxeIcon } from "@/components/icons/pickaxe";
import { PlayIcon } from "@/components/icons/play";
import { PlusIcon } from "@/components/icons/plus";
import { RefreshCWIcon } from "@/components/icons/refresh-cw";
import { RotateCWIcon } from "@/components/icons/rotate-cw";
import { SearchIcon } from "@/components/icons/search";
import { ServerIcon } from "@/components/icons/server";
import { ServerCrashIcon } from "@/components/icons/server-crash";
import { SettingsIcon } from "@/components/icons/settings";
import { ShieldCheckIcon } from "@/components/icons/shield-check";
import { SlidersHorizontalIcon } from "@/components/icons/sliders-horizontal";
import { SparklesIcon } from "@/components/icons/sparkles";
import { SquareIcon } from "@/components/icons/square";
import { SquarePenIcon } from "@/components/icons/square-pen";
import { SunIcon } from "@/components/icons/sun";
import { TerminalIcon } from "@/components/icons/terminal";
import { UploadIcon } from "@/components/icons/upload";
import { UserIcon } from "@/components/icons/user";
import { UserCheckIcon } from "@/components/icons/user-check";
import { UsersIcon } from "@/components/icons/users";
import { XIcon } from "@/components/icons/x";
import { ZapIcon } from "@/components/icons/zap";
import { cn } from "@/lib/utils";

interface Handle {
  startAnimation: () => void;
  stopAnimation: () => void;
}

type Animated = ForwardRefExoticComponent<
  HTMLAttributes<HTMLDivElement> & { size?: number } & RefAttributes<Handle>
>;

/**
 * Every icon of the panel, by the name the panel uses for it. The icon set
 * is swapped here and nowhere else.
 */
const ICONS = {
  activity: ActivityIcon,
  alert: BadgeAlertIcon,
  archive: ArchiveIcon,
  back: ArrowLeftIcon,
  backup: DatabaseBackupIcon,
  ban: BanIcon,
  bell: BellIcon,
  box: BoxIcon,
  check: CheckIcon,
  "chevron-down": ChevronDownIcon,
  "chevron-right": ChevronRightIcon,
  "circle-check": CircleCheckIcon,
  clock: ClockIcon,
  console: TerminalIcon,
  copy: CopyIcon,
  cpu: CpuIcon,
  crash: ServerCrashIcon,
  dashboard: LayoutGridIcon,
  delete: DeleteIcon,
  download: DownloadIcon,
  earth: EarthIcon,
  edit: SquarePenIcon,
  external: ExternalLinkIcon,
  eye: EyeIcon,
  "eye-off": EyeOffIcon,
  files: FolderOpenIcon,
  gauge: GaugeIcon,
  history: HistoryIcon,
  install: HardDriveDownloadIcon,
  key: KeyIcon,
  layers: LayersIcon,
  link: LinkIcon,
  lock: LockIcon,
  logout: LogoutIcon,
  mods: BlocksIcon,
  more: EllipsisIcon,
  moon: MoonIcon,
  news: FileTextIcon,
  palette: PaletteIcon,
  pause: PauseIcon,
  pickaxe: PickaxeIcon,
  play: PlayIcon,
  plus: PlusIcon,
  refresh: RefreshCWIcon,
  restart: RotateCWIcon,
  search: SearchIcon,
  server: ServerIcon,
  settings: SettingsIcon,
  shield: ShieldCheckIcon,
  sliders: SlidersHorizontalIcon,
  sparkles: SparklesIcon,
  spinner: LoaderCircleIcon,
  stop: SquareIcon,
  sun: SunIcon,
  upload: UploadIcon,
  "upload-cloud": CloudUploadIcon,
  user: UserIcon,
  "user-check": UserCheckIcon,
  users: UsersIcon,
  x: XIcon,
  zap: ZapIcon,
} satisfies Record<string, Animated>;

export type IconName = keyof typeof ICONS;

/** Elements whose hover or focus plays the icons inside them. */
const TRIGGER =
  "button, a, summary, label, [role='menuitem'], [role='tab'], [role='option'], [data-icon-trigger]";

interface IconProps {
  name: IconName;
  /** Pixels. */
  size?: number;
  className?: string;
  /** Plays while true: a spinner, or a state worth pointing at. */
  active?: boolean;
  /** Plays once when the icon appears. */
  appear?: boolean;
}

/**
 * An animated icon. It plays when the control around it is hovered or
 * focused, when `active` turns on, and stays still for people who asked
 * their system for reduced motion.
 */
export function Icon({ name, size = 16, className, active = false, appear = false }: IconProps) {
  const Component: Animated = ICONS[name];
  const handle = useRef<Handle>(null);
  const root = useRef<HTMLSpanElement>(null);
  const still = useReducedMotion();
  const held = useRef(active);
  const appearOnce = useRef(appear);
  held.current = active;

  useEffect(() => {
    if (still) return;
    if (active || appearOnce.current) handle.current?.startAnimation();
    appearOnce.current = false;
    if (!active) return;
    const current = handle.current;
    return () => current?.stopAnimation();
  }, [active, still]);

  useEffect(() => {
    const element = root.current;
    if (!element || still) return;
    const trigger = element.closest<HTMLElement>(TRIGGER) ?? element;
    const start = () => handle.current?.startAnimation();
    const stop = () => {
      if (!held.current) handle.current?.stopAnimation();
    };
    trigger.addEventListener("pointerenter", start);
    trigger.addEventListener("pointerleave", stop);
    trigger.addEventListener("focus", start);
    trigger.addEventListener("blur", stop);
    return () => {
      trigger.removeEventListener("pointerenter", start);
      trigger.removeEventListener("pointerleave", stop);
      trigger.removeEventListener("focus", start);
      trigger.removeEventListener("blur", stop);
    };
  }, [still, name]);

  return (
    <span
      ref={root}
      aria-hidden
      className={cn("inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
    >
      <Component ref={handle} size={size} className="flex" />
    </span>
  );
}
