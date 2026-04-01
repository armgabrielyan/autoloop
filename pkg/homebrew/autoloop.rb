# Homebrew Formula for autoloop
#
# This formula is available via:
#   brew install armgabrielyan/tap/autoloop
#
# Or build from source:
#   brew install --HEAD armgabrielyan/tap/autoloop

class Autoloop < Formula
  desc "Agent-agnostic iterative optimization CLI"
  homepage "https://github.com/armgabrielyan/autoloop"
  license "MIT"
  version "0.1.0" # x-release-please-version

  on_macos do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  head do
    url "https://github.com/armgabrielyan/autoloop.git", branch: "main"
    depends_on "rust" => :build
  end

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "autoloop"
    end
  end

  test do
    assert_match "autoloop", shell_output("#{bin}/autoloop --version")
  end
end
