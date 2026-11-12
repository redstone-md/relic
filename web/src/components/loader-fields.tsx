import { Field } from "@/components/common";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { LOADER_NAMES } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { useLoaderVersions, useMinecraftVersions } from "@/lib/queries";
import type { LoaderKind } from "@/lib/types";

const KINDS: LoaderKind[] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];
/** Radix forbids an empty item value, so "pick for me" needs a name. */
const LATEST = "latest";

export interface Platform {
  minecraft: string;
  kind: LoaderKind;
  /** Empty means the newest stable build. */
  loaderVersion: string;
}

/** Minecraft version, loader and loader build: the three choices that define a platform. */
export function LoaderFields({
  value,
  onChange,
}: {
  value: Platform;
  onChange: (next: Platform) => void;
}) {
  const { t } = useI18n();
  const minecraft = useMinecraftVersions();
  const builds = useLoaderVersions(value.kind, value.minecraft);
  const versions = minecraft.data ?? [];
  // A version that left the upstream list still has to show as selected.
  const listed =
    value.minecraft && !versions.includes(value.minecraft)
      ? [value.minecraft, ...versions]
      : versions;
  const known = builds.data ?? [];
  const buildItems =
    value.loaderVersion && !known.some((build) => build.version === value.loaderVersion)
      ? [{ version: value.loaderVersion, stable: true }, ...known]
      : known;

  return (
    <>
      <Field
        label={t("Minecraft version")}
        hint={minecraft.error ? t("Could not load the version list.") : t("1.12.2 and newer.")}
      >
        <Select
          value={value.minecraft || undefined}
          onValueChange={(next) => onChange({ ...value, minecraft: next, loaderVersion: "" })}
        >
          <SelectTrigger className="w-full" aria-label={t("Minecraft version")}>
            <SelectValue
              placeholder={minecraft.isPending ? t("Loading") : t("Choose a version")}
            />
          </SelectTrigger>
          <SelectContent className="max-h-72">
            {listed.map((version) => (
              <SelectItem key={version} value={version}>
                {version}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <Field label={t("Loader")}>
        <Select
          value={value.kind}
          onValueChange={(next) =>
            onChange({ ...value, kind: next as LoaderKind, loaderVersion: "" })
          }
        >
          <SelectTrigger className="w-full" aria-label={t("Loader")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {KINDS.map((kind) => (
              <SelectItem key={kind} value={kind}>
                {LOADER_NAMES[kind]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      {value.kind !== "vanilla" && (
        <Field
          label={t("{loader} build", { loader: LOADER_NAMES[value.kind] })}
          hint={
            builds.error
              ? t("No {loader} builds for this Minecraft version.", {
                  loader: LOADER_NAMES[value.kind],
                })
              : undefined
          }
        >
          <Select
            value={value.loaderVersion || LATEST}
            onValueChange={(next) =>
              onChange({ ...value, loaderVersion: next === LATEST ? "" : next })
            }
            disabled={!value.minecraft}
          >
            <SelectTrigger className="w-full" aria-label={t("Loader build")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="max-h-72">
              <SelectItem value={LATEST}>{t("Newest stable")}</SelectItem>
              {buildItems.map((build) => (
                <SelectItem key={build.version} value={build.version}>
                  {build.version}
                  {!build.stable && <span className="text-muted-foreground"> {t("beta")}</span>}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      )}
    </>
  );
}
