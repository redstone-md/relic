-- What the launcher shows behind a modpack: a picture of the admin's (still
-- or animated), or failing that one of the built-in scenes.
ALTER TABLE instances ADD COLUMN banner_url TEXT;
ALTER TABLE instances ADD COLUMN scene TEXT NOT NULL DEFAULT 'mine';
