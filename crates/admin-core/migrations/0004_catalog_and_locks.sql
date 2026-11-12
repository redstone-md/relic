-- What a mod says about itself, so that players pick optional mods from a
-- catalog instead of a list of file names. `mod_provides` is NULL until the
-- file has been looked at.
ALTER TABLE files ADD COLUMN mod_description TEXT NOT NULL DEFAULT '';
ALTER TABLE files ADD COLUMN icon_sha256 TEXT;
ALTER TABLE files ADD COLUMN mod_depends TEXT;
ALTER TABLE files ADD COLUMN mod_provides TEXT;

-- Folders of the player's game that hold only what the pack lists. Resource
-- packs are locked from the start: see-through textures are the commonest
-- way to cheat without a mod.
ALTER TABLE instances ADD COLUMN locked_dirs TEXT NOT NULL DEFAULT '["resourcepacks"]';
