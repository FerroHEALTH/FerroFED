# Build and preview the Antora site.
#
#   make            Build, then serve at http://localhost:8080 (default target)
#   make build      Build the site into build/site
#   make serve      Serve an already-built site
#   make clean      Remove the build output
#
# Override the port with:  make PORT=9000

PORT ?= 8080
PLAYBOOK := antora-playbook.yml
OUT := build/site

.PHONY: all build serve clean install

all: build serve

# node_modules is the install stamp: reinstall only when the lockfile is newer.
node_modules: package-lock.json
	npm install
	@touch node_modules

install: node_modules

# --fetch refreshes the remote UI bundle. It requires network access; drop it
# for offline builds (the bundle is cached under .cache/ after the first run).
build: node_modules
	npx antora --fetch $(PLAYBOOK)

serve:
	@test -d $(OUT) || { echo "No build found at $(OUT) - run 'make build' first."; exit 1; }
	@echo "Serving $(OUT) at http://localhost:$(PORT)/ - Ctrl-C to stop."
	@python3 -m http.server $(PORT) --directory $(OUT)

clean:
	rm -rf build
