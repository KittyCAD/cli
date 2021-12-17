# cli

The KittyCAD command line tool.

### Installing

Instructions for installing are on the [latest Release](https://github.com/KittyCAD/cli/releases).

### Acknowledgements

A lot of this is based on the excellent work by the GitHub team on the `gh` cli.

## How we document our command line syntax

### Literal text

Use plain text for parts of the command that cannot be changed.

_example:_
`kittycad help`
The argument help is required in this command.

### Placeholder values

Use angled brackets to represent a value the user must replace. No other expressions can be contained within the angled brackets.

_example:_
`kittycad file status <file-id>`
Replace `<file-id>` with a file id.

## Optional arguments

Place optional arguments in square brackets. Mutually exclusive arguments can be included inside square brackets if they are separated with vertical bars.

_example:_
`kittycad auth login [--web]`
The argument `--web` is optional.

`kittycad meta view [<number> | <url>]`
The `<number>` and `<url>` arguments are optional.

### Required mutually exclusive arguments

Place required mutually exclusive arguments inside braces, separate arguments with vertical bars.

_example:_
`kittycad file {get | create}`

### Repeatable arguments

Ellipsis represent arguments that can appear multiple times.

_example:_
`kittycad meta rm <some-number>...`

### Variable naming

For multi-word variables use dash-case (all lower case with words separated by dashes)

_example:_
`kittycad meta subcmd <multiple-words>`

### Additional examples

_optional argument with placeholder:_
`command sub-command [<arg>]`

_required argument with mutually exclusive options:_
`command sub-command {<path> | <string> | literal}`

_optional argument with mutually exclusive options:_
`command sub-command [<path> | <string>]`

