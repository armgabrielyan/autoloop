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
  version "0.1.1" # x-release-please-version

  on_macos do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "f875426a1f6b6e71076a35b9687139982a5a8d76edf6ea68a6b8cf493769eeaf"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "7e65c857cc3554913b60b808b64f8cbce5a6cb81e25d4d90c2f48b2b993471dc"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "2f380d15578e9f50eb9c54af860e6160b01d68531d9aa03a6348a158a516dd14"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "d886c01beee648507300c36d75dca33190696c7a48755f843589721c9ad1da9d"
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
