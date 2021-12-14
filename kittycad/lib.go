package kittycad

//go:generate go run generate/generate.go

// DefaultServerURL is the default server URL for the KittyCad API.
const DefaultServerURL = "https://api.kittycad.io"

// TokenEnvVar is the environment variable that contains the token.
const TokenEnvVar = "KITTYCAD_API_TOKEN"
