#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 || $# -gt 5 ]]; then
	echo "usage: $0 <target> <version> <name> <sha256> [download-base-url]" >&2
	exit 2
fi

target=$1
version=$2
name=$3
sha256=$4
download_base_url=${5:-${ZOO_CLI_DOWNLOAD_BASE_URL:-https://dl.zoo.dev/releases/cli}}
download_base_url=${download_base_url%/}
download_url="${download_base_url}/${version}/${name}-${target}"

cat <<EOF
### ${target}

EOF

case "${target}" in
	*-pc-windows-*)
		cat <<EOF
\`\`\`powershell
# Export the sha256sum for verification.
\$ZOO_CLI_SHA256 = "${sha256}"

# Define where to install the Zoo CLI.
\$zooPath = Join-Path \$env:LOCALAPPDATA "Zoo"
\$cliPath = Join-Path \$zooPath "zoo.exe"

# Create the directory for the Zoo CLI.
New-Item -ItemType Directory -Force -Path \$zooPath | Out-Null

# Download and check the sha256sum.
Invoke-WebRequest -Uri "${download_url}" -OutFile \$cliPath
\$actualHash = (Get-FileHash -Algorithm SHA256 \$cliPath).Hash.ToLowerInvariant()
if (\$actualHash -ne \$ZOO_CLI_SHA256.ToLowerInvariant()) {
    throw "Checksum verification failed for \$cliPath"
}

function Test-PathContains {
    param(
        [string]\$PathValue,
        [string]\$Directory
    )

    if ([string]::IsNullOrWhiteSpace(\$PathValue)) {
        return \$false
    }

    \$directoryToFind = \$Directory.TrimEnd('\')
    foreach (\$entry in \$PathValue -split ';') {
        if (\$entry.TrimEnd('\') -ieq \$directoryToFind) {
            return \$true
        }
    }

    return \$false
}

# Add the Zoo CLI directory to your user Path if needed.
\$userPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if (-not (Test-PathContains -PathValue \$userPath -Directory \$zooPath)) {
    \$pathEntries = @()
    if (-not [string]::IsNullOrWhiteSpace(\$userPath)) {
        \$pathEntries = \$userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace(\$_) }
    }
    \$pathEntries += \$zooPath
    [Environment]::SetEnvironmentVariable("Path", (\$pathEntries -join ';'), [EnvironmentVariableTarget]::User)
}

# Make the new Path available in this PowerShell session.
if (-not (Test-PathContains -PathValue \$env:Path -Directory \$zooPath)) {
    \$env:Path = ((@(\$env:Path, \$zooPath) | Where-Object { -not [string]::IsNullOrWhiteSpace(\$_) }) -join ';')
}

Write-Host "${name} cli installed!"

# Run it!
${name} -h
\`\`\`

EOF
		;;
	*)
		cat <<EOF
\`\`\`console
# Export the sha256sum for verification.
\$ export ZOO_CLI_SHA256="${sha256}"


# Download and check the sha256sum.
\$ curl -fSL "${download_url}" -o "/usr/local/bin/${name}" \\
	&& echo "\${ZOO_CLI_SHA256}  /usr/local/bin/${name}" | sha256sum -c - \\
	&& chmod a+x "/usr/local/bin/${name}"


\$ echo "${name} cli installed!"

# Run it!
\$ ${name} -h
\`\`\`

EOF
		;;
esac
