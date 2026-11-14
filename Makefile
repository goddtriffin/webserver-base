$(VERBOSE).SILENT:
.DEFAULT_GOAL := help

.PHONY: help
help: # Prints out help
	@IFS=$$'\n' ; \
	help_lines=(`fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##/:/'`); \
	printf "%-30s %s\n" "target" "help" ; \
	printf "%-30s %s\n" "------" "----" ; \
	for help_line in $${help_lines[@]}; do \
			IFS=$$':' ; \
			help_split=($$help_line) ; \
			help_command=`echo $${help_split[0]} | sed -e 's/^ *//' -e 's/ *$$//'` ; \
			help_info=`echo $${help_split[2]} | sed -e 's/^ *//' -e 's/ *$$//'` ; \
			printf '\033[36m'; \
			printf "%-30s %s" $$help_command ; \
			printf '\033[0m'; \
			printf "%s\n" $$help_info; \
	done
	@echo

# Paths. The repo root holds one directory per language. The template web
# server is a whole site (crate + html/ + static/), so its directory is the
# site root the asset pipeline runs in: every pipeline command `cd`s there
# first, and every `rm` is relative to it, so a failed `cd` stops the recipe
# before anything is removed.
RUST := rust
TS := typescript
TWS := rust/template_web_server
# The TypeScript package, seen from inside $(TWS).
TS_FROM_TWS := ../../typescript

.PHONY: watch_sass
watch_sass: ## hot reloads Sass stylesheets
	cd $(TWS) && sass --watch --update --style=compressed --no-source-map --color --unicode static/scss:bin/static/stylesheet

.PHONY: gen_js
gen_js: # generates Javascript from Typescript
	# generate/clean bin scripts
	cd $(TWS) && rm -rf bin/static/script && mkdir -p bin/static/script

	# generate Javascript from Typescript. The bundler reads static/script/ and
	# writes bin/static/script/ relative to the working directory (the site
	# root); its own imports resolve through the TypeScript package's config.
	cd $(TWS) && deno run \
		--config $(TS_FROM_TWS)/deno.jsonc \
		--allow-read \
		--allow-write \
		--allow-env \
		--allow-net \
		--allow-run \
		$(TS_FROM_TWS)/webserver-base/bundle.ts

.PHONY: gen_css
gen_css: # generate CSS from SCSS
	# generate/clean bin stylesheets
	cd $(TWS) && rm -rf bin/static/stylesheet && mkdir -p bin/static/stylesheet

	# generate CSS from SCSS
	cd $(TWS) && sass --style=compressed --no-source-map --color --unicode static/scss:bin/static/stylesheet

.PHONY: gen_static
gen_static: # copies the non-generated static resources into bin
	# generate/clean bin
	cd $(TWS) && rm -rf bin/assets/ bin/html/ bin/static/file/ bin/static/image/ && rm -f bin/cache-buster.json
	cd $(TWS) && mkdir -p bin/static

	# copy over: html, files, images
	cd $(TWS) && cp -R html bin/html
	cd $(TWS) && { [ -d static/file ] && cp -R static/file bin/static/file || true; }
	cd $(TWS) && cp -R static/image bin/static/image

.PHONY: gen_static_assets
gen_static_assets: # derives the icon set, then hashes everything but the scripts
	# The server binary owns this: no shim binary, no per-project glue. It
	# generates favicon.ico / apple-touch-icon / icon-192 / icon-512 from
	# favicon.svg, hashes every non-script asset, and writes cache-buster.json
	# plus the TypeScript module the JS build inlines.
	cd $(TWS)/bin && WSB_ENVIRONMENT=local ./template-web-server gen-static-assets

	# The bundler reads the source tree, so the generated module has to exist
	# there too. Both copies are build output; neither is committed.
	cd $(TWS) && mkdir -p static/script/generated && cp bin/static/script/generated/cache-buster.ts static/script/generated/cache-buster.ts

.PHONY: gen_static_scripts
gen_static_scripts: # hashes the built JavaScript, once it exists
	cd $(TWS)/bin && WSB_ENVIRONMENT=local ./template-web-server gen-static-scripts

.PHONY: assets
assets: ## builds the template site into rust/template_web_server/bin/, which dev serves and the tests drive
	# Order matters. Icons and hashing must precede the JS build, because the
	# bundler inlines the manifest; the scripts are hashed after they exist.
	cd $(RUST) && cargo build --package template-web-server --bin template-web-server
	$(MAKE) gen_static
	$(MAKE) gen_css
	mkdir -p $(TWS)/bin && cp $(RUST)/target/debug/template-web-server $(TWS)/bin/
	$(MAKE) gen_static_assets
	$(MAKE) gen_js
	$(MAKE) gen_static_scripts

.PHONY: dev
dev: assets ## runs the template web server's development binary
	set -e; \
	PORT="$${PORT:-$$(cd $(TS) && deno run --allow-net --allow-run=lsof webserver-base/free-port.ts template-web-server)}"; \
	echo "==> http://127.0.0.1:$$PORT"; \
	cd $(TWS)/bin && \
		WSB_PORT="$$PORT" \
		WSB_ENVIRONMENT="local" \
		TWS_PROJECT="Template Web Server" \
		TWS_DESCRIPTION="Here is a description of the project." \
		TWS_BASE_URL="https://www.template-web-server.com" \
		./template-web-server

.PHONY: lint
lint: ## lints the codebase
	cd $(TS) && deno lint webserver-base/
	cd $(TS) && deno doc --lint webserver-base/
	cd $(TS) && deno fmt webserver-base/

	cd $(RUST) && cargo fmt

.PHONY: test
test: assets ## runs tests; the template's tests serve the bin/ that assets builds
	cd $(TS) && deno check webserver-base/
	# --allow-net: free-port's tests bind real sockets to verify port detection
	cd $(TS) && deno test --allow-net

	cd $(RUST) && cargo fmt --check
	cd $(RUST) && cargo check --all-features
	cd $(RUST) && cargo clippy --workspace --all-targets --all-features -- -D warnings
	cd $(RUST) && cargo test --all-features

.PHONY: fix
fix: ## fixes the codebase
	cd $(RUST) && cargo fix --all-features --allow-dirty --allow-staged
	cd $(RUST) && cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

.PHONY: docs
docs: ## generates local documentation
	cd $(TS) && deno doc --html --name="webserver-base" ./webserver-base/

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing libraries to crates.io and JSR
	echo "\033[1;35m[Packaging Rust]\033[0m"
	cd $(RUST) && cargo publish --package webserver-base --dry-run
	cd $(RUST) && cargo package --package webserver-base --list

	echo "\033[1;35m[Packaging Typescript]\033[0m"
	cd $(TS) && deno publish --dry-run --allow-dirty

	echo "\033[1;35m[Finished Dry-Run Publish]\033[0m"

.PHONY: docker_build
docker_build: ## builds the template web server's Docker image (build context: the repo root)
	docker build \
		--platform linux/amd64 \
		--tag goddtriffin/template-web-server:latest \
		--file $(TWS)/Dockerfile \
		.

.PHONY: docker_run
docker_run: ## runs Docker containers
	set -e; \
	HOST_PORT="$${HOST_PORT:-$$(cd $(TS) && deno run --allow-net --allow-run=lsof webserver-base/free-port.ts template-web-server)}"; \
	echo "==> http://127.0.0.1:$$HOST_PORT"; \
	cd $(TWS) && HOST_PORT="$$HOST_PORT" docker compose up -d

.PHONY: docker_stop
docker_stop: ## stops Docker containers
	cd $(TWS) && docker compose down

.PHONY: docker_logs
docker_logs: ## displays Docker logs
	cd $(TWS) && docker compose logs template_web_server -f

.PHONY: docker_mem_usage
docker_mem_usage: ## displays the memory usage of the currently running Docker containers
	docker stats template_web_server --no-stream --format "{{.Container}}: {{.MemUsage}}"

.PHONY: docker_push
docker_push: ## pushes Docker images to Docker Hub
	# tag
	docker tag goddtriffin/template-web-server:latest goddtriffin/scannable-codes-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/turnbased-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/scribble-jump-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/video-game-recipe-book-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/5dcheckers-server:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/5ddiplomacy-server:latest

	# push
	docker push goddtriffin/scannable-codes-website:latest
	docker push goddtriffin/turnbased-website:latest
	docker push goddtriffin/scribble-jump-website:latest
	docker push goddtriffin/video-game-recipe-book-website:latest
	docker push goddtriffin/5dcheckers-server:latest
	docker push goddtriffin/5ddiplomacy-server:latest
