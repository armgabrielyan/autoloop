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
  version "0.1.2" # x-release-please-version

  on_macos do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "46b8b1039c73c41b8dc7e52a333b5cd52f3a4535bba5f949015e8b97e882ee2a"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "6f4951b36ce498e822d30b251c6e2dcaae0668f7bec1fc223269cd9b0f466f90"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "9a7a73fda141e02e1dd903c7b215d33ce2c363553a27bea7a2aff8c6e718c721"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "3a1fb762e6a11ca382568f67daa3f2c82b2057ed9e2c852646038735280c1c77"
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
