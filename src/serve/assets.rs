//! Embedded static assets for the local UI.
//!
//! The frontend is vanilla ES modules and SVG (ADR-0002): no Node, no CDN,
//! no build step. The files live in the repository's top-level `assets/`
//! directory and are embedded with `include_bytes!`, so the released binary
//! carries the UI; this module is the manifest that keeps every served path
//! and its content type in one place.
//!
//! The vendored three.js files (MIT, pinned in `assets/vendor/LICENSE-three.md`)
//! are served from here too: the browser never reaches the network.

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// One embedded file.
struct Asset {
    /// Request path under `/assets/` (`index.html` is served at `/`).
    path: &'static str,
    /// Exact `Content-Type` served with the bytes.
    content_type: &'static str,
    /// Embedded file contents.
    bytes: &'static [u8],
}

/// JavaScript modules and their vendor files.
const JAVASCRIPT: &str = "text/javascript; charset=utf-8";

/// The index page served at `/`.
const INDEX: Asset = Asset {
    path: "index.html",
    content_type: "text/html; charset=utf-8",
    bytes: include_bytes!("../../assets/index.html"),
};

/// The stylesheet.
const STYLES: Asset = Asset {
    path: "styles.css",
    content_type: "text/css; charset=utf-8",
    bytes: include_bytes!("../../assets/styles.css"),
};

/// The app module: store, routing, undo/redo, and the flow.
const APP_JS: Asset = Asset {
    path: "app.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/app.js"),
};

/// The API wrappers.
const API_JS: Asset = Asset {
    path: "api.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/api.js"),
};

/// The SVG division editor.
const EDITOR_JS: Asset = Asset {
    path: "editor.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/editor.js"),
};

/// The design form, standard panel, and fit assistant.
const PANELS_JS: Asset = Asset {
    path: "panels.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/panels.js"),
};

/// The three.js STL viewer.
const VIEWER3D_JS: Asset = Asset {
    path: "viewer3d.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/viewer3d.js"),
};

/// The pinned three.js module (MIT; the import map resolves `three` here).
const THREE_MODULE: Asset = Asset {
    path: "vendor/three.module.min.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/vendor/three.module.min.js"),
};

/// The three.js core module the pinned release splits out.
const THREE_CORE: Asset = Asset {
    path: "vendor/three.core.min.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/vendor/three.core.min.js"),
};

/// The pinned three.js STL loader.
const STL_LOADER: Asset = Asset {
    path: "vendor/STLLoader.js",
    content_type: JAVASCRIPT,
    bytes: include_bytes!("../../assets/vendor/STLLoader.js"),
};

/// The vendored three.js license note (MIT, version, source URLs, hashes).
const THREE_LICENSE: Asset = Asset {
    path: "vendor/LICENSE-three.md",
    content_type: "text/plain; charset=utf-8",
    bytes: include_bytes!("../../assets/vendor/LICENSE-three.md"),
};

/// Every embedded asset, in request-path order.
const ASSETS: &[Asset] = &[
    INDEX,
    STYLES,
    APP_JS,
    API_JS,
    EDITOR_JS,
    PANELS_JS,
    VIEWER3D_JS,
    THREE_MODULE,
    THREE_CORE,
    STL_LOADER,
    THREE_LICENSE,
];

/// `GET /` — the index page.
pub(crate) async fn index() -> Response {
    respond(&INDEX)
}

/// `GET /assets/{*path}` — one embedded file.
pub(crate) async fn asset(Path(path): Path<String>) -> Response {
    match ASSETS.iter().find(|asset| asset.path == path) {
        Some(asset) => respond(asset),
        None => (StatusCode::NOT_FOUND, "no such asset\n").into_response(),
    }
}

fn respond(asset: &Asset) -> Response {
    ([(header::CONTENT_TYPE, asset.content_type)], asset.bytes).into_response()
}
