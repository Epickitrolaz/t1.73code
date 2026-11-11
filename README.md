# t1.73code

> **Ultra low-memory, high-performance headless T3 Code engine rewritten in Rust.**  
> *Why `t1.73code`? Because 1.73 is approximately $\sqrt{3}$.*

`t1.73code` is an ultra lightweight fork of T3 Code re-engineered from the ground up in Rust. It strips out all unnecessary heavyweight provider harnesses and bundled web UI assets, dedicating itself solely to the [Pi coding agent](https://github.com/earendil-works/pi-coding-agent).

Designed specifically to run comfortably on low-spec systems with **512MB RAM** (budget VPS nodes, LXC containers, Raspberry Pi, and edge machines), `t1.73code` delivers the complete remote agent control experience with a **~10 MB RSS memory footprint** in a single **7 MB static binary**.

---

## Key Features

- **512MB RAM Ready**: Consumes only ~2.4 MB base RSS and ~10 MB under active turn execution, running comfortably within a 512MB RAM budget.
- **Dedicated Pi Agent Driver**: Connects to the fast, lightweight Pi coding agent via asynchronous JSONL RPC (`pi --mode rpc`) with full support for streaming deltas, tool executions, compactions, and turn cancellations.
- **Headless & Remote-First**: Stripped of heavy browser/web UI assets. Control your server remotely via the official T3 Code mobile app ([iOS](https://apps.apple.com/us/app/t3-code-remote-claude-more/id6787819824), [Android](https://play.google.com/store/apps/details?id=com.t3tools.t3code)), the desktop app, or [app.t3.codes](https://app.t3.codes).
- **T3 Connect with GitHub Auth**: Native implementation of the OAuth 2.0 device authorization grant flow. Authenticate seamlessly through GitHub in your browser and expose your environment via secure Cloudflare tunnels (`cloudflared`).
- **Tailscale Integration**: Instant private mesh networking via `tailscale serve --bg` with automatic FQDN detection, CGNAT IPv4 filtering, and one-click pairing URLs.
- **Full Schema Compatibility**: Complete SQLite persistence engine matching T3 Code's `statev2.sqlite` schema for projects, threads, runs, turn items, messages, and auth sessions.

---

## Installation & Setup

### Option 1: Automated Setup Script (Recommended)

Run the automated installer script, which verifies your Rust toolchain, compiles the release binaries, installs them to `~/.local/bin`, and runs the diagnostic health check:

```bash
./scripts/setup.sh
```

Or install directly via `curl`:

```bash
curl -fsSL https://raw.githubusercontent.com/Epickitrolaz/t1.73code/main/scripts/setup.sh | sh
```

### Option 2: Build from Source with Cargo

```bash
# Clone the repository
git clone https://github.com/Epickitrolaz/t1.73code.git
cd t1.73code

# Build optimized release binaries
cargo build --release -p t1_73code

# Install into your PATH
cp target/release/t1_73code ~/.local/bin/t1.73code
cp target/release/t3 ~/.local/bin/t3
```

---

## Authenticating T3 Connect via GitHub

T3 Connect allows remote mobile and web clients to reach your machine securely without opening ports or configuring firewalls. Authentication is handled through your GitHub account via Clerk:

1. **Initiate Sign-in**:
   ```bash
   t1.73code connect login
   # or
   t1.73code connect link
   ```

2. **Approve in Browser**:
   The CLI outputs your device verification link and code:
   ```text
   T3 Connect

   Sign in to T3 Connect. Open on any device:
     https://accounts.t3.codes/device?user_code=XXXX-YYYY
   Code: XXXX-YYYY
   Waiting for approval. Expires in 10 min. Ctrl+C cancels.
   ```

3. **Continue with GitHub**:
   - Open the printed URL in any browser.
   - Click **Continue with GitHub** and authorize the T3 Connect application.
   - The CLI detects the completed authorization, extracts your GitHub identity from the OIDC token, and stores the credentials securely in `~/.t3/userdata/secrets/`:
   ```text
   ✓ Signed in as your-github-username
   ✓ Authorized as your-github-username
   ```

---

## Quickstart & Usage

### 1. Verify System Health (`doctor`)

Run diagnostics to check the Pi agent CLI, Tailscale daemon, cloudflared relay, SQLite storage, and verify memory usage against the 512MB RAM budget:

```bash
t1.73code doctor
```

Output:
```text
============================================================
              t1.73code Doctor Diagnostic Report            
============================================================
  [PASS] Pi CLI                 Available on PATH (v0.84.0)
  [PASS] Tailscale              Daemon active (FQDN: your-node.ts.net, IPv4: 100.x.y.z)
  [PASS] cloudflared Relay      Ready · version 2026.10.4 (Managed)
  [PASS] Storage & Home Dir     Read/write verified at ~/.t3
  [PASS] Memory Budget          Verified under 512MB budget (current RSS: ~10 MB)
============================================================
Result: All essential systems operational.
```

### 2. Start the Server

```bash
# Start with T3 Connect tunnel:
t1.73code up --connect

# Start with Tailscale sharing:
t1.73code up --share

# Start on a custom port and host:
t1.73code up --port 3000 --host 0.0.0.0
```

### 3. Pair with Client Apps

To connect your mobile app, desktop app, or web client:

```bash
t1.73code pair
```

Output:
```text
Issued pairing token: pair_3708d91ac7454356abb47a7b633a13a7
Pairing code: pair_3708d91ac7454356abb47a7b633a13a7
pairingUrl: http://127.0.0.1:3000/pair#token=pair_3708d91ac7454356abb47a7b633a13a7
```

Scan the QR code or paste the URL into your client app to connect immediately.

---

## CLI Command Reference

Both `t1.73code` and `t3` commands can be used interchangeably:

| Command | Description |
|---|---|
| `t1.73code up [flags]` | Starts the headless server (`--connect`, `--share`, `--port`, `--host`, `--home-dir`). |
| `t1.73code serve [flags]` | Alias for `t1.73code up`. |
| `t1.73code pair` | Generates a fresh one-time administrative pairing token and URL. |
| `t1.73code doctor` | Checks Pi CLI, Tailscale, cloudflared, database storage, and validates <512MB memory budget. |
| `t1.73code connect login` | Initiates Clerk device authorization login using your GitHub account. |
| `t1.73code connect link` | Authorizes and enables T3 Connect for the current environment. |
| `t1.73code connect status` | Inspects tunnel state and edge reachability (supports `--json`). |
| `t1.73code connect unlink` | Unlinks the environment from T3 Connect and removes relay-side registration. |
| `t1.73code connect logout` | Signs out of T3 Connect locally and clears stored credentials. |

---

## License

MIT
