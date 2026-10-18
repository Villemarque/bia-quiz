# Deploying `bia-quiz` on Debian

A complete runbook starting from a fresh **Debian 12 (bookworm) or Debian 13 (trixie)**
VM. Host users, UFW, SSH, Caddy, unattended upgrades, fail2ban and the systemd service are
managed by [`ansible/`](ansible/). The release binaries and config are staged with
`make artefacts`; the database import and backups are explicit.

## Quick start

1. Provision **Debian 13 (trixie)** or **Debian 12 (bookworm)** and point DNS at it
   (§1–§2).
2. On the controller, cross-build and stage the release (§3):
   ```sh
   make artefacts
   cp deploy/artefacts/bia.toml.example deploy/artefacts/bia.toml
   $EDITOR deploy/artefacts/bia.toml
   ```
3. Create the `deploy` admin, the `bia-quiz` service account and configure UFW, SSH,
   Caddy, unattended upgrades and fail2ban with [Ansible](ansible/) (§2.5). Skip the
   `app` tag on this first host-prep run.
4. Run the `app` role to copy the artefacts, install the unit and start the service (§6).
5. Import the question bank from `/admin` (§5).
6. Run the checklist (§8) and set up backups (§9).

The manual file-install steps in §4 are only needed if you do **not** use the `app` role.

The sections below are the full reference, including upgrades, security and
troubleshooting.

## 0. What you are deploying

| Piece | Value |
|---|---|
| Binary | `bia-quiz` — SPA + Diesel migrations embedded, SQLite bundled, ~5–6 MB |
| Importer | `bia-import` — fetches/imports the bank, ~3–4 MB |
| Database | one SQLite file, ~8–10 MB (questions, figures, results) |
| TLS | Caddy (automatic Let's Encrypt) |
| Service | systemd, non-root `bia-quiz`, bound to `127.0.0.1:8080` |
| Load | one room, ~35 phones, one teacher; 1 vCPU / 1 GB RAM is enough |

The target is **Linux x86_64**; `make artefacts` cross-builds it against glibc 2.36
(compatible with Debian 12/13). SQLite is compiled in (bundled). See §3.

## 1. Provision the server

Recommended: **Debian 13 (trixie)** or **Debian 12 (bookworm)**, 1 vCPU, 1 GB RAM,
10 GB disk, public IPv4 (IPv6 optional). Any provider works (Hetzner, Scaleway, OVH,
DigitalOcean, Fly…).

Before creating it, decide the hostname, e.g. `quiz.asso-triple-a.org`.

```sh
# as root / first user
apt update && apt upgrade -y
apt install -y curl ca-certificates gnupg sqlite3
timedatectl set-ntp true          # scoring depends on the server clock
```

The `deploy` admin account, its SSH key and sudoers entry, the `bia-quiz` service
account, UFW, Caddy, unattended upgrades and fail2ban are set up by the Ansible playbook
in §2.5.
Keep a root/console session open until you have verified SSH as `deploy`. The daemon port is managed by
Ansible in §2.5, and the playbook disables password authentication; root-login
hardening is still manual.

## 2. DNS

Create the DNS record **before** starting Caddy:

```
A     quiz.asso-triple-a.org   →  <server IPv4>
AAAA  quiz.asso-triple-a.org   →  <server IPv6>   (optional)
```

The host firewall is configured by Ansible in §2.5: SSH, 80 and 443 allowed, with a
default-deny incoming policy. Open the same ports in the provider's security group.
Caddy is installed by the same run and needs DNS to be live before it can obtain the
certificate.

Verify: `dig +short quiz.asso-triple-a.org` returns the server IP. If you use Cloudflare, set
the record to **DNS only** until the certificate is issued (or use an origin certificate).

## 2.5 Host preparation (Ansible): users, firewall, SSH, Caddy, upgrades, fail2ban, systemd

The Ansible roles in [`ansible/`](ansible/) manage **Linux users, UFW, SSH, Caddy,
unattended upgrades, fail2ban and the `bia-quiz` systemd service**. The `app` role
copies the binaries from `deploy/artefacts/`, installs the staged config if present, and
starts the service. It does not import the database. Configured as below, the roles
create the `deploy` admin and the `bia-quiz` service account, install the deploy user's
SSH key and passwordless sudo, configure UFW for SSH/80/443 with a default-deny incoming
policy, install/configure Caddy for TLS, enable Debian security updates, install the
fail2ban SSH jail, and install/start the app service (the `app` role is skipped on the
first host-prep run).

Install Ansible on the controller, not on the server:

```sh
# one option (or your package manager / an existing Ansible install):
pipx install ansible-core
```

Then, from the repository root:

```sh
cd deploy/ansible
ansible-galaxy collection install -r requirements.yml
cp inventory.example.yml inventory.yml
mkdir -p group_vars
cp group_vars/all.yml.example group_vars/all.yml
$EDITOR inventory.yml group_vars/all.yml

ansible-playbook site.yml --check --diff --skip-tags app
ansible-playbook site.yml --skip-tags app
```

The `app` role is skipped here because it requires the artefacts/config from §3. After
staging them and creating the real `deploy/artefacts/bia.toml`, run:

```sh
ansible-playbook site.yml --tags app
```

`inventory.yml` connects as **root** (or the provider's initial sudo user) on a blank
server:

```yaml
all:
  children:
    bia_quiz:
      hosts:
        quiz:
          ansible_host: <server IPv4>
          ansible_user: root
```

Create the two key pairs on the controller (or point the paths at existing keys):

```sh
ssh-keygen -t ed25519 -f ~/.ssh/bia-quiz
ssh-keygen -t ed25519 -f ~/.ssh/bia-quiz-deploy
```

If the bootstrap account uses password auth, install `sshpass` on the controller and
run the playbook with `-k -K`, or set `ansible_password` / `ansible_become_password` in
the gitignored `inventory.yml`. Ansible does not prompt automatically without those flags.
The first run disables password authentication; verify key login as `deploy` before
closing the current session.

The example `bia_users` already creates `bia-quiz` and `deploy`, reading their public
keys from the controller at `~/.ssh/bia-quiz.pub` and
`~/.ssh/bia-quiz-deploy.pub`; create those keys or change the paths. No key material is
pasted into group vars. `deploy` has `sudo: true` for passwordless sudo. Set `bia_domain`
to the public hostname and `bia_app_port` to `8080` (it must match `[server].bind` in
`bia.toml`). If SSH is not on 22, set
`bia_ssh_port`; the default allowed list follows it. The `sshd` role comments the active
`Port` and password-auth lines, writes `/etc/ssh/sshd_config.d/00-bia-quiz.conf`,
disables password authentication, validates the effective config and restarts SSH.
`bia_ssh_previous_port` (default `22`) stays allowed in UFW during the change; remove it
after verifying the new port. Set `ansible_port` in
`inventory.yml` to the same value for subsequent runs. The firewall preflight refuses to
enable UFW if `bia_ssh_port` is missing from the allowed list. Set `bia_enable_ufw: false` only if the
provider firewall already handles this, and `bia_enable_caddy: false` only if Caddy is
managed elsewhere. Unattended upgrades are enabled by default and apply Debian stable +
security updates; automatic reboot is on at `03:30` when required. Set
`bia_unattended_automatic_reboot: false` to reboot manually instead. The fail2ban
SSH jail is enabled by default; add trusted CIDRs to `bia_fail2ban_ignoreip`, or set
`bia_enable_fail2ban: false` to skip it. See [`ansible/README.md`](ansible/README.md)
for every field.

Verify before closing the root session:

```sh
ssh -p <bia_ssh_port> deploy@<server> 'sudo -n true && sudo ufw status && systemctl is-active caddy'
ssh -p <bia_ssh_port> deploy@<server> "systemctl list-timers 'apt-daily*' --no-pager"
ssh -p <bia_ssh_port> deploy@<server> 'sudo fail2ban-client status sshd'
```

Then switch `ansible_user` to `deploy` and set `ansible_port` to `bia_ssh_port` for later
runs. Ansible already disables password authentication; root-login hardening is manual,
only after key login works:

```sh
sudoedit /etc/ssh/sshd_config   # PermitRootLogin no (password auth is already disabled)
sudo systemctl restart ssh
```

Keep a root/console session open until that is verified.

The `app` role creates `/var/lib/bia-quiz/backups`; the manual fallback in §4.1 also
creates it.

## 3. Build the binaries

The target server is Linux x86_64 (`uname -m`). `make artefacts` builds that target; on
the controller verify with `file deploy/artefacts/bia-quiz`.

### Build and upload

Build **Linux x86_64** on the controller regardless of its host OS/CPU:

```sh
make artefacts
file deploy/artefacts/bia-quiz
```

`make artefacts` cross-compiles with `cargo-zigbuild` and Zig, then stages
`bia-quiz`, `bia-import` and `bia.toml.example` in the gitignored `deploy/artefacts/`
directory. The systemd unit stays in `deploy/bia-quiz.service` and is copied from
there. The glibc floor is `2.36` by default (works on Debian 12 and 13).

Put the real config **directly** in `deploy/artefacts/bia.toml` (the whole directory is
gitignored). `make artefacts` preserves it; copy the staged example and edit it:

```sh
cp deploy/artefacts/bia.toml.example deploy/artefacts/bia.toml
$EDITOR deploy/artefacts/bia.toml
```

The Ansible `app` role installs that file as `/etc/bia-quiz/bia.toml`.
Prerequisites on the controller:

```sh
rustup target add x86_64-unknown-linux-gnu
cargo install cargo-zigbuild
# Zig: brew install zig, or your package manager
```

Override the glibc floor if needed with `make artefacts CROSS_GLIBC=2.41`. If the
controller is already x86_64 Linux, plain `make build` also works; a macOS/aarch64
`make build` produces a binary that will not run on this server.

For the Ansible path, the `app` role copies the binaries and optional config from
`deploy/artefacts/` and reads the unit from `deploy/bia-quiz.service`; there is no
separate SCP step. For the manual fallback, create a bundle from the same directory:

```sh
rm -rf /tmp/bia-quiz-deploy
cp -a deploy/artefacts /tmp/bia-quiz-deploy
cp deploy/bia-quiz.service /tmp/bia-quiz-deploy/
tar -czf bia-quiz-deploy.tgz -C /tmp/bia-quiz-deploy .
scp -P <bia_ssh_port> bia-quiz-deploy.tgz deploy@quiz.asso-triple-a.org:/tmp/
```

On the server:

```sh
mkdir -p /tmp/bia-quiz-deploy
tar -xzf /tmp/bia-quiz-deploy.tgz -C /tmp/bia-quiz-deploy
```

> The cross build targets glibc **2.36** by default. If you use native `make build`, build
> on the oldest supported distro or set `CROSS_GLIBC` to a floor compatible with the server.

## 4. Install the application files (manual fallback)

Skip this section if you use the Ansible `app` role: it creates the directories, copies
the binaries, installs the config when staged, and starts the unit.

### 4.1 State and config directories

The `bia-quiz` account and `/var/lib/bia-quiz` were created by Ansible (§2.5). Create the
backup directory and the root-owned file directories:

```sh
sudo install -d -m 0750 -o bia-quiz -g bia-quiz /var/lib/bia-quiz/backups
sudo install -d /opt/bia-quiz /etc/bia-quiz
id bia-quiz
ls -ld /var/lib/bia-quiz /var/lib/bia-quiz/backups
```

The systemd unit already uses `User=bia-quiz`, `Group=bia-quiz` and
`StateDirectory=bia-quiz`; if you rename the account, update those lines.

### 4.2 Install binaries and config

```sh
sudo install -m 0755 /tmp/bia-quiz-deploy/bia-quiz   /opt/bia-quiz/bia-quiz
sudo install -m 0755 /tmp/bia-quiz-deploy/bia-import /opt/bia-quiz/bia-import

# Prefer the real config staged in the bundle; fall back to the example.
if [ -f /tmp/bia-quiz-deploy/bia.toml ]; then
  sudo install -m 0640 -o root -g bia-quiz /tmp/bia-quiz-deploy/bia.toml \
    /etc/bia-quiz/bia.toml
else
  sudo install -m 0640 -o root -g bia-quiz /tmp/bia-quiz-deploy/bia.toml.example \
    /etc/bia-quiz/bia.toml
  sudo editor /etc/bia-quiz/bia.toml
fi
```

Ownership at this point:

| Path | Owner | Mode |
|---|---|---|
| `/opt/bia-quiz/bia-quiz`, `bia-import` | `root:root` | `0755` |
| `/etc/bia-quiz/bia.toml` | `root:bia-quiz` | `0640` |
| `/var/lib/bia-quiz` (+ `backups/`) | `bia-quiz:bia-quiz` | `0750` |
| Database | created by `bia-quiz` | inside the state dir |

Check that the service can actually read its config and write its state:

```sh
sudo -u bia-quiz test -r /etc/bia-quiz/bia.toml && echo "config readable"
sudo -u bia-quiz test -w /var/lib/bia-quiz && echo "state writable"
```

`/etc/bia-quiz/bia.toml` — the required values:

```toml
[server]
bind = "127.0.0.1:8080"
public_base_url = "https://quiz.asso-triple-a.org"   # QR codes + secure cookies
source_url = "https://github.com/Villemarque/bia-quiz"  # AGPL network-use obligation

[db]
path = "/var/lib/bia-quiz/bia.sqlite3"

[auth]
session_hours = 12

[prof]
user = "prof"
password = "<prof password>"                   # hashed at startup

[admin]
user = "admin"
password = "<admin password>"                  # can access /prof and /admin

[import]
upstream_url = "https://github.com/Villemarque/bia-annales-database-csv/archive/refs/heads/master.tar.gz"
```

Keep the file `0640 root:bia-quiz`: it contains both plaintext passwords.

## 5. Import the question bank

Once the service is running, open `/admin`, log in with the **admin** account, and import
from the configured upstream URL only. It downloads into a temporary directory, refuses
while a room is live, and deactivates questions missing from the TSV instead of deleting
them. Chapter titles are embedded in the binary; no local TSV, figures or chapters file is
kept.

The CLI is the bootstrap/CI fallback. It creates the DB, runs the migrations, downloads
the upstream TSV + figures (if `--fetch`), and stores the figures in SQLite. Use a
temporary working directory:

```sh
work=/var/lib/bia-quiz/import
sudo -u bia-quiz mkdir -p "$work"
sudo -u bia-quiz /opt/bia-quiz/bia-import \
  --fetch \
  --tsv "$work/annales-bia.tsv" \
  --db /var/lib/bia-quiz/bia.sqlite3 \
  --images "$work/figures" \
  --report "$work/import-report.txt"
sudo rm -rf "$work"
```

Expected:

```
records read : 1464
imported     : 1464
figures      : 90
skipped      : 0
```

(488 questions have no chapter; the teacher UI groups them by subject under `Sans chapitre`.)

No outbound HTTPS? Copy `annales-bia.tsv` and `data/figures/` into the temporary work
directory and drop `--fetch`; the baked-in chapter titles are still used.

Verify:

```sh
sudo -u bia-quiz sqlite3 /var/lib/bia-quiz/bia.sqlite3 \
  "SELECT COUNT(*) FROM questions; SELECT COUNT(*) FROM figures;"
# 1464
# 90
```

The TSV and figures are only needed for future refreshes; the database is self-contained.

## 6. systemd

Recommended: let the `app` role copy the binaries and staged config, install the unit
from `deploy/bia-quiz.service`, and start the service:

```sh
cd deploy/ansible
ansible-playbook site.yml --tags app
```

The role requires `deploy/artefacts/bia-quiz` and `/etc/bia-quiz/bia.toml` (staged or
already installed). Manual fallback:

```sh
sudo install -m 0644 /tmp/bia-quiz-deploy/bia-quiz.service \
  /etc/systemd/system/bia-quiz.service
sudo systemctl daemon-reload
sudo systemctl enable --now bia-quiz
sudo systemctl status bia-quiz
```

Verify either path:

```sh
systemctl show -p User -p Group -p StateDirectory bia-quiz
journalctl -u bia-quiz -n 50 --no-pager
```

Expected log: `loaded 90 figures from SQLite` and `listening on http://127.0.0.1:8080`.

## 7. Caddy and TLS

Caddy was installed and configured by the Ansible `caddy` role in §2.5. It serves
`https://<bia_domain>` and proxies to `127.0.0.1:<bia_app_port>`:

```
<bia_domain> {
    encode gzip
    reverse_proxy 127.0.0.1:<bia_app_port>
}
```

Caddy obtains the certificate automatically (ports 80/443 open, DNS correct) and proxies
the WebSocket upgrade. Check:

```sh
journalctl -u caddy -n 30 --no-pager
curl -I https://<bia_domain>/
```

## 8. Post-deploy checklist

```sh
curl -fsS https://quiz.asso-triple-a.org/healthz                 # ok
curl -sI https://quiz.asso-triple-a.org/ | head -1                # 200
curl -s -o /tmp/f.jpeg -w '%{http_code} %{content_type}\n' \
  https://quiz.asso-triple-a.org/figures/2015-000.jpeg            # 200 image/jpeg
curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' https://quiz.asso-triple-a.org/prof
# 303 to /login without a session; 200 in a logged-in browser
curl -s https://quiz.asso-triple-a.org/api/chapters | head -c 120 # subject + chapter catalog
```

In a browser:

1. `/teacher` → create a session (e.g. 5 Histoire questions), projector shows the code +
   QR. Choose a per-chapter count for each selected chapter.
2. Phone on **cellular** (not the LAN) → scan the QR, join, answer.
3. `/login` → prof account → `/prof` shows the session and the CSV export.
4. `/login` → admin account → `/admin` shows stats and can import.
5. Confirm the QR points at `https://quiz.asso-triple-a.org/j/…`, not `localhost`.

## 9. Backups

The database contains the questions, the 90 figures and the results (~8–10 MB).

```sh
sudo install -d -m 0750 -o bia-quiz -g bia-quiz /var/lib/bia-quiz/backups

# one-off, consistent copy while the service runs
sudo -u bia-quiz sqlite3 /var/lib/bia-quiz/bia.sqlite3 \
  "VACUUM INTO '/var/lib/bia-quiz/backups/bia-$(date +%F).sqlite3'"

# daily at 03:17, keep 14 days
sudo crontab -u bia-quiz -e
17 3 * * * sqlite3 /var/lib/bia-quiz/bia.sqlite3 "VACUUM INTO '/var/lib/bia-quiz/backups/bia-$(date +\%F).sqlite3'" && find /var/lib/bia-quiz/backups -name 'bia-*.sqlite3' -mtime +14 -delete
```

Copy backups off the server (object storage, another host) — they contain student
nicknames. Encrypt at rest/transport if the destination is shared.

Restore:

```sh
sudo systemctl stop bia-quiz
sudo -u bia-quiz cp /var/lib/bia-quiz/backups/bia-2026-10-18.sqlite3 \
  /var/lib/bia-quiz/bia.sqlite3
sudo -u bia-quiz rm -f /var/lib/bia-quiz/bia.sqlite3-wal /var/lib/bia-quiz/bia.sqlite3-shm
sudo systemctl start bia-quiz
```

## 10. Upgrades and rollback

```sh
# 1. backup
sudo -u bia-quiz sqlite3 /var/lib/bia-quiz/bia.sqlite3 \
  "VACUUM INTO '/var/lib/bia-quiz/backups/pre-upgrade-$(date +%F-%H%M).sqlite3'"

# 2. on the controller: rebuild/stage, then let the app role restart
make artefacts
cd deploy/ansible
ansible-playbook site.yml --tags app
```

Manual fallback (keep the previous binary first):

```sh
# controller
scp -P <bia_ssh_port> deploy/artefacts/bia-quiz deploy@quiz.asso-triple-a.org:/tmp/
# server
sudo cp /opt/bia-quiz/bia-quiz /opt/bia-quiz/bia-quiz.previous
sudo install -m 0755 /tmp/bia-quiz /opt/bia-quiz/bia-quiz
sudo systemctl restart bia-quiz
```

If it fails: restore `bia-quiz.previous`, restore the DB backup if the migration ran, and
start. Migrations are forward-only.

To refresh the question bank without a new binary:

```sh
sudo systemctl stop bia-quiz          # no live room during an import
sudo -u bia-quiz /opt/bia-quiz/bia-import --fetch ...   # same command as §5
sudo systemctl start bia-quiz
```

## 11. Operations

- Logs: `journalctl -u bia-quiz -f`, `journalctl -u caddy -f`, `journalctl -u fail2ban -f`.
- Health: `curl -fsS https://…/healthz`; point an uptime monitor at it.
- Disk: `df -h /var`; the DB grows with results (indefinitely, current policy).
- Time: `timedatectl` (NTP on) — speed scoring uses the server clock.
- TLS renewal: automatic (Caddy).
- Updates: the `unattended_upgrades` role configures Debian stable + security updates
  and reboots automatically at 03:30 when required; rebuild/redeploy for the app.

## 12. Security notes

- The **teacher console is open** (`/teacher`): anyone with the URL can start a room.
  `/prof` and `/admin` use axum-login sessions (two config accounts). Passwords are
  plaintext in the root-only config and hashed at startup; sessions live in memory and are
  lost on restart. `/admin` requires the `admin` role.
- `/api/prof/*` and `/api/admin/*` are session-protected; `/healthz`, `/figures`, `/ws` and
  `/api/chapters` are public by design.
- Rate limiting: the app applies a per-IP request window (240/min by default) plus a
  login-failure limiter (5 / 15 min).
- Run as `bia-quiz`, never root; `ProtectSystem=strict` + `ReadWritePaths` in the unit.
- The DB contains personal data (nicknames + results, kept indefinitely). Back it up
  securely; revisit the retention policy if the school requires it.
- Set `server.source_url` to the deployed source (AGPL §13) — the landing page links to it.
- `fail2ban` protects SSH with the `sshd` jail; also use a provider firewall and 2FA on
  the provider account.

## 13. Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `502 Bad Gateway` from Caddy | app down or wrong port: `systemctl status bia-quiz`, `ss -ltnp \| grep 8080` |
| SSH connection refused after the play | the SSH port changed: use `ssh -p <bia_ssh_port>` and set `ansible_port` |
| Permission denied (publickey) | use `deploy` + `~/.ssh/bia-quiz-deploy`; password auth is disabled |
| `cannot execute binary file` | wrong OS/CPU: `file /opt/bia-quiz/bia-quiz` vs `uname -m`; run `make artefacts` |
| TLS certificate not issued | DNS not pointing at the server, or port 80/443 blocked (provider + `ufw`) |
| `/figures/…jpeg` 404 | DB has no figures: re-run §5; check `loaded 90 figures` in the journal |
| `/prof` redirects to `/login` | no session: log in with `prof` or `admin` |
| `/admin` returns 403 | logged in as `prof`; use the `admin` account |
| import returns 409 | a room is live, or another import is running |
| login always fails | empty/incorrect `[prof]`/`[admin]` password, or 5 failed attempts (15 min) |
| QR code points to `localhost` | `public_base_url` not set to the HTTPS domain |
| WebSocket disconnects | Caddy proxies `/ws`; check the browser console and `journalctl -u caddy` |
| `database is locked` | another import or a long write; retry after the import; WAL + 5 s busy timeout |
| Import downloads fail | no outbound HTTPS; use the CLI fallback in §5 with local TSV/figures |
| Disk full after months | results grow; prune sessions or increase the disk; check `/var/lib/bia-quiz` |

## 14. Auth and `/admin` (implemented)

- `/login` (server-rendered) signs in with the `prof` or `admin` account; `/logout` clears
  the session. `admin` is a superset of `prof` (results + import).
- `/prof` and `/admin` redirect to `/login?next=…`; `/api/prof/*` returns 401 JSON,
  `/api/admin/*` returns 401/403 JSON.
- `/admin` imports from the configured upstream URL only (`[import].upstream_url`); it
  reports progress, the final report and skipped rows, and reloads the figure cache. The
  downloaded TSV and figures live in a temporary directory and are deleted afterwards.
- Passwords: plaintext in `bia.toml`, Argon2id-hashed at startup; change them and restart.
- Sessions: in-memory `tower-sessions`, 12 h inactivity by default. A restart logs everyone
  out.
- The teacher console stays open by design; `/healthz`, `/figures`, `/ws`, `/api/chapters`
  remain public.
