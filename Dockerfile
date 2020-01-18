FROM rust:alpine as build
ENV RUSTFLAGS='-C target-feature=-crt-static'
WORKDIR /app
COPY . .
RUN apk add --no-cache musl-dev
RUN cargo install --path .

FROM alpine:latest
COPY --from=build /usr/local/cargo/bin/tweet-provider /app/tweet-provider
WORKDIR /app
RUN apk add --no-cache libgcc
CMD ["./tweet-provider"]
