# kittycad.go

The Golang API client for KittyCAD.

This is generated from
[deepmap/oapi-codegen](https://github.com/deepmap/oapi-codegen).

## Generating

You can trigger a build with the GitHub action to generate the client. This will
automatically update the client to the latest version based on the spec hosted
at [api.kittycad.io](https://api.kittycad.io/).

Alternatively, if you wish to generate the client locally, make sure you have
[Docker installed](https://docs.docker.com/get-docker/) and run:

```bash
$ make generate
```

## Contributing

Please do not change the code directly since it is generated. PRs that change
the code directly will be automatically closed by a bot.
