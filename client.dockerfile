FROM fedora:latest as builder

WORKDIR /build

COPY . .

RUN dnf install -y rustc cargo openssl-devel

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --bin anda && cp /build/target/release/anda /usr/bin/anda


FROM fedora:latest as runtime


RUN dnf install -y wget curl
# download buildctl
RUN wget https://github.com/moby/buildkit/releases/download/v0.10.3/buildkit-v0.10.3.linux-amd64.tar.gz -O /buildkit.tar.gz

RUN tar -xzf /buildkit.tar.gz -C /usr/local/

COPY --from=builder  /usr/bin/anda /usr/bin/anda

RUN dnf install -y openssl-libs

ENTRYPOINT ["/usr/bin/anda"]