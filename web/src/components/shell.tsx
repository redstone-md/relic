import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Link, useLocation } from "wouter";

import { Icon, type IconName } from "@/components/icon";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api } from "@/lib/api";
import { useSampler } from "@/lib/history";
import { useI18n } from "@/lib/i18n";
import { keys } from "@/lib/queries";
import { useTheme } from "@/lib/theme";
import type { User } from "@/lib/types";
import { cn } from "@/lib/utils";

/** The width every page keeps to, bar and content alike. */
const COLUMN = "mx-auto w-full max-w-[80rem] px-4";

const PRESSABLE =
  "outline-none transition-colors hover:bg-secondary focus-visible:bg-secondary focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset";

function NavLink({
  href,
  icon,
  active,
  children,
}: {
  href: string;
  icon: IconName;
  active: boolean;
  children: ReactNode;
}) {
  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      data-icon-trigger
      className={cn(
        PRESSABLE,
        "flex h-14 shrink-0 items-center gap-2.5 px-4 text-sm font-medium text-muted-foreground",
        active && "text-foreground shadow-[inset_0_-2px_0_var(--primary)]",
      )}
    >
      <Icon name={icon} />
      {children}
    </Link>
  );
}

function IconButton({
  label,
  icon,
  onClick,
}: {
  label: string;
  icon: IconName;
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          data-icon-trigger
          onClick={onClick}
          className={cn(
            PRESSABLE,
            "grid h-14 w-11 shrink-0 cursor-pointer place-items-center text-muted-foreground hover:text-foreground",
          )}
        >
          <Icon name={icon} />
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** The languages the panel speaks, each under the name it gives itself. */
function LanguageMenu() {
  const { t, language, setLanguage, locales } = useI18n();
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={t("Language")}
              data-icon-trigger
              className={cn(
                PRESSABLE,
                "grid h-14 w-11 shrink-0 cursor-pointer place-items-center text-muted-foreground hover:text-foreground data-[state=open]:bg-secondary data-[state=open]:text-foreground",
              )}
            >
              <Icon name="earth" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{t("Language")}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end" sideOffset={0} className="min-w-40">
        <DropdownMenuRadioGroup value={language} onValueChange={setLanguage}>
          {locales.map((locale) => (
            <DropdownMenuRadioItem key={locale.code} value={locale.code} lang={locale.code}>
              {locale.name}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function Shell({ user, children }: { user: User; children: ReactNode }) {
  const { t } = useI18n();
  const [location] = useLocation();
  const theme = useTheme();
  const client = useQueryClient();
  useSampler();
  const logout = useMutation({
    mutationFn: () => api("POST", "/auth/logout"),
    onSettled: () => {
      client.clear();
      client.setQueryData(keys.me, null);
      client.setQueryData(keys.setup, { needed: false });
    },
  });
  const dark = theme.resolved === "dark";

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="sticky top-0 z-30 bg-background">
        <div className={cn(COLUMN, "flex items-center")}>
          <Link
            href="/"
            data-icon-trigger
            className="mr-4 flex h-14 shrink-0 items-center gap-3 outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
          >
            <span className="grid size-7 place-items-center bg-primary text-primary-foreground">
              <Icon name="pickaxe" size={16} />
            </span>
            <span className="hidden font-pixel text-xs sm:inline">Relic</span>
          </Link>

          <nav className="flex min-w-0 flex-1 overflow-x-auto" aria-label={t("Main")}>
            <NavLink
              href="/"
              icon="server"
              active={location === "/" || location.startsWith("/instances")}
            >
              {t("Instances")}
            </NavLink>
            <NavLink href="/news" icon="news" active={location.startsWith("/news")}>
              {t("News")}
            </NavLink>
            <NavLink href="/players" icon="users" active={location.startsWith("/players")}>
              {t("Players")}
            </NavLink>
            <NavLink href="/settings" icon="settings" active={location.startsWith("/settings")}>
              {t("Settings")}
            </NavLink>
          </nav>

          <span className="hidden max-w-40 truncate px-3 text-sm text-muted-foreground md:inline">
            {user.username}
          </span>
          <LanguageMenu />
          <IconButton
            label={dark ? t("Light theme") : t("Dark theme")}
            icon={dark ? "sun" : "moon"}
            onClick={() => theme.set(dark ? "light" : "dark")}
          />
          <IconButton label={t("Sign out")} icon="logout" onClick={() => logout.mutate()} />
        </div>
      </header>

      <main className={cn(COLUMN, "flex flex-1 flex-col gap-(--frame) pt-6 pb-10")}>{children}</main>
    </div>
  );
}
