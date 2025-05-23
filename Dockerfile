##########
# Binary #
##########

FROM rust:1.85.0-alpine3.21 AS binary_builder

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

##############
# Javascript #
##############

FROM denoland/deno:alpine-2.2.1 AS js_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache make

WORKDIR /template-web-server

# copy required files
COPY Makefile .
COPY static/script/ static/script/
COPY deno.jsonc .

# generate Javascript
RUN make gen_js

#######
# CSS #
#######

FROM node:23.8.0-alpine3.21 AS css_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache make

# install Sass
RUN npm install -g sass@1.71.1

WORKDIR /template-web-server

# copy required files
COPY Makefile .
COPY static/scss/ static/scss/

# generate stylesheet(s)
RUN make gen_css

#######################
# Template Web Server #
#######################

FROM alpine:3.21.3

# update alpine linux dependencies
RUN apk update

WORKDIR /template-web-server

# copy binary
COPY --from=binary_builder /template-web-server/target/release/template-web-server .

# copy scripts
COPY --from=js_builder /template-web-server/bin/static/script/ static/script/

# copy stylesheets
COPY --from=css_builder /template-web-server/bin/static/stylesheet/ static/stylesheet/

# copy non-generative static assets
COPY html/ html/
COPY static/file/ static/file/
COPY static/image/ static/image/

# run server
EXPOSE 8080
ENTRYPOINT ["./template-web-server"]

#ENTRYPOINT ["tail", "-f", "/dev/null"]
