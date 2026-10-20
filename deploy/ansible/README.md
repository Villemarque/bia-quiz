# Host preparation with Ansible: users, firewall, Caddy, upgrades and fail2ban

One role per task:

- **`users`** — Linux accounts, SSH keys, sudoers;
- **`firewall`** — UFW rules and default policy;
- **`sshd`** — the SSH daemon port;
- **`caddy`** — Caddy installation, `Caddyfile` and TLS;
- **`unattended_upgrades`** — Debian security updates and the apt daily timers;
- **`fail2ban`** — the SSH jail with bantime/findtime/maxretry policy;
- **`app`** — the `bia-quiz` systemd unit and service lifecycle.

The roles do not create the application config, database or imports. The `app` role
installs the systemd unit and can optionally copy the binary; deploy the files with
[`../README.md`](../README.md) §4–§9.

Targets: **Debian 12 (bookworm) / Debian 13 (trixie)**, reached over SSH with `sudo`
and Python 3.

## Quick start

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

The `app` role needs the binary/config from `deploy/README.md` §4. Run it later with
`ansible-playbook site.yml --tags app`.

Accept the host key first if needed: `ssh-keyscan -H <server> >> ~/.ssh/known_hosts`.

### First connection

The account Ansible connects as must already have `sudo`; the roles cannot bootstrap
their own access. On a blank server use **root** (as in `inventory.example.yml`) or the
provider's initial sudo user. The example `deploy` user reads
`~/.ssh/bia-quiz-deploy.pub`; after verifying key login, change `ansible_user` to
`deploy` for later runs.

Create the two example public keys on the controller (or repoint the file paths):

```sh
ssh-keygen -t ed25519 -f ~/.ssh/bia-quiz
ssh-keygen -t ed25519 -f ~/.ssh/bia-quiz-deploy
```

If the initial `root`/provider account only accepts a password, install `sshpass` and
run `ansible-playbook site.yml -k -K`, or set `ansible_password` and
`ansible_become_password` in the gitignored `inventory.yml`. Ansible does **not** prompt
for passwords unless `-k` (SSH) or `-K` (sudo) is passed.

The `sshd` role disables password authentication on the first run. Verify the `deploy`
key login **before** closing your session; later runs must use
`ansible_user: deploy` and its key (remove `ansible_password`).

### DNS and Caddy

Create the DNS `A`/`AAAA` record for `bia_domain` **before** running the playbook. Caddy
can only issue its certificate once the name resolves to the server and ports 80/443 are
reachable. The example uses `quiz.asso-triple-a.org`; change it if your hostname differs.

## What it does

- Creates the users in `bia_users`. The default is the unprivileged service account
  `bia-quiz` (`nologin`, `/var/lib/bia-quiz` mode `0750`).
- For any user with `ssh_authorized_key_files`, reads each controller file and installs
  the key with `ansible.posix.authorized_key`. Existing keys are **not** removed.
- For any user with `sudo: true`, writes `/etc/sudoers.d/<name>` with
  `NOPASSWD:ALL` and validates it with `visudo`.
- Installs UFW, allows `bia_ufw_allowed_tcp_ports` (default: SSH, 80, 443), and enables
  it with a default-deny incoming policy.
- Sets the SSH daemon port to `bia_ssh_port` through a drop-in config and restarts SSH
  after validating the config.
- Installs Caddy from the official repository, writes the `Caddyfile` for `bia_domain`,
  and starts/reloads it. Caddy obtains and renews the TLS certificate automatically and
  proxies to `127.0.0.1:{{ bia_app_port }}`.
- Installs and configures `unattended-upgrades` for Debian stable + security updates,
  and enables `apt-daily.timer` / `apt-daily-upgrade.timer`.
- Installs `fail2ban`, enables the `sshd` jail on `bia_ssh_port`, and starts the service.
- Installs the `bia-quiz` systemd unit and starts the service once the binary and
  `/etc/bia-quiz/bia.toml` exist. `make artefacts` stages the binaries in
  `deploy/artefacts/`; the unit is read from `deploy/bia-quiz.service`. A staged
  `deploy/artefacts/bia.toml` is installed if present, but the role never templates
  secrets itself.

It does **not**: create `/etc/bia-quiz/bia.toml`, import the question bank, configure
DNS, open ports in a cloud provider firewall, change root login, or remove SSH keys.
Those stay manual.

## Users

The default `bia_users` entry is the app service account:

```yaml
bia_users:
  - name: bia-quiz
    system: true
    group: bia-quiz
    home: /var/lib/bia-quiz
    shell: /usr/sbin/nologin
    create_home: true
    home_mode: "0750"
    ssh_authorized_key_files:
      - ~/.ssh/bia-quiz.pub
    sudo: false
```

Add an SSH admin user (see `group_vars/all.yml.example` for the active example):

```yaml
bia_users:
  - name: bia-quiz
    system: true
    group: bia-quiz
    home: /var/lib/bia-quiz
    shell: /usr/sbin/nologin
    create_home: true
    home_mode: "0750"
    ssh_authorized_key_files:
      - ~/.ssh/bia-quiz.pub
    sudo: false

  - name: deploy
    system: false
    group: deploy
    home: /home/deploy
    shell: /bin/bash
    create_home: true
    home_mode: "0700"
    ssh_authorized_key_files:
      - ~/.ssh/bia-quiz-deploy.pub
    sudo: true
```

`bia_users` is a list, so overriding it replaces the default list: keep the
`bia-quiz` entry if you still want the service account managed.

### User fields

| Field | Default | Meaning |
|---|---|---|
| `name` | required | Login name |
| `system` | `false` | System account (no aging, system UID range) |
| `group` | none | Primary group; created if set |
| `groups` | none | Supplementary groups |
| `append` | `false` | Append to supplementary groups instead of replacing |
| `home` | tool default | Home directory |
| `shell` | tool default | Login shell |
| `create_home` | `true` | Create the home directory |
| `home_mode` | `"0755"` | Home mode, applied when `home` is set and `create_home` is true |
| `ssh_authorized_key_files` | `[]` | Controller paths to `.pub` files (never removes existing) |
| `sudo` | `false` | Write a passwordless sudoers entry |

Paths are read on the controller; no key material belongs in group vars.

`sudo: true` grants **passwordless** sudo. Remove it or edit the generated
`/etc/sudoers.d/<name>` if you want a password prompt. The role also **removes**
`/etc/sudoers.d/<name>` for managed users with `sudo: false`.

## SSH port

```yaml
bia_manage_ssh_port: true
bia_ssh_port: 22
bia_ssh_password_authentication: false
```

- The role comments active `Port` and password-auth lines in `/etc/ssh/sshd_config`
  and writes `/etc/ssh/sshd_config.d/00-bia-quiz.conf`.
- The `00-` prefix matters: cloud images often ship
  `/etc/ssh/sshd_config.d/50-cloud-init.conf` with `PasswordAuthentication yes`.
  OpenSSH uses the first value, so a later `99-` drop-in would lose.
- It enables public-key authentication, disables `PasswordAuthentication` and
  `KbdInteractiveAuthentication`, validates the effective config with `sshd -T`, then
  restarts SSH.
- Password login is disabled by default. Verify key login before closing the current
  session; keep a console available until then.
- UFW and the fail2ban jail use the same `bia_ssh_port`.
- While changing ports, UFW also keeps `bia_ssh_previous_port` (default `22`) open.
  Remove that rule after verifying the new port.
- **After changing the port**, set `ansible_port` to the same value in
  `inventory.yml` for later runs and use `ssh -p <port>` / `scp -P <port>`.
- The current Ansible connection survives the restart; keep a console session open until
  the new port is verified.
- On an existing host, the old UFW rule is left in place; remove it manually after
  verifying the new port if you no longer want the old one open.

## Firewall

```yaml
bia_ssh_port: 22
bia_enable_ufw: true
bia_ufw_allowed_tcp_ports:
  - "{{ bia_ssh_port }}"
  - 80
  - 443
```

- Allow rules are added **before** UFW is enabled.
- The preflight assertion fails if `bia_ssh_port` is missing from the allowed list.
- Set `bia_enable_ufw: false` if a provider-level firewall already handles this.
- Test from a **second** SSH session before closing the first; keep the provider console
  available as a fallback.

## Caddy

```yaml
bia_enable_caddy: true
bia_domain: quiz.asso-triple-a.org   # public hostname
bia_app_port: 8080             # must match [server].bind in bia.toml
```

- Caddy is installed from the official Cloudsmith Debian repository.
- The generated `/etc/caddy/Caddyfile` is:
  ```
  <bia_domain> {
      log {
          output stdout
          format console
      }
      encode gzip
      reverse_proxy 127.0.0.1:<bia_app_port>
  }
  ```
- Caddy handles the WebSocket upgrade, HTTP→HTTPS redirect and certificate renewal.
- Access logs go to stdout, i.e. `journalctl -u caddy -f`.
- Set `bia_enable_caddy: false` if Caddy is managed elsewhere.
- The role fails fast if `bia_domain` is empty or `bia_app_port` is invalid.
- Rate limiting stays in the application, not in Caddy.

## Unattended upgrades

```yaml
bia_enable_unattended_upgrades: true
bia_unattended_automatic_reboot: true
bia_unattended_reboot_time: "03:30"
bia_unattended_remove_unused_kernel_packages: true
bia_unattended_remove_new_unused_dependencies: false
```

- Applies Debian stable and Debian security updates; the Caddy/Cloudsmith repo is not
  included in the automatic origins.
- `Automatic-Reboot` is **on** by default at `03:30`; set `bia_unattended_automatic_reboot: false` if you prefer to reboot manually.
- Writes `/etc/apt/apt.conf.d/20auto-upgrades` and
  `/etc/apt/apt.conf.d/50unattended-upgrades`.
- Enables the `apt-daily.timer` and `apt-daily-upgrade.timer` timers.
- Set `bia_enable_unattended_upgrades: false` to skip the role entirely.

Check the schedule with `systemctl list-timers 'apt-daily*'` and the journal with
`journalctl -u apt-daily-upgrade.service`.

## fail2ban

```yaml
bia_enable_fail2ban: true
bia_fail2ban_bantime: "1h"
bia_fail2ban_findtime: "10m"
bia_fail2ban_maxretry: 5
bia_fail2ban_ignoreip: "127.0.0.1/8 ::1"
```

- Installs `fail2ban` and `python3-systemd`.
- Writes `/etc/fail2ban/jail.local`, enabling the `sshd` jail on
  `bia_ssh_port` with the systemd backend.
- Whitelist trusted addresses in `bia_fail2ban_ignoreip` (space-separated CIDRs).
- Check with `fail2ban-client status sshd`; logs are in `journalctl -u fail2ban`.

## Application service

```yaml
# default: copy the files staged by `make artefacts`
bia_app_copy_binary: true
```

- `make artefacts` stages `bia-quiz`, `bia-import` and `bia.toml.example` in the
  gitignored `deploy/artefacts/` directory. The systemd unit stays in
  `deploy/bia-quiz.service`.
- The role copies the binaries from `deploy/artefacts/` and the unit from
  `deploy/bia-quiz.service`.
- It also installs `deploy/artefacts/bia.toml` if that optional file exists; otherwise
  `/etc/bia-quiz/bia.toml` must already be present. The config contains passwords, so
  it is never committed or templated with secrets by this role.
- It requires `/opt/bia-quiz/bia-quiz` and `/etc/bia-quiz/bia.toml`; if either is
  missing it stops with a clear error instead of starting a broken service.
- First host-prep run: use `--skip-tags app`; after `make artefacts` and the config,
  run `ansible-playbook site.yml --tags app`.
- Importing the question bank remains an explicit `/admin` or CLI step; the role does
  not populate the database.

## Tags

```sh
ansible-playbook site.yml --tags users      # users, keys, sudoers
ansible-playbook site.yml --tags firewall   # UFW install, rules, enable
ansible-playbook site.yml --tags sshd       # SSH daemon port
ansible-playbook site.yml --tags caddy      # Caddy install, config, TLS
ansible-playbook site.yml --tags unattended # unattended-upgrades
ansible-playbook site.yml --tags fail2ban   # fail2ban sshd jail
ansible-playbook site.yml --tags app        # systemd unit + app service
```

The firewall/SSH-port assertion runs with the firewall role.

## Logs and retries

`ansible.cfg` writes a log to `deploy/ansible/ansible.log` when you run the playbook
from this directory. It is gitignored. For a one-off verbose log:

```sh
ANSIBLE_LOG_PATH=/tmp/bia-ansible.log \
  ansible-playbook -vvv site.yml 2>&1 | tee /tmp/bia-ansible-console.log
```

- The failed task is the last `fatal:` block before `PLAY RECAP`; the recap shows
  `failed=…` and the host.
- List task names with `ansible-playbook site.yml --list-tasks`.
- Resume after a failure:
  ```sh
  ansible-playbook site.yml --start-at-task="Task name"
  ```
- Run one task at a time with `--step`.
- Use `--tags users`, `--tags sshd`, etc. to rerun only the failed area.
- Server-side logs:
  ```sh
  journalctl -xe
  journalctl -u ssh -u caddy -u fail2ban -n 100 --no-pager
  systemctl status ssh caddy fail2ban
  sshd -t
  sshd -T | grep -Ei 'port|passwordauth|kbd|pubkey'
  ufw status verbose
  ```

If the playbook changed the SSH port or disabled password auth before failing, reconnect
using the new port and `deploy` key (or the provider console), then fix `inventory.yml`
before rerunning.

## Requirements

- `community.general` for `ufw`
- `ansible.posix` for `authorized_key`

Installed by `ansible-galaxy collection install -r requirements.yml`.

## App deployment

Continue with [`README.md`](../README.md) §4. On the controller run `make artefacts`;
the `app` role copies the binaries from `deploy/artefacts/` and the unit directly from
`deploy/bia-quiz.service`. Put the real config directly at
`deploy/artefacts/bia.toml` (copy the staged `bia.toml.example` and edit it). The role
installs it if present; otherwise deploy `/etc/bia-quiz/bia.toml` manually.

```sh
ansible-playbook site.yml --tags app
```

It installs the systemd unit and starts the service after checking both files exist.
It does not import the question bank; use `/admin` or the CLI. Caddy is already
configured by the `caddy` role.
