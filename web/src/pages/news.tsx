import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";

import {
  ActionButton,
  Confirm,
  Empty,
  ErrorNote,
  Field,
  Loading,
  PageHeader,
  Tile,
} from "@/components/common";
import { Icon } from "@/components/icon";
import { MediaField } from "@/components/media-field";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { api, message } from "@/lib/api";
import { dateTime } from "@/lib/format";
import { useI18n } from "@/lib/i18n";
import { keys, useNews } from "@/lib/queries";
import type { NewsDraft, NewsItem } from "@/lib/types";

function Editor({ item, onClose }: { item: NewsItem | null; onClose: () => void }) {
  const { t } = useI18n();
  const client = useQueryClient();
  const [title, setTitle] = useState(item?.title ?? "");
  const [body, setBody] = useState(item?.body ?? "");
  const [image, setImage] = useState(item?.image_url ?? "");

  const save = useMutation({
    mutationFn: () => {
      const draft: NewsDraft = {
        title: title.trim(),
        body: body.trim(),
        image_url: image.trim() || null,
      };
      return item
        ? api<NewsItem>("PUT", `/admin/news/${item.id}`, { json: draft })
        : api<NewsItem>("POST", "/admin/news", { json: draft });
    },
    onSuccess: () => {
      toast.success(item ? t("Post updated") : t("Post published"));
      void client.invalidateQueries({ queryKey: keys.news });
      onClose();
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    save.mutate();
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{item ? t("Edit post") : t("New post")}</DialogTitle>
            <DialogDescription>
              {t("Shown on the home screen of every launcher.")}
            </DialogDescription>
          </DialogHeader>
          <Field label={t("Title")} htmlFor="news-title">
            <Input
              id="news-title"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              maxLength={120}
              required
              autoFocus
            />
          </Field>
          <Field label={t("Text")} htmlFor="news-body">
            <Textarea
              id="news-body"
              value={body}
              onChange={(event) => setBody(event.target.value)}
              rows={8}
              required
            />
          </Field>
          <MediaField
            id="news-image"
            label={t("Picture")}
            hint={t("Optional. Upload one or paste an https address.")}
            value={image}
            onChange={setImage}
          />
          {save.error && <ErrorNote error={save.error} />}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t("Cancel")}
            </Button>
            <ActionButton type="submit" icon="check" busy={save.isPending}>
              {item ? t("Save") : t("Publish")}
            </ActionButton>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function NewsPage() {
  const { t } = useI18n();
  const client = useQueryClient();
  const news = useNews();
  const [editing, setEditing] = useState<NewsItem | "new" | null>(null);
  const [deleting, setDeleting] = useState<NewsItem | null>(null);

  const remove = useMutation({
    mutationFn: (item: NewsItem) => api("DELETE", `/admin/news/${item.id}`),
    onSuccess: () => {
      toast.success(t("Post deleted"));
      void client.invalidateQueries({ queryKey: keys.news });
    },
    onError: (error) => toast.error(message(error)),
  });

  return (
    <>
      <PageHeader
        title={t("News")}
        description={t("Announcements players see when they open the launcher.")}
        actions={
          <Button onClick={() => setEditing("new")}>
            <Icon name="plus" />
            {t("New post")}
          </Button>
        }
      />

      {news.isPending ? (
        <Loading />
      ) : news.error ? (
        <ErrorNote error={news.error} />
      ) : news.data.length === 0 ? (
        <Empty icon="news" title={t("Nothing published yet")}>
          {t("Tell players about an update, an event or planned downtime.")}
        </Empty>
      ) : (
        <div className="grid gap-(--frame) lg:grid-cols-2">
          {news.data.map((item) => (
            <Tile key={item.id} className="gap-3 overflow-hidden">
              {item.image_url && (
                <img
                  src={item.image_url}
                  alt=""
                  loading="lazy"
                  referrerPolicy="no-referrer"
                  className="-mx-5 -mt-5 h-40 w-[calc(100%+2.5rem)] max-w-none object-cover"
                />
              )}
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate font-semibold">{item.title}</h2>
                  <p className="text-xs text-muted-foreground">{dateTime(item.published_at)}</p>
                </div>
                <div className="flex shrink-0 gap-1">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("Edit {title}", { title: item.title })}
                    onClick={() => setEditing(item)}
                  >
                    <Icon name="edit" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("Delete {title}", { title: item.title })}
                    onClick={() => setDeleting(item)}
                  >
                    <Icon name="delete" />
                  </Button>
                </div>
              </div>
              <p className="line-clamp-6 text-sm whitespace-pre-wrap text-muted-foreground">
                {item.body}
              </p>
            </Tile>
          ))}
        </div>
      )}

      {editing && (
        <Editor
          key={editing === "new" ? "new" : editing.id}
          item={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
        />
      )}
      <Confirm
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={t("Delete this post?")}
        description={
          deleting && t("“{title}” disappears from every launcher.", { title: deleting.title })
        }
        action={t("Delete")}
        destructive
        onConfirm={() => deleting && remove.mutate(deleting)}
      />
    </>
  );
}
