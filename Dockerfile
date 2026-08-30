##########
# Binary #
##########

FROM rust:1.98.0-alpine3.24 AS binary_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache git make musl-dev

WORKDIR /template-web-server

# copy required files
COPY .clippy.toml .
COPY Cargo.toml .
COPY Cargo.lock .
COPY template_web_server template_web_server
COPY webserver_base webserver_base

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

WORKDIR /template-web-server

# copy required files
COPY Makefile .
COPY static/scss/ static/scss/

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

WORKDIR /template-web-server

COPY Makefile deno.jsonc ./
COPY html/ html/
COPY static/ static/
COPY --from=css_builder /template-web-server/bin/static/stylesheet/ bin/static/stylesheet/
COPY --from=binary_builder /template-web-server/target/release/template-web-server bin/

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
COPY --from=static_builder /template-web-server/bin/ ./

# run server
EXPOSE 8080
ENTRYPOINT ["./template-web-server"]
