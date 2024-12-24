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

.PHONY: watch_sass
watch_sass: ## hot reloads Sass stylesheets
	sass --watch --update --style=compressed --no-source-map --color --unicode ui/static/scss:bin/static/stylesheet

.PHONY: gen_js
gen_js: # generates Javascript from Typescript
	# generate/clean bin scripts
	rm -rf bin/static/script
	mkdir -p bin/static/script

	# generate Javascript from Typescript
	deno run \
		--allow-read \
		--allow-write \
		--allow-env \
		--allow-net \
		--allow-run \
		ui/static/script/bundle.ts

.PHONY: gen_css
gen_css: # generate CSS from SCSS
	# generate/clean bin stylesheets
	rm -rf bin/static/stylesheet
	mkdir -p bin/static/stylesheet

	# generate CSS from SCSS
	sass --style=compressed --no-source-map --color --unicode ui/static/scss:bin/static/stylesheet

.PHONY: gen_static
gen_static: # generates static resources
	# generate/clean bin
	rm -rf bin/assets/
	rm -rf bin/html/
	rm -rf bin/static/file/
	rm -rf bin/static/image/
	mkdir -p bin/static

	# copy over ui: html, files, images
	cp -R ui/html bin/html
	cp -R ui/static/file bin/static/file
	cp -R ui/static/image bin/static/image

.PHONY: dev
dev: build gen_js gen_css gen_static ## runs the development binary
	cargo build --package template-web-server --bin template-web-server
	cp target/debug/template-web-server bin/
	cd bin && \
		ENVIRONMENT="development" \
		PROJECT_NAME="template-web-server" \
		PROJECT_DESCRIPTION="Here is a description of the project." \
		PROJECT_KEYWORDS="Todd,Everett,Griffin,todo,project" \
		HOME_URL="https://www.template-web-server.com" \
		ANALYTICS_DOMAIN="test.toddgriffin.me" \
		UPTIME_DOMAIN="https://uptime.toddgriffin.me" \
		./template-web-server

.PHONY: lint
lint: ## lints the codebase
	cargo fmt
	deno lint ui/static/script/
	deno doc --lint ui/static/script/
	deno fmt ui/static/script/

.PHONY: test
test: ## runs tests
	cargo fmt --check
	cargo check
	cargo clippy --tests
	cargo test

.PHONY: fix
fix: ## fixes the codebase
	cargo fix --allow-dirty --allow-staged
	cargo clippy --fix --allow-dirty --allow-staged

.PHONY: docs
docs: ## generates local documentation
	deno doc --html --name="webserver-base" ./ui/static/script/mod.ts

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing libraries to crates.io and JSR
	echo "\033[1;35m[Packaging Rust]\033[0m"
	cargo publish --package webserver-base --dry-run
	cargo package --list
	echo "\033[1;35m[Packaging Typescript]\033[0m"
	deno publish --dry-run
	echo "\033[1;35m[Finished Dry-Run Publish]\033[0m"

.PHONY: docker_build
docker_build: ## builds Docker container
	docker build \
		--platform linux/amd64 \
		--tag goddtriffin/template-web-server:latest \
		--file Dockerfile \
		.

.PHONY: docker_run
docker_run: ## runs Docker containers
	docker compose up -d

.PHONY: docker_stop
docker_stop: ## stops Docker containers
	docker compose down

.PHONY: docker_logs
docker_logs: ## displays Docker logs
	docker compose logs template_web_server -f

.PHONY: docker_mem_usage
docker_mem_usage: ## displays the memory usage of the currently running Docker containers
	docker stats template_web_server --no-stream --format "{{.Container}}: {{.MemUsage}}"

.PHONY: docker_push
docker_push: ## pushes Docker images to Docker Hub
	# tag
	docker tag goddtriffin/template-web-server:latest goddtriffin/rlhandbook-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/scannable-codes-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/turnbased-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/scribble-jump-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/video-game-recipe-book-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/vogue-bot-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/palms-small-engine-website:latest

	# push
	docker push goddtriffin/rlhandbook-website:latest
	docker push goddtriffin/scannable-codes-website:latest
	docker push goddtriffin/turnbased-website:latest
	docker push goddtriffin/scribble-jump-website:latest
	docker push goddtriffin/video-game-recipe-book-website:latest
	docker push goddtriffin/vogue-bot-website:latest
	docker push goddtriffin/palms-small-engine-website:latest
