{ pkgs, ... }:
{
  packages = with pkgs; [
    # Rust
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
    pkg-config
    gcc
    gnumake
    zig

    # SPA
    nodejs_22
    pnpm

    # Runtime / tooling
    sqlite
    caddy
    git
    curl
    jq
    file
    openssh

    # Deploy automation
    ansible
    ansible-lint
    sshpass
  ];

  env = {
    BIA_QUIZ_PROF_PASSWORD = "dev";
    BIA_QUIZ_ADMIN_PASSWORD = "dev";
    BIA_QUIZ_PUBLIC_BASE_URL = "http://localhost:5173";
    RUST_BACKTRACE = "1";
    RUST_LOG = "bia_quiz=info,tower_http=info";
  };

  scripts = {
    setup.exec = "pnpm --dir frontend install && make build";
    build.exec = "make build";
    run.exec = "make run";
    test.exec = "make test";
    check.exec = "make check";
    fmt.exec = "cargo fmt --all";
    lint.exec = "cargo clippy --workspace --all-targets -- -D warnings";
    import.exec = "make import";
    db.exec = "sqlite3 bia.sqlite3";
    "frontend-install".exec = "pnpm --dir frontend install";
    "frontend-check".exec = "pnpm --dir frontend check";
    "ansible-collections".exec = "cd deploy/ansible && ansible-galaxy collection install -r requirements.yml";
    "ansible-check".exec = "cd deploy/ansible && ansible-playbook -i inventory.example.yml --syntax-check site.yml";
    "ansible-lint".exec = "ansible-lint deploy/ansible";
  };

  processes = {
    backend.exec = "cargo run -p bia-server --bin bia-quiz";
    frontend.exec = "pnpm --dir frontend dev";
  };

  enterShell = ''
    echo "bia-quiz development shell"
    echo "  setup            install SPA deps and build the release binary"
    echo "  devenv up        run backend (8080) + Vite (5173)"
    echo "  test / check     Rust + Svelte checks"
    echo "  ansible-collections  install deploy collections"
    echo "  ansible-check    syntax-check the deploy playbook"
    echo "  ansible-lint     lint the deploy roles"
  '';
}
