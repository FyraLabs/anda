# Hacking Andaman

To quickly set up a developer environment to develop for Andaman. Install the `just` command runner and follow the instructions below.
There's also a devcontainer for this project if you would like to run it in a sandbox.

We at Fyra Labs develop Andaman under a GitHub codespaces dev container due to rust-analyzer's high memory usage, rendering our dev machines unusable and prone to crashes.

To initialize an Andaman developer environment, you need:

- Docker Compose
- Docker
- The BuildKit CLI
- `rustc` and `cargo`
- `just`
- k3d (or an existing Kubernetes cluster with credentials)
- `kubectl`
- `mc`/`mcli` (the minio CLI)

To quickly set up a developer environment, run the just task:
    
```bash
# But first, copy over the .env file from the example
cp .env.example .env
# Then run the just task
just dev-env
```

This will build all the necessary components and set up a development environment.

## Database schemas

Andaman uses [SeaORM](https://www.sea-ql.org/SeaORM/) as its ORM.
To make full use of the ORM, install the SeaORM CLI:
    
```bash
# Install the SeaORM CLI
cargo install sea-orm-cli

# Start migration
sea migrate up
```

Alternatively, you can directly run the SeaORM migrator exeuctable:
```bash
cargo run --bin anda-db-migration
```