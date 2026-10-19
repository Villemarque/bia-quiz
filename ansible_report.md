# Ansible security review: `deploy/ansible`

Scope: `site.yml`, `ansible.cfg`, `requirements.yml`, example inventory/vars, the seven roles
(`users`, `firewall`, `sshd`, `caddy`, `unattended_upgrades`, `fail2ban`, `app`) and the files
they install (`deploy/bia-quiz.service`, `deploy/bia.toml.example`). This is a static review
only; nothing was run against the server. Date: 2026-10-18.

## Summary

The baseline is good: key-only SSH with a verified `sshd -T` assertion, default-deny UFW with
a lockout guard, fail2ban, automatic security updates, a non-root service with a root-owned
binary, and a `0640 root:bia-quiz` config. Local inventory and vars are gitignored.

The main gaps are a handful of places where a mistake or failed run leaves the host *less*
secure than the playbook claims.

| # | Severity | Finding |
|---|---|---|
| 1 | High | `app` role will deploy the example passwords (`change-me`) without complaint |
| 2 | High | SSH hardening is not applied if a run fails after the config is written (handler never fires on the next run) |
| 3 | Medium | `bia.toml` (plaintext passwords) can leak via `--diff` into the terminal and `ansible.log` |
| 4 | Medium | Caddy is outside unattended upgrades, so the internet-facing proxy is never patched automatically |
| 5 | Medium | Caddy signing key is fetched to a predictable `/tmp` path with no fingerprint pinning |
| 6 | Medium | `deploy` has `NOPASSWD:ALL`; the initial provider account and root key are never retired |
| 7 | Medium | sshd hardening is minimal (`PermitRootLogin`, `AllowUsers`, forwarding, auth limits) |
| 8 | Medium | Caddy sends no security headers and does not protect `/admin` or `/prof` at the edge |
| 9 | Low | `bia_manage_ssh_port: false` also disables the password-auth lockdown |
| 10 | Low | Authorized keys are only ever added, never removed; the service account gets a key |
| 11 | Low | systemd unit is only lightly sandboxed; `StateDirectory` may reset the 0750 home mode |
| 12 | Low | Old SSH port stays open by default; SSH is world-reachable; lockout guard ignores `ansible_port` |
| 13 | Low | Controller side: unpinned collections, TOFU host key, world-readable `ansible.log` |
| 14 | Low | Unattended reboot and service restarts drop the in-memory live room; no failure notification |

---

## 1. High: example passwords can be deployed

`deploy/bia.toml.example` ships `password = "change-me"` and `"change-me-too"`. The quick start
tells the operator to copy the example to `deploy/artefacts/bia.toml` and edit it. The `app`
role copies whatever is at `deploy/artefacts/bia.toml` and only checks that the file exists
(`roles/app/tasks/main.yml`). An unedited copy goes live with publicly known admin/prof
credentials, and `/admin` can trigger imports and read all session data.

Fix: fail the play when the staged config has placeholder or empty passwords. Run it on the
controller, before the copy, with `no_log`:

```yaml
- name: Read the staged config
  ansible.builtin.slurp: { src: "{{ bia_app_config_src }}" }
  delegate_to: localhost
  become: false
  register: bia_cfg
  no_log: true
  when: bia_app_staged_config.stat.exists

- name: Refuse placeholder or short passwords
  ansible.builtin.assert:
    that:
      - "'change-me' not in (bia_cfg.content | b64decode)"
    fail_msg: "bia.toml still contains the example passwords."
  no_log: true
  when: bia_app_staged_config.stat.exists
```

A stricter variant parses the TOML and requires length of at least 16 for both passwords.

## 2. High: sshd handler is skipped on re-runs

The `sshd` role writes `/etc/ssh/sshd_config.d/00-bia-quiz.conf` and notifies `Restart SSH`.
Handlers run at the end of the play, and **not at all if any later task or role fails**
(`caddy`, `fail2ban`, `app` all come after). On the next run the template is unchanged, so
nothing is "changed", the handler is not notified, and sshd keeps running its old config
(password auth still on, old port). The `sshd -T` assertion reads the *files*, not the running
daemon, so it passes and gives false assurance.

Fix: flush right after the verification, and verify the live daemon:

```yaml
- name: Apply SSH changes now
  ansible.builtin.meta: flush_handlers
```

Alternatively set `force_handlers: true` on the play, or restart unconditionally. Because the
`firewall` role already runs first and keeps both ports open, flushing early is safe. Consider
also asserting from the controller (`wait_for` on the new port) before moving on.

## 3. Medium: secrets can leak through `--diff` and the log

`roles/app/tasks/main.yml` copies `bia.toml` (plaintext passwords) with no `no_log` or
`diff: false`. `deploy/README.md` and the role docs recommend `--check --diff`. On the run
where the `app` role first installs or changes the config, Ansible prints the file diff to the
terminal and, because `ansible.cfg` sets `log_path`, writes it to `ansible.log`.

Fix:

```yaml
- name: Install the staged app config
  ansible.builtin.copy: ...
  diff: false
  no_log: true
```

Also `chmod 600 ansible.log` (or log to a directory outside the repo) and tell operators to
treat the log as sensitive. I checked the current `ansible.log`: it contains hostnames, key
paths and commands but no secrets (the "password" matches are `PasswordAuthentication=no` SSH
options).

## 4. Medium: Caddy is never patched automatically

`50unattended-upgrades.j2` allows only Debian and Debian-Security origins, with the comment
"The Caddy repository is not included". Caddy is the only component directly exposed to the
internet, and its TLS/HTTP stack gets security fixes regularly. It will stay at the installed
version until someone runs `apt upgrade`.

Fix: add the Caddy repository origin (check the exact pair with `apt-cache policy caddy`),
for example `"origin=cloudsmith/caddy/stable,codename=any-version,label=..."`. Alternatively,
install Caddy from Debian's own archive, which unattended-upgrades already covers, at the cost
of an older release.

## 5. Medium: Caddy repository key handling

`roles/caddy/tasks/main.yml`:

- Downloads the key to the fixed path `/tmp/caddy-stable.gpg.key`. On a shared host a local
  user could pre-create that path or a symlink. Use a root-owned temp dir (`tempfile`) or
  `/root`.
- No checksum or fingerprint is verified; trust rests entirely on TLS to `dl.cloudsmith.io`.
  Pin the expected key fingerprint and assert it after `gpg --show-keys`.
- `creates:` means a rotated or revoked key is never refreshed.
- The apt source file is also fetched from the network and written as-is. Write it from a
  template with `signed-by=` fixed instead.
- `debian-keyring` and `debian-archive-keyring` are installed but not needed.

## 6. Medium: passwordless root and leftover accounts

- Every user with `sudo: true` gets `NOPASSWD:ALL` (`roles/users/tasks/main.yml`). A stolen
  `~/.ssh/bia-quiz-deploy` key is immediate root. Mitigate with a passphrase plus ssh-agent on
  the controller, or require a sudo password and run with `-K`.
- The bootstrap account (`debian` in the local inventory, or `root`) is never disabled. On many
  providers it already has a key, passwordless sudo and sometimes a password. Once `deploy`
  works, lock it (`passwd -l`), remove its keys or set `AllowUsers deploy`, and set
  `PermitRootLogin no`. This is not in the runbook's cleanup list.

## 7. Medium: sshd hardening is minimal

`port.conf.j2` sets only `Port`, `PubkeyAuthentication` and the two password options. Debian's
default leaves `PermitRootLogin prohibit-password`, so root key login remains. Suggested
additions: `PermitRootLogin no`, `AllowUsers {{ deploy user }}`, `MaxAuthTries 3`,
`LoginGraceTime 30`, `X11Forwarding no`, `AllowAgentForwarding no`, `AllowTcpForwarding no`,
`PermitEmptyPasswords no`. Also assert `permitrootlogin no` in the `sshd -T` check.

Note: `sshd -T` without `-C` does not evaluate `Match` blocks, so the assertion cannot see
per-user overrides.

## 8. Medium: Caddy edge hardening

`Caddyfile.j2` is just `encode gzip` + `reverse_proxy`. Missing:

- Headers: `Strict-Transport-Security`, `X-Content-Type-Options nosniff`, `X-Frame-Options
  DENY` (or CSP `frame-ancestors 'none'`; the admin "import" button is clickjackable today),
  `Referrer-Policy`, and a CSP. Neither Caddy nor the app currently sets any.
- `/admin*`, `/api/admin/*` and `/prof*` are reachable from the whole internet. Defence in
  depth: an IP allowlist or `basic_auth` for the admin paths. This matters more because the
  app's teacher WebSocket is currently unauthenticated (see `security_report.md`).
- No request timeouts or body limits at the edge.
- The app trusts `X-Forwarded-For` for its rate limiter. Caddy overwrites the header from
  untrusted clients by default; do **not** add `trusted_proxies` without reviewing that.
- Consider `admin off` (the Caddy admin API listens on `localhost:2019` and is reachable by
  any local user).

Positive: the app binds `127.0.0.1` and UFW denies 8080 anyway.

## 9. Low: `bia_manage_ssh_port` gates the whole hardening

Every task in `roles/sshd/tasks/main.yml`, including "comment out PasswordAuthentication" and
the `sshd -T` password assertion, is conditioned on `bia_manage_ssh_port`. Setting it to
`false` to keep the default port silently keeps password login. Split into
`bia_manage_sshd` (hardening) and `bia_manage_ssh_port` (port only), or document the coupling
prominently. The fail2ban jail also keeps using `bia_ssh_port` when the port is unmanaged.

## 10. Low: key lifecycle

- `ansible.posix.authorized_key` has `state: present` and no `exclusive`, so removing a key
  from `ssh_authorized_key_files` never revokes it. Add an `exclusive: true` option (for
  managed users) or a documented revocation step.
- The default and example vars install a key for the `bia-quiz` service account. Its shell is
  `nologin`, so it is not exploitable, but the service has write access to its own home via
  `ReadWritePaths`, so there is no reason to place a credential there. Use
  `ssh_authorized_key_files: []` for it.
- Keys have no restrictions (`from=`, `no-agent-forwarding`) and no expiry.

## 11. Low: service unit and state directory

`deploy/bia-quiz.service` (installed by the `app` role) already has `NoNewPrivileges`,
`PrivateTmp`, `ProtectSystem=strict`, `ProtectHome`. Missing sandboxing:
`PrivateDevices=yes`, `ProtectKernelTunables/Modules/ControlGroups=yes`,
`RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `RestrictNamespaces=yes`,
`LockPersonality=yes`, `MemoryDenyWriteExecute=yes`, `CapabilityBoundingSet=`,
`SystemCallFilter=@system-service`, `ProtectClock`, `ProtectHostname`.

`StateDirectory=bia-quiz` defaults to `StateDirectoryMode=0755`, and systemd re-applies it on
service start, so it may undo the `0750` set by the `users` role (and make the role report
"changed" on every run). With the default umask the SQLite file, which holds student
nicknames and results, is then world-readable by other local users. Verify with
`stat -c %a /var/lib/bia-quiz`; fix with `StateDirectoryMode=0750` and `UMask=0077`.

## 12. Low: firewall

- `bia_ssh_previous_port` defaults to 22 and stays open indefinitely; removal is manual.
  Add an explicit "closeout" variable/task.
- The lockout guard only checks `bia_ssh_port`. If the play is run over a different
  `ansible_port` (e.g. a provider that uses a non-standard port) enabling UFW will lock the
  controller out. Add `ansible_port | default(22) in bia_ufw_allowed_tcp_ports`.
- SSH is open to the whole internet (obscure port plus fail2ban only). If the operator has a
  stable IP, restrict by `from_ip`.
- No `ufw logging` setting and no outbound policy (default allow).
- fail2ban only protects SSH. Check `fail2ban-client status sshd` after a run to confirm the
  ban action actually works with the host's nftables/UFW setup.

## 13. Low: controller side

- `requirements.yml` does not pin collection versions (`community.general`, `ansible.posix`).
  Pin versions (and, ideally, checksums via a lock/`ansible-galaxy` with a requirements lock).
- README suggests `ssh-keyscan -H <server> >> ~/.ssh/known_hosts`, which blindly trusts the
  first response. Verify the fingerprint through the provider's console instead.
- `ansible.log` is `0644` and sits inside the repo (gitignored). Keep it out of backups and
  shares.
- `deprecation_warnings = False` hides useful upgrade signals.
- The README mentions `ansible_password` in `inventory.yml`; prefer `-k/-K` prompts or
  `ansible-vault`. The inventory is gitignored, which is good. Confirm with
  `git check-ignore -v deploy/ansible/inventory.yml` before committing.
- Release binaries are copied from `deploy/artefacts/` with no checksum or provenance check.
  Record a `sha256` at build time and verify it in the role.

## 14. Low: availability side effects with security impact

- `Automatic-Reboot "true"` at 03:30 and the `Restart bia-quiz` handler both discard the
  in-memory live room (rooms are not persisted). An upgrade during a class ends the quiz.
  Consider a "refuse to restart if a room is live" check (the `/api/admin/*` API exposes it)
  or document upgrading outside lessons.
- No `Unattended-Upgrade::Mail`, so failed upgrades are silent. The defaults file comment
  ("Reboots are left to the operator by default") contradicts the default value `true`.
- `needrestart` is not installed, so long-running services may keep old libraries after
  updates until the nightly reboot.

---

## Checked and fine

- `lineinfile` edits to `sshd_config` are anchored, idempotent, and the drop-in uses the `00-`
  prefix so cloud-init's `50-` file cannot override it.
- `visudo -cf` validates sudoers fragments; they are `0440 root:root`.
- Config is `0640 root:bia-quiz` in a root-owned directory; binaries are root-owned `0755`.
- UFW enables only after the SSH port is allowed, with an assertion guarding it.
- Unattended upgrades are limited to Debian and Debian-Security origins (no third-party
  auto-install, except for the gap in finding 4).
- Local secrets files (`inventory.yml`, `group_vars/all.yml`, `deploy/artefacts/`, `*.log`) are
  gitignored and not tracked (`git ls-files` shows only the `*.example` files).

## Suggested order of work

1. Findings 1, 2 and 3 (small changes; they prevent the worst outcomes).
2. Findings 6 and 7 (retire the bootstrap account, tighten sshd).
3. Findings 4, 5 and 8 (Caddy: patching, key pinning, headers, admin-path protection).
4. The remaining Low items as time allows.
