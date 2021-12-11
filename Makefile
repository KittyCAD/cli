# Setup name variables for the package/tool
NAME := cli
PKG := github.com/kittycad/$(NAME)

CGO_ENABLED := 0

# Set any default go build tags.
BUILDTAGS :=

include basic.mk

.PHONY: prebuild
prebuild:

.PHONY: gen-docs
gen-docs: gen-website gen-man ## Generate all the docs.

.PHONY: gen-website
gen-website: ## Generate the website documentation.
	go run $(CURDIR)/gen-docs/main.go --doc-path generated_docs/website --website

.PHONY: gen-man
gen-man: ## Generate the man pages.
	go run $(CURDIR)/gen-docs/main.go --doc-path generated_docs/man --man-page
