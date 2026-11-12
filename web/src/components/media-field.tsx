import { useMutation } from "@tanstack/react-query";
import { useRef } from "react";
import { toast } from "sonner";

import { Field } from "@/components/common";
import { Icon } from "@/components/icon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, message } from "@/lib/api";
import { useI18n } from "@/lib/i18n";

/** What the daemon accepts for one upload. */
const MAX_BYTES = 16 * 1024 * 1024;

export const IMAGES = "image/png,image/jpeg,image/gif,image/webp";
export const FONTS = ".ttf,.otf,font/ttf,font/otf";

/**
 * A picture or font the launcher shows: a file uploaded to this server, or
 * the address of one kept elsewhere.
 */
export function MediaField({
  id,
  label,
  hint,
  value,
  onChange,
  accept = IMAGES,
  preview = true,
}: {
  id: string;
  label: string;
  hint?: string;
  value: string;
  onChange: (value: string) => void;
  accept?: string;
  /** Shows the picture under the field. Off for fonts. */
  preview?: boolean;
}) {
  const { t } = useI18n();
  const picker = useRef<HTMLInputElement>(null);

  const upload = useMutation({
    mutationFn: (file: File) => {
      if (file.size > MAX_BYTES) {
        throw new Error(t("The file is larger than 16 MB"));
      }
      return api<{ url: string }>("POST", "/admin/media", { raw: file });
    },
    onSuccess: ({ url }) => onChange(url),
    onError: (error) => toast.error(message(error)),
  });

  return (
    <Field label={label} htmlFor={id} hint={hint}>
      <div className="flex gap-2">
        <Input
          id={id}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder="https://"
          className="min-w-0 flex-1"
        />
        <input
          ref={picker}
          type="file"
          accept={accept}
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) upload.mutate(file);
            event.target.value = "";
          }}
        />
        <Button
          type="button"
          variant="outline"
          disabled={upload.isPending}
          onClick={() => picker.current?.click()}
        >
          <Icon name={upload.isPending ? "spinner" : "upload"} active={upload.isPending} />
          {upload.isPending ? t("Uploading") : t("Upload")}
        </Button>
        {value && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={t("Remove {what}", { what: label.toLowerCase() })}
            onClick={() => onChange("")}
          >
            <Icon name="delete" />
          </Button>
        )}
      </div>
      {preview && value && (
        <img
          src={value}
          alt=""
          referrerPolicy="no-referrer"
          className="max-h-32 max-w-full justify-self-start border object-contain"
        />
      )}
    </Field>
  );
}
