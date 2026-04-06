class MagicCode < Formula
  desc "Open-source TUI agentic AI coding agent"
  homepage "https://github.com/kienbui1995/mc-code"
  version "0.7.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/kienbui1995/mc-code/releases/download/v#{version}/magic-code-darwin-arm64"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/kienbui1995/mc-code/releases/download/v#{version}/magic-code-darwin-amd64"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/kienbui1995/mc-code/releases/download/v#{version}/magic-code-linux-arm64"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/kienbui1995/mc-code/releases/download/v#{version}/magic-code-linux-amd64"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install Dir["magic-code*"].first => "magic-code"
  end

  test do
    assert_match "magic-code", shell_output("#{bin}/magic-code --version")
  end
end
