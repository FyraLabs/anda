serve:
    cargo run --bin anda-server


cli:
    cargo run --bin anda

web:
    cd anda-frontend && pnpm run dev


build-web:
    cd anda-frontend && pnpm run build


build-server:
    cargo build --release --bin anda-server

build-cli:
    cargo build --release --bin anda

build-backend: build-web build-server


clean:
    cargo clean
    rm -rf anda-frontend/dist