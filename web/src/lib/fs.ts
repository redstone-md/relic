//! Reading and writing single files of an instance's folder, and the two
//! formats the panel edits by hand: `server.properties` and the trash names.

import { useQuery } from "@tanstack/react-query";

import { api, downloadUrl } from "@/lib/api";
import { keys } from "@/lib/queries";

function base(id: string): string {
  return `/admin/instances/${id}/fs`;
}

export function contentUrl(id: string, path: string): string {
  return `${downloadUrl(`${base(id)}/content`)}?path=${encodeURIComponent(path)}`;
}

/** A folder of the instance as one zip. */
export function archiveUrl(id: string, path: string): string {
  return `${downloadUrl(`${base(id)}/archive`)}?path=${encodeURIComponent(path)}`;
}

/** The file as text, or `null` when the server has not written it yet. */
export async function readText(id: string, path: string): Promise<string | null> {
  const response = await fetch(contentUrl(id, path), { credentials: "same-origin" });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`The server answered ${response.status}`);
  return response.text();
}

export function writeText(id: string, path: string, text: string): Promise<void> {
  return api("PUT", `${base(id)}/content`, { raw: new Blob([text]), query: { path } });
}

export function useText(id: string, path: string) {
  return useQuery({
    queryKey: keys.fsText(id, path),
    queryFn: () => readText(id, path),
  });
}

/** A JSON list the server keeps, such as `ops.json`. Anything else reads as empty. */
export function parseList<T>(text: string | null | undefined): T[] {
  if (!text) return [];
  try {
    const value: unknown = JSON.parse(text);
    return Array.isArray(value) ? (value as T[]) : [];
  } catch {
    return [];
  }
}

// --- server.properties ---------------------------------------------------

function unescape(value: string): string {
  return value.replace(/\\(u[0-9a-fA-F]{4}|.)/g, (_, code: string) => {
    if (code.length === 5) return String.fromCharCode(parseInt(code.slice(1), 16));
    if (code === "n") return "\n";
    if (code === "t") return "\t";
    if (code === "r") return "\r";
    return code;
  });
}

function escape(value: string): string {
  let out = "";
  for (const [index, char] of [...value].entries()) {
    const code = char.codePointAt(0)!;
    if (char === "\\") out += "\\\\";
    else if (char === "\n") out += "\\n";
    else if (char === "\r") out += "\\r";
    else if (char === "\t") out += "\\t";
    else if (char === " " && index === 0) out += "\\ ";
    else if (code < 0x20 || code > 0x7e) {
      // Every Minecraft version reads these; older ones read nothing else.
      for (let unit = 0; unit < char.length; unit += 1) {
        out += `\\u${char.charCodeAt(unit).toString(16).toUpperCase().padStart(4, "0")}`;
      }
    } else out += char;
  }
  return out;
}

const PAIR = /^\s*([^#!\s][^=:\s]*)\s*[=:]?\s*(.*)$/;

/** The settings of a `server.properties`, in the order the file has them. */
export function parseProperties(text: string | null | undefined): Map<string, string> {
  const found = new Map<string, string>();
  for (const line of (text ?? "").split(/\r?\n/)) {
    const pair = PAIR.exec(line);
    if (pair) found.set(unescape(pair[1]!), unescape(pair[2]!));
  }
  return found;
}

/**
 * The file with some settings changed. Lines the panel does not touch stay
 * exactly as the server wrote them; new settings go to the end.
 */
export function patchProperties(text: string | null | undefined, changes: Map<string, string>): string {
  const left = new Map(changes);
  const lines = (text ?? "#Minecraft server properties").split(/\r?\n/).map((line) => {
    const pair = PAIR.exec(line);
    if (!pair) return line;
    const key = unescape(pair[1]!);
    const value = left.get(key);
    if (value === undefined) return line;
    left.delete(key);
    return `${pair[1]}=${escape(value)}`;
  });
  while (lines.length > 0 && lines.at(-1) === "") lines.pop();
  for (const [key, value] of left) lines.push(`${key}=${escape(value)}`);
  return `${lines.join("\n")}\n`;
}

// --- Trash ----------------------------------------------------------------

/** Where deleted files wait. Hidden from the file list. */
export const TRASH = ".trash";

/** A name in the trash that remembers when and from where the file came. */
export function trashName(path: string): string {
  return `${Date.now().toString(36)}_${encodeURIComponent(path)}`;
}

export function trashOrigin(name: string): { path: string; at: number } | null {
  const cut = name.indexOf("_");
  if (cut < 1) return null;
  const at = parseInt(name.slice(0, cut), 36);
  try {
    const path = decodeURIComponent(name.slice(cut + 1));
    return Number.isFinite(at) && path ? { path, at: Math.floor(at / 1000) } : null;
  } catch {
    return null;
  }
}

/** Moves a file or folder into the trash, from where it can be put back. */
export function toTrash(id: string, path: string): Promise<void> {
  return api("POST", `${base(id)}/rename`, {
    json: { from: path, to: `${TRASH}/${trashName(path)}` },
  });
}
