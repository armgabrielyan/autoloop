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
  version "0.1.3" # x-release-please-version

  on_macos do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "cba6042af71f9c41dec40ed9f856645f68c60168c195b88d913cad642263fc39"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "e7aa7c4f12bf4c6a526885f113b8eb027da064b260f3b0488a5614722886ae91"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "89293b227e717269c91931143c4ec397695e13547cdfacf6cbf5e11b8beee417"
    end

    on_arm do
      url "https://github.com/armgabrielyan/autoloop/releases/download/v#{version}/autoloop-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a744a0455fd9a390ac7c13cb2ab0bae980776c65ba7ecbcaabe6a9f66e0b2a3a"
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
