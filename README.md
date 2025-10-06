# Headway

A localhost HTTP tileserver for offline mapping applications.

## Overview

Headway enables existing mapping applications which expect an HTTP interface to function offline by serving offline data via a localhost webserver.
The approach was inspired by [Cardinal Maps](https://github.com/ellenhp/cardinal).

## Architecture

The core logic is implemented as a shared Rust library in the `common` directory.
Platform-specific code exists in separate directories (currently `apple` for iOS).
[UniFFI](https://mozilla.github.io/uniffi-rs/) generates client-specific bindings,
enabling nicer integration with native platform code.

## Usage

```rust
use headway::{HeadwayServer, Bounds};
use std::sync::Arc;

// Create server with storage directory and remote source for extracts
let server = HeadwayServer::new(
    "/path/to/storage",
    "http://example.com/planet.pmtiles"
).await?;

// Start the HTTP server
tokio::spawn(async move {
    server.start("127.0.0.1:9123").await
});

// Download a complete low-resolution tileset
server.download_system_pmtiles_if_necessary(
    "http://example.com/low-res-planet.pmtiles",
    "overview.pmtiles"
).await?;

// Extract a specific region for offline use
let bounds = Arc::new(Bounds::nesw(47.7, -122.2, 47.5, -122.4));
let plan = server.prepare_pmtiles_extract(bounds.clone(), None).await?;
server.extract_pmtiles_region(plan, None).await?;
```

## API Endpoints

- `GET /tileserver/data/default/{z}/{x}/{y}.pbf` - Vector tile data
- `GET /tileserver/styles/basic/style.json` - Map style definition
- `GET /tileserver/data/default.json` - TileJSON metadata
- `GET /status` - Server health check

## Building

For iOS:
```bash
./bin/build-ios.sh
```
