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
gen_js: ## generates Javascript from Typescript
	# generate/clean bin scripts
	rm -rf bin/static/script
	mkdir -p bin/static/script

	# generate Javascript from Typescript
	deno bundle ui/static/script/scitylana.ts bin/static/script/scitylana.js

.PHONY: gen_css
gen_css: ## generate CSS from SCSS
	# generate/clean bin stylesheets
	rm -rf bin/static/stylesheet
	mkdir -p bin/static/stylesheet

	# generate CSS from SCSS
	sass --style=compressed --no-source-map --color --unicode ui/static/scss:bin/static/stylesheet

.PHONY: gen_static
gen_static: ## generates static resources
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

.PHONY: build
build: ## builds the binary locally
	cargo build --package template-web-server --bin template-web-server

.PHONY: dev
dev: build gen_js gen_css gen_static ## runs the development binary locally
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
lint: ## lints the codebase using rustfmt and Clippy
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

.PHONY: docs
docs: ## generates documentation
	deno doc --html --name="webserver-base" ./ui/static/script/mod.ts

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing libraries to crates.io and JSR
	echo "\033[1;35m[Packaging Rust]\033[0m"
	cargo publish --package webserver-base --dry-run
	cargo package --list
	echo "\033[1;35m[Packaging Typescript]\033[0m"
	deno publish --dry-run
	echo "\033[1;35m[Finished Dry-Run Publish]\033[0m"

.PHONY: build_docker
build_docker: ## builds Docker container
	docker build --tag goddtriffin/template-web-server:latest --file ./Dockerfile .

.PHONY: run_docker
run_docker: build_docker ## runs a new Docker container
	docker run \
	--name "template_web_server" \
	-d --restart unless-stopped \
	-p 8080:8080 \
	-e ENVIRONMENT="development" \
	-e HOST="0.0.0.0" \
	-e PROJECT_NAME="template-web-server" \
    -e PROJECT_DESCRIPTION="Here is a description of the project." \
    -e PROJECT_KEYWORDS="Todd,Everett,Griffin,todo,project" \
    -e HOME_URL="https://www.template-web-server.com" \
    -e ANALYTICS_DOMAIN="test.toddgriffin.me" \
	-e UPTIME_DOMAIN="https://uptime.toddgriffin.me" \
	-e SENTRY_DSN=${SENTRY_DSN} \
	goddtriffin/template-web-server

.PHONY: start_docker
start_docker: ## resumes a stopped Docker container
	docker start template_web_server

.PHONY: stop_docker
stop_docker: ## stops the Docker container
	docker stop template_web_server

.PHONY: remove_docker
remove_docker: stop_docker ## removes the Docker container
	docker rm template_web_server

.PHONY: push_docker
push_docker: ## pushes new Docker image to Docker Hub
	# tag
	docker tag goddtriffin/template-web-server:latest goddtriffin/minesweeper-royale-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/rlhandbook-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/scannable-codes-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/turnbased-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/scribble-jump-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/triple-entendre-website:latest
	docker tag goddtriffin/template-web-server:latest goddtriffin/video-game-recipe-book-website:latest

	# push
	docker push goddtriffin/minesweeper-royale-website:latest
	docker push goddtriffin/rlhandbook-website:latest
	docker push goddtriffin/scannable-codes-website:latest
	docker push goddtriffin/turnbased-website:latest
	docker push goddtriffin/scribble-jump-website:latest
	docker push goddtriffin/triple-entendre-website:latest
	docker push goddtriffin/video-game-recipe-book-website:latest

.PHONY: restart_deployment
restart_deployment: ## restarts all pods in the k8s deployment
	kubectl rollout restart deployment minesweeper-royale-website
	kubectl rollout restart deployment rlhandbook-website
	kubectl rollout restart deployment scannable-codes-website
	kubectl rollout restart deployment turnbased-website

.PHONY: deploy
deploy: build_docker push_docker restart_deployment ## builds/pushes new docker image at :latest and restarts k8s deployment

.PHONY: mem_usage
mem_usage: ## displays the memory usage of the currently running Docker container
	docker stats template_web_server --no-stream --format "{{.Container}}: {{.MemUsage}}"

.PHONY: logs
logs: ## displays logs from the currently running Docker container
	docker logs template_web_server
