//! What the panel knows about `server.properties`: a readable name and an
//! explanation for the settings people actually change. Settings missing
//! here still show up on the page, under their raw key.

export type PropertyGroup = "gameplay" | "world" | "players" | "network" | "performance";

export type PropertyKind =
  | { type: "bool" }
  | { type: "number"; min: number; max: number }
  | { type: "choice"; options: { value: string; label: string }[] }
  | { type: "text" };

export interface PropertySpec {
  key: string;
  group: PropertyGroup;
  title: string;
  text: string;
  fallback: string;
  kind: PropertyKind;
  /** Decided elsewhere in the panel; shown, not edited. */
  managed?: string;
}

export const GROUP_NAMES: Record<PropertyGroup, string> = {
  gameplay: "Gameplay",
  world: "World",
  players: "Players",
  network: "Network",
  performance: "Performance",
};

const bool: PropertyKind = { type: "bool" };
const text: PropertyKind = { type: "text" };
const number = (min: number, max: number): PropertyKind => ({ type: "number", min, max });
const choice = (...options: [string, string][]): PropertyKind => ({
  type: "choice",
  options: options.map(([value, label]) => ({ value, label })),
});

export const PROPERTIES: PropertySpec[] = [
  {
    key: "gamemode",
    group: "gameplay",
    title: "Game mode",
    text: "The mode new players start in.",
    fallback: "survival",
    kind: choice(
      ["survival", "Survival"],
      ["creative", "Creative"],
      ["adventure", "Adventure"],
      ["spectator", "Spectator"],
    ),
  },
  {
    key: "force-gamemode",
    group: "gameplay",
    title: "Force the game mode",
    text: "Puts every player back into the default mode each time they join.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "difficulty",
    group: "gameplay",
    title: "Difficulty",
    text: "How hard mobs hit and whether hunger can kill.",
    fallback: "easy",
    kind: choice(["peaceful", "Peaceful"], ["easy", "Easy"], ["normal", "Normal"], ["hard", "Hard"]),
  },
  {
    key: "hardcore",
    group: "gameplay",
    title: "Hardcore",
    text: "Players who die become spectators for good. Difficulty is locked to hard.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "pvp",
    group: "gameplay",
    title: "Player versus player",
    text: "Lets players hurt each other.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "allow-flight",
    group: "gameplay",
    title: "Allow flight",
    text: "Stops the server from kicking survival players who stay in the air. Mods with jetpacks or wings need it.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "enable-command-block",
    group: "gameplay",
    title: "Command blocks",
    text: "Lets command blocks run their commands.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "spawn-protection",
    group: "gameplay",
    title: "Spawn protection",
    text: "Blocks around the spawn point that only operators can change. 0 switches it off.",
    fallback: "16",
    kind: number(0, 1024),
  },
  {
    key: "allow-nether",
    group: "gameplay",
    title: "The Nether",
    text: "Lets players travel through Nether portals.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "spawn-monsters",
    group: "gameplay",
    title: "Monsters",
    text: "Spawns hostile mobs at night and in the dark.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "spawn-animals",
    group: "gameplay",
    title: "Animals",
    text: "Spawns cows, sheep and other animals. Older versions only.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "spawn-npcs",
    group: "gameplay",
    title: "Villagers",
    text: "Spawns villagers. Older versions only.",
    fallback: "true",
    kind: bool,
  },

  {
    key: "level-name",
    group: "world",
    title: "World folder",
    text: "The folder the world is kept in. Changing it makes the server open or create another world.",
    fallback: "world",
    kind: text,
  },
  {
    key: "level-seed",
    group: "world",
    title: "Seed",
    text: "Decides how a new world is generated. An existing world ignores it.",
    fallback: "",
    kind: text,
  },
  {
    key: "level-type",
    group: "world",
    title: "World type",
    text: "The generator for a new world. Mods may add their own types.",
    fallback: "minecraft:normal",
    kind: text,
  },
  {
    key: "generate-structures",
    group: "world",
    title: "Structures",
    text: "Generates villages, strongholds and other structures in new chunks.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "max-world-size",
    group: "world",
    title: "World border limit",
    text: "The farthest the world border may reach from the centre, in blocks.",
    fallback: "29999984",
    kind: number(1, 29999984),
  },

  {
    key: "max-players",
    group: "players",
    title: "Player slots",
    text: "How many players may be online at once.",
    fallback: "20",
    kind: number(1, 10000),
  },
  {
    key: "white-list",
    group: "players",
    title: "Whitelist",
    text: "Only players on the whitelist may join. The list is on the Players page.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "enforce-whitelist",
    group: "players",
    title: "Enforce the whitelist",
    text: "Kicks players who are online when they are taken off the whitelist.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "player-idle-timeout",
    group: "players",
    title: "Idle timeout",
    text: "Minutes without input before a player is kicked. 0 never kicks.",
    fallback: "0",
    kind: number(0, 100000),
  },
  {
    key: "op-permission-level",
    group: "players",
    title: "Operator level",
    text: "What operators may do: 1 only bypasses spawn protection, 4 may stop the server.",
    fallback: "4",
    kind: choice(["1", "1"], ["2", "2"], ["3", "3"], ["4", "4"]),
  },
  {
    key: "hide-online-players",
    group: "players",
    title: "Hide the player list",
    text: "Leaves the names of online players out of the server list.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "online-mode",
    group: "players",
    title: "Account check",
    text: "Verifies every player's account. Relic keeps this on and checks accounts itself.",
    fallback: "true",
    kind: bool,
    managed: "Set by Relic at every start",
  },

  {
    key: "server-port",
    group: "network",
    title: "Port",
    text: "The port players connect to.",
    fallback: "25565",
    kind: number(1, 65535),
    managed: "Change it on the Settings page",
  },
  {
    key: "server-ip",
    group: "network",
    title: "Bind address",
    text: "The network address the server listens on. Leave empty to listen on all of them.",
    fallback: "",
    kind: text,
  },
  {
    key: "enable-status",
    group: "network",
    title: "Show in the server list",
    text: "Answers pings from the multiplayer menu. Off, the server looks offline but still accepts players.",
    fallback: "true",
    kind: bool,
  },
  {
    key: "enable-query",
    group: "network",
    title: "Query",
    text: "Answers the GameSpy4 query protocol that some monitoring sites use.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "enable-rcon",
    group: "network",
    title: "Remote console",
    text: "Opens RCON for remote commands. The console on this panel does not need it.",
    fallback: "false",
    kind: bool,
  },
  {
    key: "resource-pack",
    group: "network",
    title: "Resource pack link",
    text: "A direct link to a resource pack the server offers to every player.",
    fallback: "",
    kind: text,
  },
  {
    key: "require-resource-pack",
    group: "network",
    title: "Require the resource pack",
    text: "Disconnects players who decline the pack.",
    fallback: "false",
    kind: bool,
  },

  {
    key: "view-distance",
    group: "performance",
    title: "View distance",
    text: "How many chunks around each player the server sends. The biggest lever on load.",
    fallback: "10",
    kind: number(2, 32),
  },
  {
    key: "simulation-distance",
    group: "performance",
    title: "Simulation distance",
    text: "How many chunks around each player mobs, crops and machines keep running in.",
    fallback: "10",
    kind: number(2, 32),
  },
  {
    key: "max-tick-time",
    group: "performance",
    title: "Watchdog limit",
    text: "Milliseconds one tick may take before the server stops itself as hung. -1 switches the watchdog off.",
    fallback: "60000",
    kind: number(-1, 9007199254740991),
  },
  {
    key: "network-compression-threshold",
    group: "performance",
    title: "Compression threshold",
    text: "Packets at least this many bytes are compressed. -1 never compresses.",
    fallback: "256",
    kind: number(-1, 65535),
  },
  {
    key: "entity-broadcast-range-percentage",
    group: "performance",
    title: "Entity range",
    text: "How far away players still see mobs and items, as a percentage of the normal range.",
    fallback: "100",
    kind: number(10, 1000),
  },
  {
    key: "sync-chunk-writes",
    group: "performance",
    title: "Synchronous chunk writes",
    text: "Waits for the disk on every chunk save. Safer after a power cut, slower on hard drives.",
    fallback: "true",
    kind: bool,
  },
];

/** Shown and edited on its own tile, not among the rest. */
export const MOTD_KEY = "motd";
