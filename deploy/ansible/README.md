# Host preparation with Ansible: users, firewall, Caddy, upgrades and fail2ban

One role per task:

- **`users`** — Linux accounts, SSH keys, sudoers;
- **`firewall`** — UFW rules and default policy;
- **`caddy`** — Caddy installation, `Caddyfile` and TLS;
- **`unattended_upgrades`** — Debian security updates and the apt daily timers;
- **`fail2ban`** — the SSH jail with bantime/findtime/maxretry policy.

The roles do not touch the application binary, config, application systemd unit,
database or imports. Deploy the app with [`../README.md`](../README.md) §4–§9.

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

ansible-playbook site.yml --check --diff
ansible-playbook site.yml
```

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

### DNS and Caddy

Create the DNS `A`/`AAAA` record for `bia_domain` **before** running the playbook. Caddy
can only issue its certificate once the name resolves to the server and ports 80/443 are
reachable. Keep `bia_domain = quiz.example.org` only as a placeholder; the Caddy role
refuses to run with it.

## What it does

- Creates the users in `bia_users`. The default is the unprivileged service account
  `bia-quiz` (`nologin`, `/var/lib/bia-quiz` mode `0750`).
- For any user with `ssh_authorized_key_files`, reads each controller file and installs
  the key with `ansible.posix.authorized_key`. Existing keys are **not** removed.
- For any user with `sudo: true`, writes `/etc/sudoers.d/<name>` with
  `NOPASSWD:ALL` and validates it with `visudo`.
- Installs UFW, allows `bia_ufw_allowed_tcp_ports` (default: SSH, 80, 443), and enables
  it with a default-deny incoming policy.
- Installs Caddy from the official repository, writes the `Caddyfile` for `bia_domain`,
  and starts/reloads it. Caddy obtains and renews the TLS certificate automatically and
  proxies to `127.0.0.1:{{ bia_app_port }}`.
- Installs and configures `unattended-upgrades` for Debian stable + security updates,
  and enables `apt-daily.timer` / `apt-daily-upgrade.timer`.
- Installs `fail2ban`, enables the `sshd` jail on `bia_ssh_port`, and starts the service.

It does **not**: deploy the app, create the app database, configure DNS, open ports in a
cloud provider firewall, harden `sshd`, disable root login, or remove SSH keys. Those stay
manual.

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
bia_domain: quiz.example.org   # replace with the real public hostname
bia_app_port: 8080             # must match [server].bind in bia.toml
```

- Caddy is installed from the official Cloudsmith Debian repository.
- The generated `/etc/caddy/Caddyfile` is:
  ```
  <bia_domain> {
      encode gzip
      reverse_proxy 127.0.0.1:<bia_app_port>
  }
  ```
- Caddy handles the WebSocket upgrade, HTTP→HTTPS redirect and certificate renewal.
- Set `bia_enable_caddy: false` if Caddy is managed elsewhere.
- The role fails fast if `bia_domain` is still the example or `bia_app_port` is invalid.
- Rate limiting stays in the application, not in Caddy.

## Unattended upgrades

```yaml
bia_enable_unattended_upgrades: true
bia_unattended_automatic_reboot: false
bia_unattended_reboot_time: "03:30"
bia_unattended_remove_unused_kernel_packages: true
bia_unattended_remove_new_unused_dependencies: false
```

- Applies Debian stable and Debian security updates; the Caddy/Cloudsmith repo is not
  included in the automatic origins.
- `Automatic-Reboot` is **off** by default; reboot manually during a maintenance window.
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

## Tags

```sh
ansible-playbook site.yml --tags users      # users, keys, sudoers
ansible-playbook site.yml --tags firewall   # UFW install, rules, enable
ansible-playbook site.yml --tags caddy      # Caddy install, config, TLS
ansible-playbook site.yml --tags unattended # unattended-upgrades
ansible-playbook site.yml --tags fail2ban   # fail2ban sshd jail
```

The firewall/SSH-port assertion runs with the firewall role.

## Requirements

- `community.general` for `ufw`
- `ansible.posix` for `authorized_key`

Installed by `ansible-galaxy collection install -r requirements.yml`.

## App deployment

Continue with [`README.md`](../README.md) §4. The roles create the service account and
its home; still create `/var/lib/bia-quiz/backups` and install the files/config as
described there. Caddy is already configured by the `caddy` role.
