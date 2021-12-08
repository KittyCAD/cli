DOCKER_IMAGE_NAME := kittycad/go-generator

INTERACTIVE := $(shell [ -t 0 ] && echo 1 || echo 0)
ifeq ($(INTERACTIVE), 1)
	DOCKER_FLAGS += -t
endif

.PHONY: generate
generate: docker-image
	docker run --rm -i $(DOCKER_FLAGS) \
		--name go-generator \
		-v $(CURDIR):/usr/src \
		--workdir /usr/src \
		$(DOCKER_IMAGE_NAME) oapi-codegen \
			-generate types,client \
			--config ./config.yml \
			-o ./kittycad.go \
			https://api.kittycad.io

.PHONY: docker-image
docker-image:
	docker build -t $(DOCKER_IMAGE_NAME) .


.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
