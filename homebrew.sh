#!/bin/bash

mkdir homebrew
cp homebrew-template.rb homebrew/kittycad.rb

input_names=(
  "x86_64-apple-darwin"
  "aarch64-apple-darwin"
  # "x86_64-unknown-linux-musl"
  # "aarch64-unknown-linux-musl"
)
homebrew_names=(
  "x86_64_darwin"
  "aarch64_darwin"
  # "x86_64_linux"
  # "aarch64_linux"
)

to_zip_files=""

# iterate over the input names
for i in "${!input_names[@]}"; do
  input_name="${input_names[$i]}"
  homebrew_name="${homebrew_names[$i]}"
  
  mkdir "./homebrew/$homebrew_name"
  cp "./cross/kittycad-$input_name" "./homebrew/$homebrew_name/kittycad"

  # create variable with the sha256sum of the file without the filename
  sha256=$(sha256sum "./homebrew/$homebrew_name/kittycad")
  hash=$(printf '%s\n' "$sha256" | cut -d' ' -f1)
  sed -i '' "s#replace-$homebrew_name-sha#$hash#g" "./homebrew/kittycad.rb"

  # concat the strings homebrew/x86_64_darwin/kittycad into a variable to_zip_files
  to_zip_files="$to_zip_files $homebrew_name/kittycad"

done


(cd ./homebrew && tar -czvf kittycad-cli.tar.gz $to_zip_files)

sha256=$(sha256sum "./homebrew/kittycad-cli.tar.gz")
hash=$(printf '%s\n' "$sha256" | cut -d' ' -f1)
sed -i '' "s#replace-tarball-sha#$hash#g" "./homebrew/kittycad.rb"

version=v$(toml get Cargo.toml package.version | jq -r .)
echo $version
sed -i '' "s#replace-semver#$version#g" "./homebrew/kittycad.rb"

for homebrew_name in "${homebrew_names[@]}"; do
  rm -rf "./homebrew/$homebrew_name"
done