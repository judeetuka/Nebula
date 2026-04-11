# NEBULA

Distributed Android device cluster management platform. Orchestrate fleets of Android phones for SMS automation, USSD processing, payment workflows, and more — securely, at scale.

## Architecture

```
                     ┌──────────────────────┐
                     │   NEBULA SERVER       │
                     │   (Rust)              │
                     │                       │
                     │  REST API + WebSocket │
                     │  MQTT Relay Broker    │
                     │  SeaORM (SQLite/PG)   │
                     │  JWT Auth + TLS       │
                     └──────┬───────────────┘
                            │
              ┌─────────────┼─────────────┐
              │             │             │
        ┌─────┴─────┐ ┌────┴────┐ ┌──────┴──────┐
        │ CLUSTER A │ │CLUSTER B│ │ ADMIN APP   │
        │           │ │         │ │ (Flutter)   │
        │  Master   │ │ Master  │ │             │
        │  ╱  ╲     │ │  ╱  ╲  │ │ Dashboards  │
        │ W1   W2   │ │ W3  W4 │ │ Workflows   │
        └───────────┘ └────────┘ └─────────────┘
```

**Hub-and-Spoke topology**: Workers connect to their cluster master's MQTT broker. Cross-network nodes relay through the central server. Server-mediated failover with automatic master election.

## Components

| Component | Language | Description |
|-----------|----------|-------------|
| `nebula-core` | Rust | Shared identity, crypto (AES-256-GCM, HMAC-SHA256, HKDF), wire protocol |
| `nebula-engine` | Rust | Node runtime: MQTT, storage (SQLCipher), plugins, failover, thermal management |
| `nebula-server` | Rust | Central server: REST API, JWT auth, WebSocket events, MQTT relay, SeaORM |
| `nebula-plugin-sdk` | Rust | Plugin development kit with proot, SRT streaming, and WhatsApp convenience APIs |
| `nebula-node` | Flutter + Rust | Android node app (worker/master) |
| `nebula-admin` | Flutter | Cross-platform admin dashboard |
| `manny_ui` | Flutter | Shared UI library: frosted glass, neumorphic, Rust FFT audio visualizer |

## Plugins

### Built-in (6)
| Plugin | Purpose |
|--------|---------|
| `device-info` | Battery, CPU, memory, network metrics |
| `observer` | SMS/notification content monitoring |
| `comm-link` | USSD dial, SMS send |
| `accessibility` | Screen tap, type text, navigate |
| `action-mirror` | Puppet mode action mirroring |
| `file-access` | File read/write/list on device |

### First-party (8)
| Plugin | Purpose |
|--------|---------|
| `browser` | Headless web automation |
| `classifier` | ML text classification (M-Pesa, Airtel Money, etc.) |
| `contacts` | Device contact access |
| `email` | SMTP email sending |
| `payment-processor` | End-to-end payment pipeline |
| `linux-bridge` | proot Linux distros (Alpine, Ubuntu 24.04, Debian 13, Arch) |
| `whatsapp` | WhatsApp Web multi-device client (Signal Protocol E2EE) |
| `screen-stream` | SRT-based screen capture relay |

## Key Features

- **Encrypted everything**: SQLCipher (AES-256) for storage, TLS for MQTT, SecurityEnvelope for payloads, Android Keystore for key material
- **Plugin sandboxing**: Ed25519 signature verification before loading `.so` binaries
- **Thermal protection**: 6-state thermal manager with graduated response (reduce heartbeat → pause plugins → voluntary demotion → protective shutdown)
- **Battery awareness**: Graduated workload reduction at 25% battery, graceful shutdown at 15%
- **Automatic failover**: Server-mediated master election with health-gated succession scoring
- **SRT streaming**: Low-latency screen streaming over unreliable networks
- **Visual workflow editor**: Drag-and-drop plugin action pipelines (vyuh_node_flow)
- **42 production safety issues resolved**: Deadlock prevention, MQTT reconnection, crash recovery, manufacturer battery saver workarounds

## Quick Start

### Server

```bash
# Development (SQLite)
cargo run -p nebula-server -- --config config/server.development.toml

# Production (Docker + PostgreSQL)
docker compose up -d
```

### Build Plugins (Android)

```bash
./scripts/build-plugins.sh release
```

### Admin Dashboard

```bash
cd nebula-admin
flutter pub get
flutter run
```

### Node App (Android)

```bash
cd nebula-node
flutter pub get
flutter run
```

## Project Structure

```
Nebula/
├── crates/
│   ├── nebula-core/          # Shared crypto, identity, protocol
│   ├── nebula-engine/        # Node runtime engine
│   ├── nebula-plugin-sdk/    # Plugin SDK (proot, SRT, WhatsApp APIs)
│   └── nebula-server/        # Central server
├── plugins/
│   ├── built-in/             # 6 core plugins
│   └── first-party/          # 8 extended plugins
├── nebula-node/              # Flutter Android node app
├── nebula-admin/             # Flutter admin dashboard
├── shared/manny_ui/          # Shared UI library
├── config/                   # Server TOML configs
├── scripts/                  # Build scripts
├── Dockerfile                # Multi-stage server image
└── docker-compose.yml        # Server + PostgreSQL
```

## Testing

```bash
# All Rust tests (750 tests)
cargo test --workspace

# Flutter admin
cd nebula-admin && flutter test

# Android cross-compilation check
cargo check -p nebula-engine --target aarch64-linux-android
```

## Deployment

| Target | Method |
|--------|--------|
| Server | `docker compose up -d` or bare-metal binary |
| Node app | Flutter APK → sideload or Play Store |
| Admin app | Flutter desktop/mobile/web build |
| Plugins | `./scripts/build-plugins.sh` → `.so` files pushed to nodes |

Environment variables for production:
- `JWT_SECRET` — JWT signing key (required)
- `NEBULA_DATABASE_URL` — `postgres://...` or `sqlite://...`
- `NEBULA_PLUGIN_VERIFY_KEY` — Ed25519 public key (64 hex chars) for plugin verification
- `MQTT_PORT` — MQTT relay broker port (default: 1884)

## CI/CD

GitHub Actions pipeline triggers on:
- Push to `release` or `release/*` branches
- Version tags (`v*`)

Jobs: check → test → build-release (artifact uploaded for 30 days)

## Security

- All MQTT traffic encrypted (TLS + payload AES-256-GCM)
- SQLCipher full-database encryption with Android Keystore key derivation
- Ed25519 plugin signature verification
- JWT authentication with argon2 password hashing
- R8/ProGuard obfuscation for release APK
- TLS certificate pinning with SHA-256 fingerprints
- Auth tokens stored in encrypted blob store (not plaintext)

## License

Proprietary. Copyright HexiCore.
