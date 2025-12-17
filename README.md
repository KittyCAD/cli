# cli

The Zoo command line tool.

The instructions below refer to instructions for contributing to the repo.

For the CLI docs for end users refer to: https://zoo.dev/docs/cli/manual

### Installing

On MacOS, you can use Homebrew:
```
brew tap kittycad/kittycad
brew install kittycad
zoo --help   
```
For all other installs, see the instructions on the [latest release](https://github.com/KittyCAD/cli/releases).

### Updating the API spec

Updating the API spec is as simple as updating the [`spec.json`](spec.json) file. The macro will take it from there when
you `cargo build`. It likely might need some tender love and care to make it a nice command like the other generated ones
if it is out of the ordinary.

Only `create`, `edit`, `view/get`, `list`, `delete` commands are generated. The rest are bespoke and any generation lead to something
that seemed harder to maintain over time. But if you are brave you can try.

For examples of the macro formatting, checkout some of the commands under `src/` like `cmd_file` or `cmd_user`.

**Note:** If you update the API spec here, you will likely want to bump the spec for the [kittycad.rs](https://github.com/KittyCAD/kittycad.rs)
repo as well since that is where the API client comes from.

### Running the tests

The tests use the `ZOO_TEST_TOKEN`  variables for knowing how to authenticate.

### Releasing a new version

1. Make sure the `Cargo.toml` has the new version you want to release.
2. When upgrading Zoo crates, make sure to update them _all_, even transitive dependencies.
    ```
    cargo update -p kittycad-modeling-cmds -p kcl-derive-docs -p kcl-error -p kcl-lib -p kcl-test-server
    cargo check
    ```
3. Run `make tag` this is just an easy command for making a tag formatted
   correctly with the version.
4. Push the tag (the result of `make tag` gives instructions for this)
5. Everything else is triggered from the tag push. Just make sure all the tests
   and cross compilation pass on the `main` branch before making and pushing
   a new tag.

### Building

To build, simply run `cargo build` like usual.

Make sure to update to the latest stable rustc: for example, if you use `rustup`, run `rustup update`.

#### Cross compiling

If you're on Debian or Ubuntu, install the required dependencies by running `.github/workflows/cross-deps.sh`. Otherwise, look there to see what packages are required.

Then, simply run `make`. Binaries will be available in `cross/`.

If you want to only build one of the cross targets, supply the `CROSS_TARGETS` environment variable:

    CROSS_TARGETS=x86_64-unknown-linux-musl make


If you get an error about md5sum on mac when running `make release`, you probably need to `brew install coreutils`

