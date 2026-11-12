-- News shown in the launcher. Bodies are Markdown.
CREATE TABLE news (
    id           INTEGER PRIMARY KEY,
    title        TEXT NOT NULL,
    body         TEXT NOT NULL,
    image_url    TEXT,
    published_at INTEGER NOT NULL
) STRICT;
