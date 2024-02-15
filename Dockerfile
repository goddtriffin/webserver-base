FROM --platform=linux/amd64 rust:1.75.0-alpine3.19 AS binary_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache git make musl-dev

WORKDIR /template-web-server

# copy required files
COPY .clippy.toml .
COPY Cargo.toml .
COPY Cargo.lock .
COPY server server

# generate binary
RUN cargo build --release --package server --bin server

FROM --platform=linux/amd64 denoland/deno:alpine-1.40.2 as js_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache make

WORKDIR /template-web-server

# copy required files
COPY Makefile .
COPY ui/static/script/ ui/static/script/
COPY deno.jsonc .

# generate Javascript
RUN make gen_js

FROM --platform=linux/amd64 node:21.6.0-alpine3.19 as css_builder

# update alpine linux dependencies
RUN apk update
RUN apk add --no-cache make

# install Sass
RUN npm install -g sass@1.70.0

WORKDIR /template-web-server

# copy required files
COPY Makefile .
COPY ui/static/scss/ ui/static/scss/

# generate stylesheet(s)
RUN make gen_css

FROM --platform=linux/amd64 alpine:3.19.1

# update alpine linux dependencies
RUN apk update

WORKDIR /template-web-server

# copy binary
COPY --from=binary_builder /template-web-server/target/release/server .

# copy scripts
COPY --from=js_builder /template-web-server/bin/static/script/ static/script/

# copy stylesheets
COPY --from=css_builder /template-web-server/bin/static/stylesheet/ static/stylesheet/

# copy non-generative static assets
COPY ui/html/ html/
COPY ui/static/file/ static/file/
COPY ui/static/image/ static/image/

# run server
EXPOSE 8080
ENTRYPOINT ["./server"]

#ENTRYPOINT ["tail", "-f", "/dev/null"]
