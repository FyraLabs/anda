FROM rust:latest as builder

WORKDIR /build

COPY . .

RUN cargo build --release --bin anda-server

FROM fedora:latest as runtime

COPY --from=builder /build/target/release/anda-server /usr/bin/anda-server

ENV ROCKET_ADDRESS=0.0.0.0

EXPOSE 8000

CMD ["/usr/bin/anda-server"]