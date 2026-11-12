const BASE = "/api/v1";

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

/** Called when the daemon stops recognising the session. */
let onUnauthorized: () => void = () => {};

export function setUnauthorizedHandler(handler: () => void) {
  onUnauthorized = handler;
}

async function fail(response: Response): Promise<never> {
  let code = "internal";
  let message = `The server answered ${response.status}`;
  try {
    const body = await response.json();
    if (typeof body?.code === "string") code = body.code;
    if (typeof body?.message === "string") message = body.message;
  } catch {
    // Not JSON: a proxy error page, most likely. Keep the generic text.
  }
  if (response.status === 401) onUnauthorized();
  throw new ApiError(response.status, code, message);
}

interface Options {
  /** Sent as JSON. */
  json?: unknown;
  /** Sent as is: a file. */
  raw?: Blob;
  query?: Record<string, string | number | boolean | undefined | null>;
  signal?: AbortSignal;
}

function url(path: string, query?: Options["query"]): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value !== undefined && value !== null) search.set(key, String(value));
  }
  const tail = search.size ? `?${search}` : "";
  return `${BASE}${path}${tail}`;
}

export async function api<T = void>(
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
  path: string,
  options: Options = {},
): Promise<T> {
  const headers: Record<string, string> = {};
  let body: BodyInit | undefined;
  if (options.json !== undefined) {
    headers["content-type"] = "application/json";
    body = JSON.stringify(options.json);
  } else if (options.raw) {
    headers["content-type"] = "application/octet-stream";
    body = options.raw;
  }
  const response = await fetch(url(path, options.query), {
    method,
    headers,
    body,
    credentials: "same-origin",
    signal: options.signal,
  });
  if (!response.ok) return fail(response);
  if (response.status === 204 || response.status === 202) return undefined as T;
  const text = await response.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

/** Path of a modpack file inside an instance, safe to put into a URL. */
export function filePath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

/** Uploads a file with progress, which `fetch` cannot report. */
export function upload<T>(
  path: string,
  file: Blob,
  query: Options["query"],
  onProgress: (sent: number) => void,
  method: "PUT" | "POST" = "PUT",
): Promise<T> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open(method, url(path, query));
    request.setRequestHeader("content-type", "application/octet-stream");
    request.upload.onprogress = (event) => onProgress(event.loaded);
    request.onerror = () => reject(new ApiError(0, "network", "The connection was lost"));
    request.onload = () => {
      let body: { code?: string; message?: string } | undefined;
      try {
        body = JSON.parse(request.responseText);
      } catch {
        body = undefined;
      }
      if (request.status >= 200 && request.status < 300) {
        resolve(body as T);
        return;
      }
      if (request.status === 401) onUnauthorized();
      reject(
        new ApiError(
          request.status,
          body?.code ?? "internal",
          body?.message ?? `The server answered ${request.status}`,
        ),
      );
    };
    request.send(file);
  });
}

export function socketUrl(path: string): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}${BASE}${path}`;
}

export function downloadUrl(path: string): string {
  return `${BASE}${path}`;
}

export function message(error: unknown): string {
  // The daemon words its errors as lowercase fragments; show them as sentences.
  const text = error instanceof Error ? error.message : String(error);
  return text.charAt(0).toUpperCase() + text.slice(1);
}
