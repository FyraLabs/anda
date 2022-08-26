set dotenv-load

build_all: build-backend build-cli


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

watch-web:
    cd anda-frontend && pnpm run watch

format:
    cargo fmt --all

# Test commands for quickly creating a new test environment

docker-client:
    DOCKER_BUILDKIT=1 docker build -f client.dockerfile -t localhost:5050/anda/anda-client:latest .
    docker push localhost:5050/anda/anda-client:latest

test-cluster:
    k3d cluster create --registry-create local-registry:5050

test-buildkit:
    docker run -d --name test-buildkitd --publish 1234:1234 --privileged moby/buildkit:latest -addr "tcp://0.0.0.0:1234"

docker-server:
    docker build -t localhost:5050/anda/anda:latest .

docker-compose:
    docker-compose up -d

minio-client: docker-compose
    mcli alias set anda http://localhost:9000 minioadmin minioadmin

dev-env: docker-compose test-cluster test-buildkit docker-client

test-push-anda:
    cargo run --bin anda push --target owo --scope test

