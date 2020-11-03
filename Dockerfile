FROM debian:buster

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

ARG BINARY

WORKDIR /opt

COPY target/release/${BINARY} services/${BINARY}/Rocket.toml* /opt/

RUN chmod +x /opt/${BINARY}

ENV BINARY=$BINARY

ENTRYPOINT /opt/${BINARY}

RUN useradd -m rust

USER rust
