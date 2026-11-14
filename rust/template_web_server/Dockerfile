# Build context: the repo root (`make docker_build`), because the image needs
# both the Rust workspace and the TypeScript bundler. Ignore rules live next to
# this file in Dockerfile.dockerignore.

##########
# Binary #
##########

FROM rust:1.100.0-alpine3.24 AS binary_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache git make musl-dev

WORKDIR /toddstack/rust

# The whole workspace: Cargo refuses to build if any workspace member's
# manifest is missing, so copying crates one by one breaks the moment a crate
# is added.
COPY rust/ ./

# generate binary
RUN cargo build --release --package template-web-server --bin template-web-server

#######
# CSS #
#######

FROM node:26.7.0-alpine3.24 AS css_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache make

# install Sass
RUN npm install -g sass@1.103.1

WORKDIR /toddstack

# copy required files
COPY Makefile .
COPY rust/template_web_server/static/scss/ rust/template_web_server/static/scss/

# generate stylesheet(s)
RUN make gen_css

##################
# Static Assets  #
##################

# One stage, not three. The generator is a static musl binary, so it runs
# happily inside the Deno image — which means the whole pipeline stays in one
# place and the build keeps the same four stages it had before.
#
# The order is load-bearing: the JavaScript build INLINES the manifest, so
# hashing has to happen first; and the built JavaScript can only be hashed once
# it exists, so the scripts are a second pass. Scripts never need their own
# hash — the layout reads that from the manifest.
FROM denoland/deno:alpine-2.9.5 AS static_builder

RUN apk update
RUN apk add --no-cache make

WORKDIR /toddstack

COPY Makefile ./
COPY typescript/ typescript/
COPY rust/template_web_server/html/ rust/template_web_server/html/
COPY rust/template_web_server/static/ rust/template_web_server/static/
COPY --from=css_builder /toddstack/rust/template_web_server/bin/static/stylesheet/ rust/template_web_server/bin/static/stylesheet/
COPY --from=binary_builder /toddstack/rust/target/release/template-web-server rust/template_web_server/bin/

RUN make gen_static \
 && make gen_static_assets \
 && make gen_js \
 && make gen_static_scripts

#######################
# Template Web Server #
#######################

FROM alpine:3.24.1

# update alpine linux dependencies
RUN apk update

WORKDIR /template-web-server

# The runtime tree is exactly what the pipeline produced: hashed assets, the
# manifest that names them, the templates, and the binary. Nothing is generated
# or renamed at startup, so this filesystem is never written to — the image can
# be read-only, and an in-place restart cannot corrupt it.
COPY --from=static_builder /toddstack/rust/template_web_server/bin/ ./

# run server
EXPOSE 8080
ENTRYPOINT ["./template-web-server"]
