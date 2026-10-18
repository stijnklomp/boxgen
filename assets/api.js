// Thin wrappers around the local API (`src/serve/dto.rs`).
//
// No geometry and no rules live here: every edit is answered by the Rust
// validator through `/api/plan`, and every build runs through the one build
// pipeline (ADR-0002).

/** One API failure: the status, the message, and the schema-path errors. */
export class ApiError extends Error {
  constructor(status, message, errors) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.errors = errors ?? [];
  }
}

async function request(method, path, body) {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const text = await response.text();
  let value = null;
  if (text.length > 0) {
    try {
      value = JSON.parse(text);
    } catch {
      value = null;
    }
  }

  if (!response.ok) {
    const message =
      value && typeof value.error === "string"
        ? value.error
        : `${method} ${path} failed with status ${response.status}`;
    const errors = value && Array.isArray(value.errors) ? value.errors : [];
    throw new ApiError(response.status, message, errors);
  }

  return value;
}

const get = (path) => request("GET", path);
const post = (path, body) => request("POST", path, body);
const id = (value) => encodeURIComponent(value);

export const api = {
  health: () => get("/api/health"),
  standard: () => get("/api/standard"),

  boxes: () => get("/api/boxes"),
  open: (box) => post(`/api/boxes/${id(box)}/open`),
  save: (box, design) => post(`/api/boxes/${id(box)}/save`, { design }),

  plan: (design) => post("/api/plan", { design }),
  fit: (item) => post("/api/fit", item),

  build: (design) => post("/api/build", { design }),
  buildStatus: () => get("/api/build/status"),

  artifacts: (box) => get(`/api/artifacts/${id(box)}`),
  artifactUrl: (box, name) => `/api/artifacts/${id(box)}/${id(name)}`,

  export: (base) => post("/api/export", { base }),
  publish: (box) => post("/api/publish", { id: box }),
  index: () => post("/api/index"),
};
