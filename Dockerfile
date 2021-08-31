FROM rust

WORKDIR /build/
COPY ./Cargo.lock ./Cargo.toml ./
COPY ./src/ ./src/
RUN cargo build --locked --release --bins

FROM scratch

COPY --from=0 /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=0 /etc/passwd /etc/group /etc/
COPY --from=0 /build/target/release/bdns-resolver /

USER nobody:nogroup
STOPSIGNAL SIGINT

# ENV HEALTHCHECK_ENABLE 1
# HEALTHCHECK CMD healthy http://127.0.0.1:8000/ping || exit 1

ENTRYPOINT ["/bdns-resolver"]
EXPOSE 8000
