FROM alpine
WORKDIR /root/nether
RUN apk add --no-cache lld clang git rustup mtools && \
    ln -s /usr/bin/clang /usr/bin/cc && \
    ln -s /usr/bin/ld.lld /usr/bin/ld
RUN rustup-init -y --default-toolchain nightly --profile minimal && \
    source /root/.cargo/env && \
    rustup target add aarch64-unknown-none && \
    rustup component add rust-src
COPY . .
ENTRYPOINT ["/bin/sh", "-l"]
