class Kittycad < Formula
  desc " is a command-line interface to KittyCAD for use in your terminal or your scripts."
  homepage "https://kittycad.io/docs/cli/manual"
  url "https://dl.kittycad.io/releases/cli/replace-semver/kittycad-cli.tar.gz"
  sha256 "replace-tarball-sha"


  # specify the target architectures for the binary files
  bottle do
    sha256 cellar: :any_skip_relocation, x86_64_darwin:  "replace-x86_64_darwin-sha"
    sha256 cellar: :any_skip_relocation, aarch64_darwin: "replace-aarch64_darwin-sha"
    sha256 cellar: :any_skip_relocation, x86_64_linux:   "replace-x86_64_linux-sha"
    sha256 cellar: :any_skip_relocation, aarch64_linux:  "replace-aarch64_linux-sha"
  end

  def install
    # check if the user is using Linux and their hardware and install the appropriate binary
    if OS.linux?
      if Hardware::CPU.type == :intel
        bin.install "x86_64_linux/kittycad"
      elsif Hardware::CPU.type == :arm
        bin.install "aarch64_linux/kittycad"
      end
    else
      if Hardware::CPU.type == :intel
        bin.install "x86_64_darwin/kittycad"
      elsif Hardware::CPU.type == :arm
        bin.install "aarch64_darwin/kittycad"
      end
    end
  end
end
